use std::fs;
use std::path::{Path, PathBuf};

use erc_convert::Kepubify;
use erc_core::send::{Progress, send_books, send_library_books};
use erc_device_kobo::KoboDevice;
use erc_epub::{Epub, MetadataEdit};
use erc_library::{ImportOutcome, Library};

const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub")
        .join(name)
}

#[test]
fn converts_and_sends_each_book_independently() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();

    let broken = dir.path().join("Broken.epub");
    fs::write(&broken, b"not an epub").unwrap();
    let sources = vec![fixture("minimal.epub"), broken];

    let mut events = Vec::new();
    let outcomes = send_books(&kobo, &Kepubify::locate().unwrap(), &sources, |p| {
        events.push(p)
    });

    assert!(outcomes[0].result.is_ok());
    assert!(mount.join("KEpub/minimal.kepub.epub").is_file());
    assert!(outcomes[1].result.is_err());
    assert!(!mount.join("KEpub/Broken.kepub.epub").exists());
    assert!(matches!(
        events.first(),
        Some(Progress::Converting {
            index: 1,
            total: 2,
            ..
        })
    ));
    assert!(matches!(
        events.last(),
        Some(Progress::Failed { index: 2, .. })
    ));

    // Sending the same book again is refused rather than overwriting.
    let again = send_books(&kobo, &Kepubify::locate().unwrap(), &sources[..1], |_| {});
    assert!(
        again[0]
            .result
            .as_ref()
            .unwrap_err()
            .contains("already on the device")
    );
}

#[test]
fn sends_library_books_with_edited_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&fixture("minimal.epub")).unwrap()
    else {
        panic!("fixture already imported");
    };
    library
        .update(
            book.id,
            &MetadataEdit {
                title: "Edited: Title".into(),
                authors: vec!["Ann Author".into()],
                series: Some("Saga".into()),
                series_index: Some("1".into()),
                tags: vec![],
            },
        )
        .unwrap();

    let outcomes = send_library_books(
        &library,
        &kobo,
        &Kepubify::locate().unwrap(),
        &[book.id],
        |_| {},
    );
    let sent = outcomes[0].result.as_ref().unwrap();
    assert_eq!(
        sent.device_path,
        "KEpub/Edited_ Title - Ann Author.kepub.epub"
    );

    // The KEPUB on the device carries the edited metadata.
    let on_device = Epub::open(mount.join(&sent.device_path))
        .unwrap()
        .metadata();
    assert_eq!(on_device.title.as_deref(), Some("Edited: Title"));
    assert_eq!(on_device.series.as_deref(), Some("Saga"));

    // Renaming and sending again is refused rather than leaving two copies.
    let mut edit = library.book(book.id).unwrap().edit();
    edit.title = "Renamed".into();
    library.update(book.id, &edit).unwrap();
    let again = send_library_books(
        &library,
        &kobo,
        &Kepubify::locate().unwrap(),
        &[book.id],
        |_| {},
    );
    assert!(
        again[0]
            .result
            .as_ref()
            .unwrap_err()
            .contains("already on the device")
    );

    let copies = library.device_copies("N000000000001").unwrap();
    assert_eq!(copies[0].book_id, book.id);
    assert_eq!(
        copies[0].content_id,
        format!("file:///mnt/onboard/{}", sent.device_path)
    );
}

#[test]
fn updates_and_removes_books_on_device() {
    use erc_core::device::{books_on_device, remove_from_device};
    use erc_core::send::update_on_device;
    use rusqlite::Connection;

    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let db = mount.join(".kobo/KoboReader.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(include_str!(
        "../../../tests/fixtures/kobo/schema-db174.sql"
    ))
    .unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    let kobo = KoboDevice::open(&mount).unwrap();
    let backups = dir.path().join("backups");
    let converter = Kepubify::locate().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&fixture("minimal.epub")).unwrap()
    else {
        panic!();
    };

    let sent = send_library_books(&library, &kobo, &converter, &[book.id], |_| {});
    let content_id = sent[0].result.as_ref().unwrap().content_id.clone();
    // Nickel imports the book on eject.
    Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Accessibility, IsDownloaded, ___PercentRead)
             VALUES (?1, 6, 'application/x-kobo-epub+zip', 'kepub_user', 'ERC Test Book', -1, 'true', 55)",
            [&content_id],
        )
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(1100)); // timestamps have 1 s resolution
    let mut edit = library.book(book.id).unwrap().edit();
    edit.title = "Better Title".into();
    edit.series = Some("Saga".into());
    edit.series_index = Some("2".into());
    library.update(book.id, &edit).unwrap();
    assert!(books_on_device(&library, &kobo).unwrap()[0].stale);

    let updated =
        update_on_device(&library, &kobo, &converter, &backups, &[book.id], |_| {}).unwrap();
    assert!(updated[0].result.is_ok(), "{:?}", updated[0].result);
    let (title, series, pct): (String, String, i64) = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT Title, Series, ___PercentRead FROM content WHERE ContentID = ?1",
            [&content_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (title.as_str(), series.as_str(), pct),
        ("Better Title", "Saga", 55)
    );
    let on_device = books_on_device(&library, &kobo).unwrap();
    assert!(!on_device[0].stale);
    // Same file name as before: the path (and progress) is kept.
    assert_eq!(
        on_device[0].device_path,
        sent[0].result.as_ref().unwrap().device_path
    );

    let removed = remove_from_device(
        Some(&library),
        &kobo,
        &backups,
        std::slice::from_ref(&content_id),
    )
    .unwrap();
    assert!(removed[0].result.is_ok());
    assert!(books_on_device(&library, &kobo).unwrap().is_empty());
    assert!(!mount.join(&on_device[0].device_path).exists());
}

