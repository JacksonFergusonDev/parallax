//! The parser against a real export's shape (tests/fixtures/goodreads, anonymized).

use std::path::PathBuf;

use parallax_core::goodreads::{Shelf, read_file, shelf_counts};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/goodreads/library-export.csv")
}

#[test]
fn reads_every_row_of_the_export() {
    let books = read_file(&fixture()).unwrap();
    assert_eq!(books.len(), 283);
    let on = |shelf: Shelf| books.iter().filter(|b| b.shelf == shelf).count();
    assert_eq!(on(Shelf::Read), 116);
    assert_eq!(on(Shelf::ToRead), 166);
    assert_eq!(on(Shelf::CurrentlyReading), 1);

    let counts = shelf_counts(&books);
    assert_eq!(counts["read"], 116);
    assert_eq!(counts["to-read"], 166);
    assert_eq!(counts["favorites"], 19);
    assert_eq!(counts["favourite-fantasy"], 7);
    assert_eq!(counts["favourite-scifi"], 7);
}

#[test]
fn every_book_is_complete_enough_to_use() {
    for book in read_file(&fixture()).unwrap() {
        assert!(!book.goodreads_id.is_empty());
        assert!(
            !book.title.is_empty() && !book.title.contains(", #"),
            "{}",
            book.title
        );
        assert!(!book.authors.is_empty());
        assert!(
            book.isbn
                .as_deref()
                .is_none_or(|i| i.len() == 10 && !i.contains('"'))
        );
        assert!(
            book.isbn13
                .as_deref()
                .is_none_or(|i| i.len() == 13 && !i.contains('"'))
        );
        assert!(book.rating.is_none_or(|r| (1..=5).contains(&r)));
        assert!(book.date_added.is_some());
        assert!(book.date_read.is_none() || book.shelf == Shelf::Read);
        assert!(book.review.as_deref().is_none_or(|r| !r.contains("<br/>")));
    }
}

#[test]
fn finds_series_and_keeps_other_parentheses() {
    let books = read_file(&fixture()).unwrap();
    assert_eq!(books.iter().filter(|b| b.series.is_some()).count(), 116);
    assert!(
        books.iter().any(|b| b.title.contains('(')),
        "a title keeps its own parenthetical"
    );
}
