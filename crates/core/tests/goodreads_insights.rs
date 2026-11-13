//! Insights over the anonymized export (tests/fixtures/goodreads). The
//! expected numbers were counted from the CSV separately, not by this code.

use std::path::PathBuf;

use chrono::NaiveDate;
use parallax_core::goodreads::{self, Book, ExportFile, Resolutions, Series, Shelf, read_file};
use parallax_core::goodreads_insights::{compute, insights};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/goodreads/library-export.csv")
}

fn today() -> NaiveDate {
    NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()
}

fn book(id: &str, shelf: Shelf) -> Book {
    let mut b = read_file(&fixture()).unwrap().remove(0);
    b.goodreads_id = id.into();
    b.title = format!("Book {id}");
    b.authors = vec!["Ann Author".into()];
    b.shelf = shelf;
    b.shelves = vec![];
    b.series = None;
    b.rating = None;
    b.average_rating = None;
    b.date_read = None;
    b.date_added = None;
    b.read_count = 0;
    b.pages = None;
    b.review = None;
    b
}

fn dated(id: &str, read: &str) -> Book {
    let mut b = book(id, Shelf::Read);
    b.date_read = Some(read.parse().unwrap());
    b.read_count = 1;
    b
}

#[test]
fn counts_the_export() {
    let books = read_file(&fixture()).unwrap();
    let i = compute(&books, &[], today());
    let t = &i.totals;
    assert_eq!((t.books, t.read, t.to_read, t.reading), (283, 116, 166, 1));
    assert_eq!((t.read_undated, t.reviewed, t.pages_read), (53, 49, 32387));
    assert_eq!(t.rated, 116 - 42);
    assert_eq!(i.ratings.stars, [0, 3, 12, 32, 27]);
    assert_eq!(i.ratings.unrated, 42);
    assert_eq!(
        i.ratings.against_goodreads, None,
        "no averages in a 2026 export"
    );
    let years: Vec<(i32, usize)> = i.years.iter().map(|y| (y.year, y.books)).collect();
    assert_eq!(
        years,
        [
            (2021, 23),
            (2022, 24),
            (2023, 7),
            (2024, 8),
            (2025, 1),
            (2026, 0)
        ]
    );
    assert_eq!(
        i.years.iter().map(|y| y.books).sum::<usize>() + t.read_undated,
        t.read,
        "every read book is in a year or counted as undated"
    );
    let wait = i.wait.unwrap();
    assert_eq!(
        (wait.books, wait.median_days, wait.longest_days),
        (60, 19, 443)
    );
    assert_eq!(wait.logged_after, 3);
    assert_eq!(i.rereads.len(), 1);
    assert_eq!(i.books.len(), 283);
    assert!(i.books.iter().all(|b| b.library.is_empty()));
}

#[test]
fn every_reference_is_a_saved_book() {
    let books = read_file(&fixture()).unwrap();
    let i = compute(&books, &[], today());
    let ids: std::collections::HashSet<&str> = i
        .books
        .iter()
        .map(|b| b.book.goodreads_id.as_str())
        .collect();
    let refs = [
        i.to_read.oldest.as_deref(),
        i.lengths.longest.as_deref(),
        i.lengths.shortest.as_deref(),
        i.wait.as_ref().map(|w| w.longest.as_str()),
    ];
    for r in refs
        .into_iter()
        .flatten()
        .chain(i.rereads.iter().map(String::as_str))
    {
        assert!(ids.contains(r), "{r}");
    }
    let bucketed: usize = i.lengths.buckets.iter().map(|b| b.read + b.to_read).sum();
    let paged = books
        .iter()
        .filter(|b| b.pages.is_some() && matches!(b.shelf, Shelf::Read | Shelf::ToRead))
        .count();
    assert_eq!(bucketed, paged);
}

#[test]
fn pace_compares_with_the_same_day_last_year() {
    let books = [
        dated("1", "2025-02-01"),
        dated("2", "2025-11-12"),
        dated("3", "2025-11-13"),
        dated("4", "2026-03-01"),
    ];
    let pace = compute(&books, &[], today()).pace.unwrap();
    assert_eq!(
        (pace.this_year, pace.last_year_by_now, pace.last_year),
        (1, 2, 3)
    );
    assert_eq!(
        compute(&[dated("1", "2020-01-01")], &[], today()).pace,
        None
    );
}

