use std::fs;
use std::path::Path;

use parallax_device_kobo::{AnnotationKind, KoboDb, KoboDevice};
use rusqlite::{Connection, params};

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";
const BOOK: &str = "file:///mnt/onboard/KEpub/Saga.kepub.epub";
const PATH: &str = "/mnt/onboard/KEpub/Saga.kepub.epub";

fn bookmark(
    conn: &Connection,
    id: &str,
    file: &str,
    progress: f64,
    (text, note): (Option<&str>, Option<&str>),
    kind: &str,
    hidden: &str,
) {
    conn.execute(
        "INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex, StartOffset,
             EndContainerPath, EndContainerChildIndex, EndOffset, Text, Annotation, ChapterProgress, Hidden, Type, DateCreated)
         VALUES (?1, ?2, ?3, 'span#kobo\\.1\\.1', 0, 0, 'span#kobo\\.1\\.2', 0, 4, ?4, ?5, ?6, ?7, ?8, '2026-10-30T20:00:00Z')",
        params![id, BOOK, format!("{PATH}!!{file}"), text, note, progress, hidden, kind],
    )
    .unwrap();
}

#[test]
fn reads_highlights_in_reading_order_with_chapter_titles() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    // Spine: title, part1 (two files), part2. Contents: Part One, Part Two.
    for (i, file) in ["title.xhtml", "part1.xhtml", "part1b.xhtml", "part2.xhtml"]
        .iter()
        .enumerate()
    {
        conn.execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, BookID, ___UserID, Title, VolumeIndex)
             VALUES (?1, 9, 'application/xhtml+xml', ?2, 'kepub_user', ?3, ?4)",
            params![format!("{PATH}!!{file}"), BOOK, file, i],
        )
        .unwrap();
    }
    for (i, (file, title)) in [("part1.xhtml-1", "Part One"), ("part2.xhtml-1", "Part Two")]
        .iter()
        .enumerate()
    {
        conn.execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, BookID, ___UserID, Title, VolumeIndex, Depth)
             VALUES (?1, 899, 'application/x-kobo-epub+zip', ?2, 'kepub_user', ?3, ?4, 1)",
            params![format!("{PATH}!!{file}"), BOOK, title, i],
        )
        .unwrap();
    }
    bookmark(
        &conn,
        "b3",
        "part2.xhtml",
        0.5,
        (Some("Later."), None),
        "highlight",
        "false",
    );
    bookmark(
        &conn,
        "b1",
        "part1.xhtml",
        0.9,
        (Some(" First.  "), Some("Why?")),
        "note",
        "false",
    );
    bookmark(
        &conn,
        "b2",
        "part1b.xhtml",
        0.1,
        (None, None),
        "dogear",
        "false",
    );
    bookmark(
        &conn,
        "b4",
        "title.xhtml",
        0.0,
        (Some("Front"), None),
        "highlight",
        "false",
    );
    bookmark(
        &conn,
        "gone",
        "part1.xhtml",
        0.2,
        (Some("Deleted"), None),
        "highlight",
        "true",
    );
    drop(conn);

    let kobo = KoboDevice::open(&mount).unwrap();
    let found = kobo.open_db().unwrap().annotations(Some(BOOK)).unwrap();
    let summary: Vec<(&str, AnnotationKind, Option<&str>, Option<&str>)> = found
        .iter()
        .map(|a| {
            (
                a.id.as_str(),
                a.kind,
                a.chapter.as_deref(),
                a.text.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        summary,
        [
            ("b4", AnnotationKind::Highlight, None, Some("Front")),
            ("b1", AnnotationKind::Note, Some("Part One"), Some("First.")),
            // A later file of the same part keeps the part's title.
            ("b2", AnnotationKind::Bookmark, Some("Part One"), None),
            (
                "b3",
                AnnotationKind::Highlight,
                Some("Part Two"),
                Some("Later.")
            ),
        ]
    );
    assert_eq!(found[1].note.as_deref(), Some("Why?"));
    assert!((found[1].position - 1.9).abs() < 1e-9);
    assert!(kobo.annotation_files().is_empty());

    // Adobe annotation files are found under Digital Editions.
    let annots = mount.join("Digital Editions/Annotations/Books");
    fs::create_dir_all(&annots).unwrap();
    fs::write(annots.join("Plain.epub.annot"), b"<annotationSet/>").unwrap();
    fs::write(annots.join("._Plain.epub.annot"), b"x").unwrap();
    let files = kobo.annotation_files();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, "Books/Plain.epub.annot");
}

#[test]
fn real_database_chapters_name_highlights() {
    let real = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/private/KoboReader-libra2-fw4.38-db174.sqlite");
    if !real.exists() {
        eprintln!("skipping: {} not present", real.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("KoboReader.sqlite");
    fs::copy(&real, &db).unwrap();
    // The real device has no highlights; add two to a real book's chapters.
    let conn = Connection::open(&db).unwrap();
    let book: String = conn
        .query_row(
            "SELECT ContentID FROM content WHERE ContentType = 6 AND Title LIKE 'Arcanum Unbounded%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let path = book.trim_start_matches("file://");
    for (id, file) in [
        ("x1", "OEBPS/xhtml/part2_split_001.xhtml"),
        ("x2", "OEBPS/xhtml/preface.xhtml"),
    ] {
        conn.execute(
            "INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex, StartOffset,
                 EndContainerPath, EndContainerChildIndex, EndOffset, Text, ChapterProgress, Hidden, Type)
             VALUES (?1, ?2, ?3, 'p', 0, 0, 'p', 0, 1, 'quote', 0.5, 'false', 'highlight')",
            params![id, book, format!("{path}!!{file}")],
        )
        .unwrap();
    }
    drop(conn);
    let found = KoboDb::open_read_only(&db)
        .unwrap()
        .annotations(None)
        .unwrap();
    let chapters: Vec<Option<&str>> = found.iter().map(|a| a.chapter.as_deref()).collect();
    assert_eq!(chapters, [Some("Preface"), Some("The Emperor’s Soul")]);
}
