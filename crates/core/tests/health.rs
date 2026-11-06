//! "Check library" across many books at once.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;

use erc_core::health::{Check, Finding, check_library};
use erc_library::{ImportOutcome, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const BOOKS: usize = 40;

/// A small EPUB with no cover and, as `author`, an optional creator.
fn epub(path: &Path, title: &str, author: &str) {
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>{title}</dc:title>
    <dc:creator>{author}</dc:creator>
    <dc:identifier id="uid">urn:uuid:{title}</dc:identifier>
  </metadata>
  <manifest><item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#
    );
    let xhtml = format!(
        r#"<?xml version="1.0" encoding="utf-8"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>{title}</title></head><body><p>Text.</p></body></html>"#
    );
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
        ("c1.xhtml", &xhtml),
    ] {
        zip.start_file(name, stored).unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
}

/// A library of `BOOKS` books; the book with index 7 has a damaged file.
fn library(dir: &Path) -> (Library, Vec<i64>) {
    let mut library = Library::open(dir.join("library")).unwrap();
    let ids: Vec<i64> = (0..BOOKS)
        .map(|i| {
            let path = dir.join(format!("{i}.epub"));
            epub(&path, &format!("Book {i}"), "An Author");
            match library.import(&path).unwrap() {
                ImportOutcome::Added { book, .. } => book.id,
                other @ ImportOutcome::AlreadyInLibrary { .. } => panic!("not imported: {other:?}"),
            }
        })
        .collect();
    fs::write(library.book_file(ids[7]), b"not a zip").unwrap();
    (library, ids)
}

fn of(findings: &[Finding], check: Check) -> Vec<i64> {
    findings
        .iter()
        .filter(|f| f.check == check)
        .flat_map(|f| f.book_ids.iter().copied())
        .collect()
}

#[test]
fn findings_belong_to_the_right_books_in_library_order() {
    let dir = tempfile::tempdir().unwrap();
    let (library, ids) = library(dir.path());
    let findings = check_library(&library, |_, _| {}).unwrap();

    assert_eq!(of(&findings, Check::Unreadable), [ids[7]]);
    // `books()` order, however the reads were scheduled.
    let listed: Vec<i64> = library.books().unwrap().iter().map(|b| b.id).collect();
    assert_eq!(of(&findings, Check::NoCover), listed);
    assert_eq!(findings, check_library(&library, |_, _| {}).unwrap());
}

#[test]
fn progress_counts_every_book_exactly_once() {
    let dir = tempfile::tempdir().unwrap();
    let (library, _) = library(dir.path());
    let seen = Mutex::new(Vec::new());
    check_library(&library, |done, total| {
        seen.lock().unwrap().push((done, total));
    })
    .unwrap();
    let expected: Vec<_> = (1..=BOOKS).map(|done| (done, BOOKS)).collect();
    assert_eq!(seen.into_inner().unwrap(), expected);
}

#[test]
fn an_empty_library_has_no_findings() {
    let dir = tempfile::tempdir().unwrap();
    let library = Library::open(dir.path().join("library")).unwrap();
    assert!(
        check_library(&library, |_, _| panic!("no books"))
            .unwrap()
            .is_empty()
    );
}
