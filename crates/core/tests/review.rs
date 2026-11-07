//! Reviewing books just added.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use erc_core::duplicates::library_duplicates;
use erc_core::review::{AUTO_REVIEW_LIMIT, Issue, Review, review};
use erc_library::{ImportOutcome, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// A small EPUB with no cover. `manifest` is extra manifest items.
fn epub(path: &Path, title: &str, creator: &str, subject: &str, manifest: &str) {
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    {creator}
    {subject}
    <dc:identifier id="uid">urn:uuid:{title}</dc:identifier>
  </metadata>
  <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>{manifest}</manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
    );
    let xhtml = r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title></head><body><p>Text.</p></body></html>"#;
    let container = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, text) in [
        ("mimetype", "application/epub+zip"),
        ("META-INF/container.xml", container),
        ("content.opf", &opf),
        ("c1.xhtml", xhtml),
    ] {
        zip.start_file(name, stored).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

const AUTHOR: &str = "<dc:creator>An Author</dc:creator>";
const GENRE: &str = "<dc:subject>Fantasy</dc:subject>";

fn add(library: &mut Library, path: &Path) -> i64 {
    match library.import(path).unwrap() {
        ImportOutcome::Added { book, .. } => book.id,
        other @ ImportOutcome::AlreadyInLibrary { .. } => panic!("not imported: {other:?}"),
    }
}

fn run(library: &Library, ids: &[i64]) -> Review {
    let books: Vec<_> = ids.iter().map(|&id| library.book(id).unwrap()).collect();
    let files: Vec<_> = ids.iter().map(|&id| library.book_files(id)).collect();
    review(&books, &files, library_duplicates(library).unwrap())
}

fn issues(review: &Review, id: i64) -> Vec<Issue> {
    review
        .findings
        .iter()
        .filter(|f| f.book_id == id)
        .map(|f| f.issue)
        .collect()
}

#[test]
fn raises_what_can_be_fixed_and_nothing_else() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let bare = dir.path().join("bare.epub");
    epub(&bare, "my_scan_01", "", "", "");
    let tagged = dir.path().join("tagged.epub");
    epub(&tagged, "A Proper Title", AUTHOR, GENRE, "");
    let (bare, tagged) = (add(&mut library, &bare), add(&mut library, &tagged));

    let found = run(&library, &[bare, tagged]);
    assert_eq!(
        issues(&found, bare),
        [
            Issue::NoGenre,
            Issue::NoAuthor,
            Issue::TitleFromFile,
            Issue::NoCover
        ]
    );
    assert_eq!(issues(&found, tagged), [Issue::NoCover]);
    assert!(!found.bulk);
}

#[test]
fn a_missing_file_is_damage_but_a_broken_link_is_not_raised() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let path = dir.path().join("book.epub");
    epub(
        &path,
        "Damaged",
        AUTHOR,
        GENRE,
        r#"<item id="gone" href="gone.xhtml" media-type="application/xhtml+xml"/>"#,
    );
    let id = add(&mut library, &path);
    let found = run(&library, &[id]);
    let damage: Vec<_> = found
        .findings
        .iter()
        .filter(|f| f.issue == Issue::Damaged)
        .collect();
    assert_eq!(damage.len(), 1);
    assert!(damage[0].detail.contains("gone.xhtml"));
}

#[test]
fn only_duplicates_involving_these_books_are_raised() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let mut ids = Vec::new();
    for (name, title) in [("a", "Pair"), ("b", "Pair"), ("c", "Old"), ("d", "Old")] {
        let path = dir.path().join(format!("{name}.epub"));
        epub(&path, title, AUTHOR, GENRE, &format!("<!-- {name} -->"));
        ids.push(add(&mut library, &path));
    }
    let found = run(&library, &ids[1..2]);
    assert_eq!(found.duplicates.len(), 1);
    let members: Vec<i64> = found.duplicates[0]
        .members
        .iter()
        .map(|m| m.book_id)
        .collect();
    assert!(members.contains(&ids[0]) && members.contains(&ids[1]));
}

#[test]
fn more_books_than_the_limit_are_bulk() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let ids: Vec<i64> = (0..=AUTO_REVIEW_LIMIT)
        .map(|i| {
            let path = dir.path().join(format!("{i}.epub"));
            epub(&path, &format!("Book {i}"), AUTHOR, GENRE, "");
            add(&mut library, &path)
        })
        .collect();
    assert!(run(&library, &ids).bulk);
    assert!(!run(&library, &ids[1..]).bulk);
}
