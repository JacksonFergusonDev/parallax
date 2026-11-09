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

/// A Kobo database with no books; ERC only writes to a Kobo it can read.
fn empty_database(mount: &Path) {
    rusqlite::Connection::open(mount.join(".kobo/KoboReader.sqlite"))
        .unwrap()
        .execute_batch(include_str!(
            "../../../tests/fixtures/kobo/schema-db174.sql"
        ))
        .unwrap();
}

#[test]
fn converts_and_sends_each_book_independently() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    empty_database(&mount);
    let kobo = KoboDevice::open(&mount).unwrap();

    let broken = dir.path().join("Broken.epub");
    fs::write(&broken, b"not an epub").unwrap();
    let sources = vec![fixture("minimal.epub"), broken];

    let mut events = Vec::new();
    let outcomes = send_books(&kobo, &Kepubify::locate().unwrap(), &sources, |p| {
        events.push(p);
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
    empty_database(&mount);
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
                contributors: None,
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

    library.remove(book.id).unwrap();
    let catalog =
        erc_core::device::device_catalog(&library, &kobo, &kobo.open_db().unwrap()).unwrap();
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].book.id, sent.content_id);
    assert!(catalog[0].pending_import);
}

#[test]
fn updates_and_removes_books_on_device() {
    use erc_core::device::{books_on_device, device_catalog, remove_from_device_with_progress};
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
    let catalog =
        |library: &Library| device_catalog(library, &kobo, &kobo.open_db().unwrap()).unwrap();
    let pending = catalog(&library);
    assert_eq!(pending.len(), 1);
    assert!(pending[0].pending_import);
    assert_eq!(pending[0].book.id, content_id);
    assert_eq!(pending[0].book.title, book.title);
    // Nickel imports the book on eject.
    Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Accessibility, IsDownloaded, ___PercentRead)
             VALUES (?1, 6, 'application/x-kobo-epub+zip', 'kepub_user', 'ERC Test Book', -1, 'true', 55)",
            [&content_id],
        )
        .unwrap();
    let imported = catalog(&library);
    assert_eq!(imported.len(), 1);
    assert!(!imported[0].pending_import);
    assert_eq!(imported[0].book.id, content_id);
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

    let mut removal = Vec::new();
    let removed = remove_from_device_with_progress(
        Some(&library),
        &kobo,
        &backups,
        std::slice::from_ref(&content_id),
        |completed, total, fraction| removal.push((completed, total, fraction)),
    )
    .unwrap();
    assert!(removed[0].result.is_ok());
    assert_eq!(removed[0].book_id, Some(book.id));
    assert_eq!(removal.first(), Some(&(0, 1, 0.0)));
    assert!(removal.iter().any(|&(completed, total, fraction)| {
        completed == 0 && total == 1 && fraction > 0.0 && fraction < 1.0
    }));
    assert_eq!(removal.last(), Some(&(1, 1, 0.0)));
    assert!(books_on_device(&library, &kobo).unwrap().is_empty());
    assert!(!mount.join(&on_device[0].device_path).exists());
    assert!(catalog(&library).is_empty());

    let backup = kobo.backups(&backups)[0].file.clone();
    kobo.restore_backup(&backups, Path::new(&backup)).unwrap();
    assert_eq!(
        books_on_device(&library, &kobo).unwrap()[0].book_id,
        book.id
    );
    assert!(!catalog(&library)[0].pending_import);
}

#[test]
fn sends_into_collection_and_applies_series_after_import() {
    use erc_core::collections::{CollectionChange, apply, pending_series, reconcile_series};
    use erc_core::device::device_catalog;
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
    let catalog = device_catalog(&library, &kobo, &kobo.open_db().unwrap()).unwrap();
    assert_eq!(catalog.len(), 1);
    assert!(catalog[0].pending_import);
    assert_eq!(catalog[0].book.collections, ["Favourites"]);

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
    use erc_core::rules::{Trigger, plan, run};
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

    // Runs the app starts itself never create collections: nothing exists
    // yet, so nothing is written.
    assert_eq!(
        run(&library, &kobo, &backups, Trigger::Automatic).unwrap(),
        0
    );
    assert!(!backups.exists());
    assert_eq!(run(&library, &kobo, &backups, Trigger::User).unwrap(), 3);
    // Pre-import rows count as done: nothing more to plan, no new write session.
    assert!(plan(&rules, &library, &kobo).unwrap().is_empty());
    let sessions = fs::read_dir(backups.join("N000000000001")).unwrap().count();
    assert_eq!(
        run(&library, &kobo, &backups, Trigger::Automatic).unwrap(),
        0
    );
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

    // Once a collection exists, automatic runs file new books into it.
    Connection::open(&db)
        .unwrap()
        .execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Attribution, Accessibility, IsDownloaded)
             VALUES ('file:///mnt/onboard/KEpub/new.kepub.epub', 6, 'application/x-kobo-epub+zip', 'kepub_user', 'New Book', 'Ann Author', -1, 'true')",
            [],
        )
        .unwrap();
    run(&library, &kobo, &backups, Trigger::Automatic).unwrap();
    let names: Vec<(String, usize)> = kobo
        .open_db()
        .unwrap()
        .collections()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.book_count))
        .collect();
    assert_eq!(
        names,
        [("#Favourites".to_owned(), 1), ("Ann Author".to_owned(), 3)]
    );
}

