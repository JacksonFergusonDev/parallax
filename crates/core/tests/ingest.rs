//! Adding Amazon books to the library.

use std::fs;
use std::path::{Path, PathBuf};

use parallax_convert::amazon::InProcess;
use parallax_core::ingest::{Error, import_file};
use parallax_library::{ImportOutcome, Library};

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