#[test]
fn the_to_read_pile_and_how_long_it_would_take() {
    let mut old = book("1", Shelf::ToRead);
    old.date_added = NaiveDate::from_ymd_opt(2020, 1, 1);
    let mut new = book("2", Shelf::ToRead);
    new.date_added = NaiveDate::from_ymd_opt(2026, 11, 2);
    let books = [old, new, dated("3", "2026-06-01"), dated("4", "2025-06-01")];
    let pile = compute(&books, &[], today()).to_read;
    assert_eq!(pile.books, 2);
    assert_eq!(pile.oldest.as_deref(), Some("1"));
    assert_eq!(pile.read_last_year, 1);
    assert_eq!(pile.years_to_clear, Some(2.0));
}

#[test]
fn series_with_more_to_read_or_gaps() {
    let entry = |id: &str, n: &str, shelf: Shelf| {
        let mut b = book(id, shelf);
        b.series = Some(Series {
            name: "Saga".into(),
            number: n.into(),
        });
        b
    };
    let books = [
        entry("1", "1", Shelf::Read),
        entry("2", "4", Shelf::Read),
        entry("3", "10", Shelf::ToRead),
        entry("4", "2.5", Shelf::Read),
        entry("5", "2", Shelf::ToRead),
    ];
    let [saga] = compute(&books, &[], today()).series.try_into().unwrap();
    assert_eq!(saga.read, ["1", "2.5", "4"]);
    assert_eq!(saga.unread, ["2", "10"]);
    assert_eq!(saga.missing, [3]);
    let finished = [entry("1", "1", Shelf::Read), entry("2", "2", Shelf::Read)];
    assert!(
        compute(&finished, &[], today()).series.is_empty(),
        "nothing left"
    );
}

#[test]
fn compares_ratings_with_goodreads_when_the_export_has_averages() {
    let books: Vec<Book> = (1..=5)
        .map(|n| {
            let mut b = dated(&n.to_string(), "2026-01-01");
            b.rating = Some(3);
            b.average_rating = Some(4.0);
            b
        })
        .collect();
    let gap = compute(&books, &[], today())
        .ratings
        .against_goodreads
        .unwrap();
    assert_eq!((gap.books, gap.mine, gap.goodreads), (5, 3.0, 4.0));
    assert_eq!(
        compute(&books[..4], &[], today()).ratings.against_goodreads,
        None,
        "too few to say"
    );
}

#[test]
fn includes_the_last_import() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = parallax_library::Library::open(dir.path()).unwrap();
    let empty = insights(&lib, today()).unwrap();
    assert_eq!(
        (empty.import, empty.imports, empty.totals.books),
        (None, 0, 0)
    );
    goodreads::import_file(&mut lib, &fixture(), &Resolutions::default()).unwrap();
    let books = read_file(&fixture()).unwrap();
    goodreads::import(
        &mut lib,
        &books,
        &ExportFile::default(),
        &Resolutions::default(),
    )
    .unwrap();
    let i = insights(&lib, today()).unwrap();
    assert_eq!(i.imports, 2);
    let last = i.import.unwrap();
    assert_eq!((last.file_name, last.books, last.added), (None, 283, 0));
    assert_eq!(i.totals.books, 283);
}

/// A series' name, numbers read, numbers unread and numbers missing.
type Progress<'a> = (&'a str, Vec<String>, Vec<String>, Vec<u32>);

#[test]
fn series_join_across_apostrophes_and_unmarked_volumes() {
    let marked = |id: &str, name: &str, n: &str| {
        let mut b = book(id, Shelf::Read);
        b.series = Some(Series {
            name: name.into(),
            number: n.into(),
        });
        b
    };
    let titled = |id: &str, title: &str, shelf: Shelf| {
        let mut b = book(id, shelf);
        b.title = title.into();
        b
    };
    let books = [
        marked("1", "Harbor's Light", "1"),
        marked("2", "Harbor\u{2018}s Light", "3"),
        marked("3", "Saga", "1"),
        titled("4", "Saga, Vol. 2", Shelf::Read),
        titled("5", "Saga Vol. 3 [Saga 3]", Shelf::Read),
        titled("6", "Saga, Vol. 4", Shelf::ToRead),
        titled("7", "Collected Letters, Vol. 2", Shelf::Read),
    ];
    let series = compute(&books, &[], today()).series;
    let found: Vec<Progress> = series
        .iter()
        .map(|s| {
            (
                s.name.as_str(),
                s.read.clone(),
                s.unread.clone(),
                s.missing.clone(),
            )
        })
        .collect();
    assert_eq!(
        found,
        [
            (
                "Saga",
                vec!["1".into(), "2".into(), "3".into()],
                vec!["4".into()],
                vec![]
            ),
            (
                "Harbor's Light",
                vec!["1".into(), "3".into()],
                vec![],
                vec![2]
            ),
        ],
        "a lone unmarked volume isn't a series"
    );
}