fn make_comic(path: &Path) {
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    let mut image = Cursor::new(Vec::new());
    image::GrayImage::from_pixel(1500, 2000, image::Luma([150]))
        .write_to(&mut image, image::ImageFormat::Png)
        .unwrap();
    let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
    zip.start_file("01.png", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(image.get_ref()).unwrap();
    zip.finish().unwrap();
}

fn comic_kobo(mount: &Path, model: &str) -> KoboDevice {
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(
        mount.join(".kobo/version"),
        format!("{}{model}", &VERSION[..VERSION.len() - 3]),
    )
    .unwrap();
    empty_database(mount);
    KoboDevice::open(mount).unwrap()
}

#[test]
fn comic_sends_follow_three_kobo_screen_profiles() {
    use std::io::Read;
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("Storm v02.cbz");
    make_comic(&source);
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&source).unwrap() else {
        panic!()
    };
    let converter = Kepubify::locate().unwrap();
    for (model, expected) in [
        ("310", (600, 800)),
        ("376", (1072, 1429)),
        ("388", (1260, 1680)),
    ] {
        let mount = dir.path().join(model);
        let kobo = comic_kobo(&mount, model);
        let mut events = Vec::new();
        let sent = send_library_books(&library, &kobo, &converter, &[book.id], |p| events.push(p));
        let info = sent[0].result.as_ref().unwrap();
        assert!(events.iter().any(|p| matches!(
            p,
            Progress::ProcessingComic {
                page: 1,
                pages: 1,
                ..
            }
        )));
        let file = fs::File::open(mount.join(&info.device_path)).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut png = Vec::new();
        zip.by_name("OEBPS/images/p00001.png")
            .unwrap()
            .read_to_end(&mut png)
            .unwrap();
        let image = image::load_from_memory(&png).unwrap();
        assert_eq!((image.width(), image.height()), expected);
    }
}

