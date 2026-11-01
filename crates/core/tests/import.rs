use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use erc_convert::{Converter, Kepubify};
use erc_core::device::{books_on_device, sync_annotations, sync_reading};
use erc_core::import::{DeviceImport, import_from_device};
use erc_device_kobo::KoboDevice;
use erc_library::Library;
use rusqlite::Connection;
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";
const KEPUB: &str = "KEpub/Fantasy/Mini - Test Author.kepub.epub";
const DRM: &str = "KEpub/Locked.epub";
const PDF: &str = "Comics/Strip.pdf";
const STORE: &str = "0a1b2c3d-store-book";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub")
        .join(name)
}

fn cid(path: &str) -> String {
    format!("file:///mnt/onboard/{path}")
}

/// `minimal.epub` plus an Adobe `rights.xml`.
fn drm_epub(dest: &Path) {
    let mut src = ZipArchive::new(File::open(fixture("minimal.epub")).unwrap()).unwrap();
    let mut out = zip::ZipWriter::new(File::create(dest).unwrap());
    for i in 0..src.len() {
        out.raw_copy_file(src.by_index_raw(i).unwrap()).unwrap();
    }
    out.start_file("META-INF/rights.xml", SimpleFileOptions::default())
        .unwrap();
    out.write_all(b"<rights/>").unwrap();
    out.finish().unwrap();
}

/// ContentID, MimeType, user, title, series, series number, Accessibility.
type Row<'a> = (
    String,
    &'a str,
    &'a str,
    &'a str,
    Option<&'a str>,
    Option<&'a str>,
    i64,
);

/// A Kobo with a kepubified sideload (series set by calibre), a DRM-protected
/// sideload, a PDF and a store book, as Nickel would list them.
fn fake_kobo(root: &Path) -> KoboDevice {
    let mount = root.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    for path in [KEPUB, DRM, PDF] {
        fs::create_dir_all(mount.join(path).parent().unwrap()).unwrap();
    }
    Kepubify::locate()
        .unwrap()
        .convert(&fixture("minimal.epub"), &mount.join(KEPUB))
        .unwrap();
    drm_epub(&mount.join(DRM));
    fs::write(mount.join(PDF), b"%PDF-1.4").unwrap();

    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute_batch(include_str!(
        "../../../tests/fixtures/kobo/schema-db174.sql"
    ))
    .unwrap();
    let insert = "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Attribution, Series, SeriesNumber, Accessibility, IsDownloaded)
                  VALUES (?1, 6, ?2, ?3, ?4, 'Test Author', ?5, ?6, ?7, 'true')";
    let rows: [Row; 4] = [
        (
            cid(KEPUB),
            "application/x-kobo-epub+zip",
            "kepub_user",
            "ERC Test Book",
            Some("Test Saga"),
            Some("3"),
            -1,
        ),
        (
            cid(DRM),
            "application/epub+zip",
            "adobe_user",
            "Locked",
            None,
            None,
            -1,
        ),
        (
            cid(PDF),
            "application/pdf",
            "pdf_user",
            "Strip",
            None,
            None,
            -1,
        ),
        (
            STORE.to_owned(),
            "application/x-kobo-epub+zip",
            "user",
            "Store Book",
            None,
            None,
            1,
        ),
    ];
    for (id, mime, user, title, series, number, access) in rows {
        conn.execute(
            insert,
            rusqlite::params![id, mime, user, title, series, number, access],
        )
        .unwrap();
    }
    drop(conn);
    KoboDevice::open(&mount).unwrap()
}

fn outcome_for<'a>(
    outcomes: &'a [erc_core::import::DeviceImportOutcome],
    id: &str,
) -> &'a DeviceImport {
    outcomes
        .iter()
        .find(|o| o.content_id == id)
        .unwrap()
        .result
        .as_ref()
        .unwrap()
}

fn skipped(o: &DeviceImport) -> &str {
    match o {
        DeviceImport::Skipped { reason } => reason,
        other => panic!("expected a skip, got {other:?}"),
    }
}

#[test]
fn imports_sideloads_cleaned_and_skips_the_rest() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let db_before = fs::read(kobo.db_path()).unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();

    let mut progress = Vec::new();
    let outcomes =
        import_from_device(&mut library, &kobo, None, |i, n, _| progress.push((i, n))).unwrap();
    assert_eq!(progress, [(1, 4), (2, 4), (3, 4), (4, 4)]);

    let DeviceImport::Added { book_id } = *outcome_for(&outcomes, &cid(KEPUB)) else {
        panic!("{outcomes:?}");
    };
    assert!(skipped(outcome_for(&outcomes, &cid(DRM))).contains("DRM"));
    assert!(skipped(outcome_for(&outcomes, &cid(PDF))).contains("not an EPUB"));
    assert!(skipped(outcome_for(&outcomes, STORE)).contains("Kobo store"));

    let book = library.book(book_id).unwrap();
    assert_eq!(book.title, "ERC Test Book");
    assert_eq!(book.original_name, "Mini - Test Author.epub");
    // Nickel's series (written by calibre) is kept since the EPUB has none.
    assert_eq!(book.series.as_deref(), Some("Test Saga"));
    assert_eq!(book.series_index.as_deref(), Some("3"));

    // The stored copy is a plain EPUB.
    let mut zip = ZipArchive::new(File::open(library.book_file(book_id)).unwrap()).unwrap();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).unwrap();
        let mut text = String::new();
        let _ = entry.read_to_string(&mut text);
        assert!(!text.contains("koboSpan"), "{}", entry.name().unwrap());
    }

    // It shows as on the Kobo, up to date, without being sent again.
    let on = books_on_device(&library, &kobo).unwrap();
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].content_id, cid(KEPUB));
    assert!(!on[0].stale);

    // Read-only: the device database is untouched and nothing was recorded.
    assert_eq!(fs::read(kobo.db_path()).unwrap(), db_before);
    assert!(!kobo.mount().join(".erc").exists());

    // A second run finds nothing new.
    let again = import_from_device(&mut library, &kobo, Some(&[cid(KEPUB)]), |_, _, _| {}).unwrap();
    assert_eq!(again.len(), 1);
    assert_eq!(
        skipped(outcome_for(&again, &cid(KEPUB))),
        "already in your library"
    );
    assert_eq!(library.books().unwrap().len(), 1);
}

