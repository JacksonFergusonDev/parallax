use std::fs;
use std::path::{Path, PathBuf};

use erc_core::duplicates::{
    LIBRARY_SCOPE, Reason, device_duplicates, device_scope, library_duplicates,
};
use erc_device_kobo::KoboDevice;
use erc_epub::Epub;
use erc_library::{ImportOutcome, Library};
use rusqlite::{Connection, params};

const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub")
        .join(name)
}

fn added(outcome: ImportOutcome) -> i64 {
    match outcome {
        ImportOutcome::Added { book, .. } => book.id,
        other @ ImportOutcome::AlreadyInLibrary { .. } => panic!("{other:?}"),
    }
}

#[test]
fn library_groups_suggest_keeping_the_richer_copy() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let plain = added(library.import(&fixture("minimal.epub")).unwrap());
    // Same book, different file: a tagged copy with a subtitle.
    let epub = Epub::open(fixture("minimal.epub")).unwrap();
    let mut edit = library.book(plain).unwrap().edit();
    edit.title = "ERC Test Book: Special Edition".into();
    edit.tags = vec!["Fiction".into()];
    let copy = dir.path().join("copy.epub");
    epub.write_with_metadata(&edit, &copy).unwrap();
    let tagged = added(library.import(&copy).unwrap());

    let groups = library_duplicates(&library).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].reasons, [Reason::SameTitleAndAuthor]);
    let remove: Vec<(i64, bool)> = groups[0]
        .members
        .iter()
        .map(|m| (m.book_id, m.remove))
        .collect();
    assert_eq!(remove, [(plain, true), (tagged, false)]);

    // A copy on the Kobo is never suggested for removal.
    library
        .record_device_copy(
            plain,
            "N1",
            "KEpub/a.kepub.epub",
            "file:///mnt/onboard/KEpub/a.kepub.epub",
        )
        .unwrap();
    let groups = library_duplicates(&library).unwrap();
    assert!(
        groups[0]
            .members
            .iter()
            .all(|m| !m.remove || m.book_id != plain)
    );

    // "Not duplicates" hides the group.
    library
        .dismiss_duplicates(LIBRARY_SCOPE, &groups[0].key)
        .unwrap();
    assert!(library_duplicates(&library).unwrap().is_empty());
}

/// Path (or store id), title, author, ReadStatus, percent, file bytes.
type Row<'a> = (&'a str, &'a str, &'a str, i64, i64, Option<&'a [u8]>);

#[test]
fn device_groups_formats_and_protect_progress() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::create_dir_all(mount.join("KEpub")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute_batch(include_str!(
        "../../../tests/fixtures/kobo/schema-db174.sql"
    ))
    .unwrap();
    let books: [Row; 6] = [
        (
            "KEpub/Dune.kepub.epub",
            "Dune",
            "Frank Herbert",
            0,
            0,
            Some(b"kepub"),
        ),
        (
            "KEpub/Dune.epub",
            "Dune (Dune Chronicles, Book 1)",
            "Herbert, Frank",
            1,
            30,
            Some(b"epub!"),
        ),
        (
            "KEpub/Dune copy.epub",
            "Dune copy",
            "Frank Herbert",
            0,
            0,
            Some(b"epub!"),
        ),
        ("store-dune", "Dune", "Frank Herbert", 0, 0, None),
        (
            "KEpub/Emma.kepub.epub",
            "Emma",
            "Jane Austen",
            0,
            0,
            Some(b"emma"),
        ),
        (
            "KEpub/Emma 2.kepub.epub",
            "Emma",
            "Jane Austen",
            0,
            0,
            Some(b"emma 2"),
        ),
    ];
    for (path, title, author, status, pct, bytes) in books {
        let (id, access, user) = match bytes {
            Some(b) => {
                fs::write(mount.join(path), b).unwrap();
                (format!("file:///mnt/onboard/{path}"), -1, "kepub_user")
            }
            None => (path.to_owned(), 1, "store_user"),
        };
        conn.execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Attribution, ReadStatus, ___PercentRead, Accessibility, IsDownloaded)
             VALUES (?1, 6, 'application/epub+zip', ?2, ?3, ?4, ?5, ?6, ?7, 'true')",
            params![id, user, title, author, status, pct, access],
        )
        .unwrap();
    }
    drop(conn);
    let kobo = KoboDevice::open(&mount).unwrap();
    let library = Library::open(dir.path().join("lib")).unwrap();

    let groups = device_duplicates(&library, &kobo).unwrap();
    assert_eq!(groups.len(), 2, "{groups:#?}");
    let dune = groups
        .iter()
        .find(|g| g.members.len() == 4)
        .expect("all Dune copies, the identical file included");
    assert_eq!(dune.reasons, [Reason::SameFile, Reason::SameTitleAndAuthor]);
    let summary: Vec<(&str, &str, bool, bool)> = dune
        .members
        .iter()
        .map(|m| {
            (
                m.content_id.rsplit('/').next().unwrap(),
                m.format.as_str(),
                m.removable,
                m.remove,
            )
        })
        .collect();
    let mut summary = summary;
    summary.sort_unstable();
    assert_eq!(
        summary,
        [
            ("Dune copy.epub", "EPUB", true, true),
            // Being read: kept.
            ("Dune.epub", "EPUB", true, false),
            ("Dune.kepub.epub", "KEPUB", true, true),
            ("store-dune", "Kobo store", false, false),
        ]
    );

    let emma = groups.iter().find(|g| g.members.len() == 2).unwrap();
    assert_eq!(emma.members.iter().filter(|m| m.remove).count(), 1);
    library
        .dismiss_duplicates(&device_scope("N000000000001"), &emma.key)
        .unwrap();
    assert_eq!(device_duplicates(&library, &kobo).unwrap().len(), 1);
}