fn send_update_comic_on(mount: &Path, root: &Path) {
    use erc_core::collections::reconcile_series;
    use erc_core::device::books_on_device;
    use erc_core::send::{COMIC_SPREADS, update_on_device};
    use rusqlite::Connection;

    let kobo = comic_kobo(mount, "388");
    let source = root.join("Storm! v02.cbz");
    make_comic(&source);
    let mut library = Library::open(root.join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&source).unwrap() else {
        panic!()
    };
    let backups = root.join("backups");
    let converter = Kepubify::locate().unwrap();
    let estimate = erc_core::comic_send::estimate(&library, &kobo, &[book.id])
        .unwrap()
        .unwrap();
    assert_eq!((estimate.width, estimate.height), (1264, 1680));
    assert!(estimate.bytes > 0 && estimate.free > estimate.bytes);
    let sent = send_library_books(&library, &kobo, &converter, &[book.id], |_| {});
    let sent = sent[0].result.as_ref().unwrap();
    assert!(sent.device_path.starts_with("KEpub/Storm_ v02 - "));
    let db = kobo.db_path();
    Connection::open(&db).unwrap().execute(
        "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Accessibility, IsDownloaded, ___PercentRead)
         VALUES (?1, 6, 'application/x-kobo-epub+zip', 'kepub_user', 'Storm', -1, 'true', 55)",
        [&sent.content_id],
    ).unwrap();
    assert_eq!(reconcile_series(&library, &kobo, &backups).unwrap(), 1);
    assert!(!books_on_device(&library, &kobo).unwrap()[0].stale);

    library
        .set_setting(COMIC_SPREADS, Some("whole_then_halves"))
        .unwrap();
    assert!(books_on_device(&library, &kobo).unwrap()[0].stale);
    let outcome =
        update_on_device(&library, &kobo, &converter, &backups, &[book.id], |_| {}).unwrap();
    assert_eq!(
        outcome[0].result.as_ref().unwrap().device_path,
        sent.device_path
    );
    assert!(!books_on_device(&library, &kobo).unwrap()[0].stale);
    let (series, index, series_id, percent): (String, String, String, i64) = Connection::open(&db).unwrap().query_row(
        "SELECT Series, SeriesNumber, SeriesID, ___PercentRead FROM content WHERE ContentID = ?1",
        [&sent.content_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    ).unwrap();
    assert_eq!(
        (series.as_str(), index.as_str(), series_id.as_str(), percent),
        ("Storm!", "2", "Storm!", 55)
    );
    assert!(mount.join(&sent.device_path).is_file());
    assert!(
        !fs::read_dir(mount.join("KEpub"))
            .unwrap()
            .flatten()
            .any(|e| e.file_name().to_string_lossy().starts_with('.'))
    );
}

#[test]
fn comic_update_preserves_path_and_progress() {
    let dir = tempfile::tempdir().unwrap();
    send_update_comic_on(&dir.path().join("kobo"), dir.path());
}

#[cfg(target_os = "macos")]
#[test]
#[ignore = "mounts a disk image: just test-fat"]
fn sends_and_updates_a_comic_on_fat32() {
    use std::process::Command;

    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("comic.dmg");
    let mount = dir.path().join("kobo");
    fs::create_dir(&mount).unwrap();
    let run = |cmd: &str, args: &[&str]| {
        let out = Command::new(cmd).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{cmd}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let image = image.to_str().unwrap();
    let mount_str = mount.to_str().unwrap();
    run(
        "diskutil",
        &[
            "image",
            "create",
            "blank",
            "--format",
            "RAW",
            "--size",
            "40m",
            "--volumeName",
            "KOBOTEST",
            "--fs",
            "MS-DOS",
            image,
        ],
    );
    run(
        "hdiutil",
        &["attach", "-nobrowse", "-mountpoint", mount_str, image],
    );
    let result = std::panic::catch_unwind(|| send_update_comic_on(&mount, dir.path()));
    run("hdiutil", &["detach", mount_str]);
    result.unwrap();
}

#[test]
fn comics_added_from_the_kobo_are_never_replaced() {
    use erc_core::device::books_on_device;
    use erc_core::send::{COMIC_SPREADS, update_on_device};

    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("kobo");
    let kobo = comic_kobo(&mount, "388");
    // The comic as it was on the Kobo, linked as "Add from Kobo" does.
    let rel = "Comics/Storm v02.cbz";
    let on_kobo = mount.join(rel);
    fs::create_dir_all(on_kobo.parent().unwrap()).unwrap();
    make_comic(&on_kobo);
    let original = fs::read(&on_kobo).unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&on_kobo).unwrap() else {
        panic!()
    };
    let content_id = format!("file:///mnt/onboard/{rel}");
    library
        .record_device_copy(book.id, &kobo.version().serial, rel, &content_id)
        .unwrap();

    library
        .set_setting(COMIC_SPREADS, Some("keep_whole"))
        .unwrap();
    let mut edit = book.edit();
    edit.title = "Storm, edited".into();
    library.update(book.id, &edit).unwrap();
    assert!(!books_on_device(&library, &kobo).unwrap()[0].stale);

    let converter = Kepubify::locate().unwrap();
    let backups = dir.path().join("backups");
    let outcome =
        update_on_device(&library, &kobo, &converter, &backups, &[book.id], |_| {}).unwrap();
    assert!(
        outcome[0]
            .result
            .as_ref()
            .unwrap_err()
            .contains("can't be updated")
    );
    assert_eq!(fs::read(&on_kobo).unwrap(), original);
}

#[test]
fn a_guessed_screen_changing_does_not_mark_comics_out_of_date() {
    use erc_core::device::books_on_device;

    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("kobo");
    // A model Parallax doesn't know, with no covers yet: the screen is assumed.
    let kobo = comic_kobo(&mount, "999");
    let source = dir.path().join("Storm v02.cbz");
    make_comic(&source);
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&source).unwrap() else {
        panic!()
    };
    let converter = Kepubify::locate().unwrap();
    let sent = send_library_books(&library, &kobo, &converter, &[book.id], |_| {});
    assert!(sent[0].result.is_ok());
    assert!(!books_on_device(&library, &kobo).unwrap()[0].stale);

    // Nickel makes a cover, so the screen is now guessed from it instead.
    let images = mount.join(".kobo-images/12/34");
    fs::create_dir_all(&images).unwrap();
    image::GrayImage::new(1264, 1680)
        .save_with_format(images.join("abc - N3_FULL.parsed"), image::ImageFormat::Png)
        .unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();
    assert_ne!(
        (
            kobo.version().screen(&mount).width,
            kobo.version().screen(&mount).height
        ),
        (1072, 1448)
    );
    assert!(!books_on_device(&library, &kobo).unwrap()[0].stale);
}