#[test]
fn links_books_the_library_already_has() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let erc_library::ImportOutcome::Added { book, .. } =
        library.import(&fixture("minimal.epub")).unwrap()
    else {
        panic!();
    };

    let outcomes =
        import_from_device(&mut library, &kobo, Some(&[cid(KEPUB)]), |_, _, _| {}).unwrap();
    assert_eq!(
        *outcome_for(&outcomes, &cid(KEPUB)),
        DeviceImport::Linked { book_id: book.id }
    );
    assert_eq!(library.books().unwrap().len(), 1, "no duplicate");
    assert_eq!(
        books_on_device(&library, &kobo).unwrap()[0].book_id,
        book.id
    );
}

#[test]
fn reading_progress_reaches_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let outcomes =
        import_from_device(&mut library, &kobo, Some(&[cid(KEPUB)]), |_, _, _| {}).unwrap();
    let DeviceImport::Added { book_id } = *outcome_for(&outcomes, &cid(KEPUB)) else {
        panic!();
    };
    assert_eq!(library.reading(book_id).unwrap(), None);

    Connection::open(kobo.db_path())
        .unwrap()
        .execute(
            "UPDATE content SET ReadStatus = 1, ___PercentRead = 40, DateLastRead = '2026-10-30T20:00:00Z', TimeSpentReading = 3600 WHERE ContentID = ?1",
            [cid(KEPUB)],
        )
        .unwrap();
    assert_eq!(sync_reading(&library, &kobo).unwrap(), 1);
    let r = library.reading(book_id).unwrap().unwrap();
    assert_eq!(
        (
            r.status.as_str(),
            r.percent,
            r.time_spent_secs,
            r.device_serial.as_str()
        ),
        ("reading", 40, Some(3600), "N000000000001")
    );
    assert!(r.last_read.unwrap().starts_with("2026-10-30T20:00:00"));
    // Nothing changed since: nothing written.
    assert_eq!(sync_reading(&library, &kobo).unwrap(), 0);
}

#[test]
fn annotations_are_copied_and_kept_after_deletion() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let outcomes =
        import_from_device(&mut library, &kobo, Some(&[cid(KEPUB)]), |_, _, _| {}).unwrap();
    let DeviceImport::Added { book_id } = *outcome_for(&outcomes, &cid(KEPUB)) else {
        panic!();
    };
    let conn = Connection::open(kobo.db_path()).unwrap();
    for (id, text, note) in [("h1", "A line.", None), ("h2", "Another.", Some("Mine"))] {
        conn.execute(
            "INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex, StartOffset,
                 EndContainerPath, EndContainerChildIndex, EndOffset, Text, Annotation, ChapterProgress, Hidden, Type)
             VALUES (?1, ?2, ?3, 'p', 0, 0, 'p', 0, 1, ?4, ?5, 0.5, 'false', 'highlight')",
            rusqlite::params![id, cid(KEPUB), format!("/mnt/onboard/{KEPUB}!!c1.xhtml"), text, note],
        )
        .unwrap();
    }
    let annots = kobo.mount().join("Digital Editions/Annotations/KEpub");
    fs::create_dir_all(&annots).unwrap();
    fs::write(annots.join("Plain.epub.annot"), b"<annotationSet/>").unwrap();

    assert_eq!(sync_annotations(&mut library, &kobo).unwrap(), 2);
    assert_eq!(
        sync_annotations(&mut library, &kobo).unwrap(),
        0,
        "nothing new"
    );
    let stored = library.annotations(book_id).unwrap();
    assert_eq!(stored.len(), 2);
    assert_eq!(stored[1].kind, "note");
    assert_eq!(library.annotation_counts().unwrap()[&book_id], 2);
    assert!(
        dir.path()
            .join("lib/annotation-files/N000000000001/KEpub/Plain.epub.annot")
            .is_file()
    );

    // Deleted on the Kobo: kept in the library, marked as removed there.
    conn.execute("DELETE FROM Bookmark WHERE BookmarkID = 'h1'", [])
        .unwrap();
    assert_eq!(sync_annotations(&mut library, &kobo).unwrap(), 1);
    let stored = library.annotations(book_id).unwrap();
    assert_eq!(stored.len(), 2);
    assert!(!stored.iter().find(|a| a.id == "h1").unwrap().on_device);
}
