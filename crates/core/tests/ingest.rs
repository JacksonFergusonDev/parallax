//! Adding Amazon books and bundles to the library.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use parallax_convert::amazon::InProcess;
use parallax_core::ingest::{Error, import_archive, import_file};
use parallax_library::{ImportOutcome, Library};
use zip::write::SimpleFileOptions;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/amazon")
        .join(name)
}

#[test]
fn an_amazon_book_is_stored_as_an_epub_and_only_added_once() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    let path = fixture("book.azw3");

    let outcome = import_file(&mut library, &InProcess, &path).unwrap();
    let ImportOutcome::Added { book, .. } = outcome else {
        panic!("expected a new book");
    };
    assert_eq!(book.title, "Amazon Fixture");
    assert_eq!(book.original_name, "book.azw3");
    assert!(book.has_cover);
    assert!(parallax_epub::Epub::open(library.book_file(book.id)).is_ok());

    let again = import_file(&mut library, &InProcess, &path).unwrap();
    assert!(matches!(again, ImportOutcome::AlreadyInLibrary { .. }));
    assert_eq!(library.books().unwrap().len(), 1);
}

#[test]
fn a_non_amazon_failure_is_reported_by_the_library() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path()).unwrap();
    let junk = dir.path().join("notes.azw3");
    fs::write(&junk, "not a book").unwrap();
    let err = import_file(&mut library, &InProcess, &junk).unwrap_err();
    assert!(matches!(err, Error::Library(_)), "{err}");
    let missing = dir.path().join("missing.azw3");
    assert!(import_file(&mut library, &InProcess, &missing).is_err());
}

#[test]
fn a_book_bundle_imports_supported_members_and_keeps_partial_results() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let path = dir.path().join("Books.ZIP");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    let epub = fs::read(fixture("../epub/minimal.epub")).unwrap();
    let kindle = fs::read(fixture("book.azw3")).unwrap();
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, bytes) in [
        ("README.txt", b"notes".as_slice()),
        ("nested/broken.epub", b"not an EPUB"),
        ("./nested/First.EPUB", &epub),
        ("../escape.epub", &epub),
        ("hidden/../escape.epub", &epub),
        (r"windows\..\escape.epub", &epub),
        ("__MACOSX/sidecar.epub", &epub),
        ("other/books.zip", &epub),
        (r"windows\Second.AZW3", &kindle),
        ("another/First.EPUB", &epub),
    ] {
        zip.start_file(name, options).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.add_symlink("link.epub", "nested/First.EPUB", options)
        .unwrap();
    zip.finish().unwrap();

    let mut seen = Vec::new();
    let results = import_archive(&mut library, &InProcess, &path, |name| {
        seen.push(name.to_owned());
    });
    assert_eq!(
        seen,
        ["broken.epub", "First.EPUB", "Second.AZW3", "First.EPUB"],
        "{results:?}"
    );
    assert_eq!(results.len(), 4);
    assert_eq!(results[0].0, path.join("nested/broken.epub"));
    assert!(results[0].1.is_err());
    for (result, name) in results[1..3].iter().zip(["First.EPUB", "Second.AZW3"]) {
        let ImportOutcome::Added { book, .. } = result.1.as_ref().unwrap() else {
            panic!("expected a new book");
        };
        assert_eq!(book.original_name, name);
        assert!(parallax_epub::Epub::open(library.book_file(book.id)).is_ok());
    }
    assert!(matches!(
        results[3].1,
        Ok(ImportOutcome::AlreadyInLibrary { .. })
    ));
    assert_eq!(library.books().unwrap().len(), 2);
    assert!(!dir.path().join("escape.epub").exists());
}

#[test]
fn an_archive_without_books_reports_why_nothing_was_added() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let path = dir.path().join("empty.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    zip.start_file("nested.7z", SimpleFileOptions::default())
        .unwrap();
    zip.finish().unwrap();

    let results = import_archive(&mut library, &InProcess, &path, |_| unreachable!());
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].0, path);
    assert!(
        results[0]
            .1
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("no supported book files")
    );
}

#[test]
fn rar_and_7z_bundles_import_books_from_the_native_reader() {
    for archive in ["books.rar", "books.7z"] {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::open(dir.path()).unwrap();
        let path = fixture("../book-archives").join(archive);
        let results = import_archive(&mut library, &InProcess, &path, |_| {});
        assert_eq!(results.len(), 2, "{archive}: {results:?}");
        assert!(
            matches!(results[0].1, Ok(ImportOutcome::Added { .. })),
            "{archive}: {results:?}"
        );
        assert!(matches!(
            results[1].1,
            Ok(ImportOutcome::AlreadyInLibrary { .. })
        ));
        assert_eq!(library.books().unwrap().len(), 1);
    }
}

#[test]
fn oversized_members_are_refused_after_keeping_completed_books() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let path = dir.path().join("books.zip");
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default();
    zip.start_file(
        "first.epub",
        options.compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(&fs::read(fixture("../epub/minimal.epub")).unwrap())
        .unwrap();
    zip.start_file(
        "oversized.epub",
        options.compression_method(zip::CompressionMethod::Deflated),
    )
    .unwrap();
    zip.write_all(b"tiny payload with a forged size").unwrap();
    let zip = zip::ZipArchive::new(zip.finish().unwrap()).unwrap();
    let size_field =
        usize::try_from(zip.by_index_data(1).unwrap().central_header_start()).unwrap() + 24;
    let mut bytes = zip.into_inner().into_inner();
    bytes[size_field..size_field + 4].copy_from_slice(&(1024u32 * 1024 * 1024 + 1).to_le_bytes());
    fs::write(&path, bytes).unwrap();

    let results = import_archive(&mut library, &InProcess, &path, |_| {});
    assert_eq!(results.len(), 2, "{results:?}");
    assert!(matches!(results[0].1, Ok(ImportOutcome::Added { .. })));
    assert!(
        results[1]
            .1
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("expands beyond 1 GiB")
    );
    assert_eq!(library.books().unwrap().len(), 1);
}