#[test]
fn sends_into_collection_and_applies_series_after_import() {
    use erc_core::collections::{CollectionChange, apply, pending_series, reconcile_series};
    use rusqlite::Connection;

    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let db = mount.join(".kobo/KoboReader.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(include_str!(
        "../../../tests/fixtures/kobo/schema-db174.sql"
    ))
    .unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    let kobo = KoboDevice::open(&mount).unwrap();
    let backups = dir.path().join("backups");
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&fixture("minimal.epub")).unwrap()
    else {
        panic!();
    };
    let mut edit = library.book(book.id).unwrap().edit();
    edit.series = Some("Saga".into());
    edit.series_index = Some("3".into());
    library.update(book.id, &edit).unwrap();

    // Send, then put it in a collection before Nickel has imported it.
    let sent = send_library_books(
        &library,
        &kobo,
        &Kepubify::locate().unwrap(),
        &[book.id],
        |_| {},
    );
    let content_id = sent[0].result.as_ref().unwrap().content_id.clone();
    apply(
        &kobo,
        &backups,
        &[CollectionChange::Add {
            name: "Favourites".into(),
            content_ids: vec![content_id.clone()],
        }],
    )
    .unwrap();
    // Not imported yet: nothing to reconcile.
    assert!(pending_series(&library, &kobo).unwrap().is_empty());

    // Nickel imports the book (without series, as it does for sideloads).
    Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Accessibility, IsDownloaded)
             VALUES (?1, 6, 'application/x-kobo-epub+zip', 'kepub_user', 'ERC Test Book', -1, 'true')",
            [&content_id],
        )
        .unwrap();
    let shown = kobo.open_db().unwrap().books().unwrap();
    assert_eq!(shown[0].collections, ["Favourites"]);
    assert_eq!(pending_series(&library, &kobo).unwrap().len(), 1);

    assert_eq!(reconcile_series(&library, &kobo, &backups).unwrap(), 1);
    let shown = kobo.open_db().unwrap().books().unwrap();
    assert_eq!(
        (
            shown[0].series.as_deref(),
            shown[0].series_number.as_deref()
        ),
        (Some("Saga"), Some("3"))
    );
    // Nothing pending: no write session (no new backup).
    let backups_before = fs::read_dir(backups.join("N000000000001")).unwrap().count();
    assert_eq!(reconcile_series(&library, &kobo, &backups).unwrap(), 0);
    assert_eq!(
        fs::read_dir(backups.join("N000000000001")).unwrap().count(),
        backups_before
    );
}

#[test]
fn rules_group_sent_books_before_and_after_import() {
    use erc_core::rules::{plan, run};
    use erc_library::{Rule, RuleKind};
    use rusqlite::Connection;

    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let db = mount.join(".kobo/KoboReader.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(include_str!(
        "../../../tests/fixtures/kobo/schema-db174.sql"
    ))
    .unwrap();
    // A calibre-era book already on the device, by the same author.
    conn.execute(
        "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Attribution, Accessibility, IsDownloaded)
         VALUES ('file:///mnt/onboard/KEpub/old.kepub.epub', 6, 'application/x-kobo-epub+zip', 'kepub_user', 'Old Book', 'Ann Author', -1, 'true')",
        [],
    )
    .unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    let kobo = KoboDevice::open(&mount).unwrap();
    let backups = dir.path().join("backups");
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&fixture("minimal.epub")).unwrap()
    else {
        panic!();
    };
    let mut edit = library.book(book.id).unwrap().edit();
    edit.authors = vec!["Ann Author".into()];
    edit.tags = vec!["Favourites".into()];
    library.update(book.id, &edit).unwrap();
    library
        .save_rule(&Rule {
            id: 0,
            kind: RuleKind::Author,
            enabled: true,
            min_books: 2,
            prefix: String::new(),
        })
        .unwrap();
    library
        .save_rule(&Rule {
            id: 0,
            kind: RuleKind::Tag,
            enabled: true,
            min_books: 1,
            prefix: "#".into(),
        })
        .unwrap();

    send_library_books(
        &library,
        &kobo,
        &Kepubify::locate().unwrap(),
        &[book.id],
        |_| {},
    );
    let rules = library.rules().unwrap();
    let planned = plan(&rules, &library, &kobo).unwrap();
    let summary: Vec<(&str, usize)> = planned
        .iter()
        .map(|p| (p.name.as_str(), p.content_ids.len()))
        .collect();
    assert_eq!(summary, [("Ann Author", 2), ("#Favourites", 1)]);

    assert_eq!(run(&library, &kobo, &backups).unwrap(), 3);
    // Pre-import rows count as done: nothing more to plan, no new write session.
    assert!(plan(&rules, &library, &kobo).unwrap().is_empty());
    let sessions = fs::read_dir(backups.join("N000000000001")).unwrap().count();
    assert_eq!(run(&library, &kobo, &backups).unwrap(), 0);
    assert_eq!(
        fs::read_dir(backups.join("N000000000001")).unwrap().count(),
        sessions
    );

    let collections: Vec<(String, usize)> = kobo
        .open_db()
        .unwrap()
        .collections()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.book_count))
        .collect();
    assert_eq!(
        collections,
        [("#Favourites".to_owned(), 1), ("Ann Author".to_owned(), 2)]
    );
}
