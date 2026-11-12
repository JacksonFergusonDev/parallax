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

mod saved {
    use super::fixture;
    use parallax_core::goodreads::{self, GoodreadsError};
    use parallax_library::Library;

    fn library() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        (dir, lib)
    }

    /// The fixture with one book changed, one removed and one added, as a later export would be.
    fn later_export(dir: &std::path::Path) -> std::path::PathBuf {
        let text = std::fs::read_to_string(fixture()).unwrap();
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        let removed = lines.remove(1);
        let (_, rest) = removed.split_once(',').unwrap();
        // A new book reuses the removed row's data under a new Book Id.
        lines.push(format!("99999999990,{rest}"));
        // An existing book changes: it's been read once more.
        let line = lines.iter_mut().find(|l| l.ends_with(",0,0")).unwrap();
        line.replace_range(line.len() - 4.., ",1,0");
        let path = dir.join("later.csv");
        std::fs::write(&path, lines.join("\n") + "\n").unwrap();
        path
    }

    #[test]
    fn importing_the_same_export_again_changes_nothing() {
        let (_dir, mut lib) = library();
        let first = goodreads::import(&mut lib, &fixture()).unwrap();
        assert_eq!((first.sync.added, first.skipped), (283, 0));
        let before = goodreads::saved(&lib).unwrap();

        let second = goodreads::import(&mut lib, &fixture()).unwrap();
        assert_eq!(
            (
                second.sync.unchanged,
                second.sync.added,
                second.sync.updated
            ),
            (283, 0, 0)
        );
        assert_eq!(goodreads::saved(&lib).unwrap(), before);
        assert_eq!(before.len(), 283);
        assert!(before.iter().all(|b| !b.missing));
    }

    #[test]
    fn a_later_export_updates_in_place() {
        let (dir, mut lib) = library();
        goodreads::import(&mut lib, &fixture()).unwrap();
        let report = goodreads::import(&mut lib, &later_export(dir.path())).unwrap();
        assert_eq!(report.sync.added, 1);
        assert_eq!(report.sync.missing, 1);
        assert_eq!(report.sync.updated, 1);
        assert_eq!(report.sync.unchanged, 281);
        let saved = goodreads::saved(&lib).unwrap();
        assert_eq!(saved.len(), 284, "the removed book is kept and flagged");
        assert_eq!(saved.iter().filter(|b| b.missing).count(), 1);
        let mut ids: Vec<_> = saved.iter().map(|b| &b.book.goodreads_id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 284, "no duplicates");
    }

    #[test]
    fn an_empty_or_foreign_file_changes_nothing() {
        let (dir, mut lib) = library();
        goodreads::import(&mut lib, &fixture()).unwrap();
        let header = std::fs::read_to_string(fixture())
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned();
        let empty = dir.path().join("empty.csv");
        std::fs::write(&empty, header + "\n").unwrap();
        assert!(matches!(
            goodreads::import(&mut lib, &empty),
            Err(GoodreadsError::Empty)
        ));
        let other = dir.path().join("other.csv");
        std::fs::write(&other, "Name,Price\nA,1\n").unwrap();
        assert!(matches!(
            goodreads::import(&mut lib, &other),
            Err(GoodreadsError::MissingColumn(_))
        ));
        let saved = goodreads::saved(&lib).unwrap();
        assert_eq!(saved.len(), 283);
        assert!(saved.iter().all(|b| !b.missing));
    }
}
