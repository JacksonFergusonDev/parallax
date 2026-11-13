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
    use chrono::NaiveDate;
    use parallax_core::goodreads::{
        self, Book, Edit, Field, GoodreadsError, MissingPick, Pick, Resolutions, Shelf,
    };
    use parallax_library::Library;

    const TODAY: NaiveDate = NaiveDate::from_ymd_opt(2026, 11, 12).unwrap();

    fn library() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        (dir, lib)
    }

    fn export() -> Vec<Book> {
        goodreads::read_file(&fixture()).unwrap()
    }

    fn saved(lib: &Library, id: &str) -> Book {
        goodreads::saved(lib)
            .unwrap()
            .into_iter()
            .find(|b| b.goodreads_id == id)
            .unwrap()
    }

    fn first_to_read(books: &[Book]) -> String {
        books
            .iter()
            .find(|b| b.shelf == Shelf::ToRead && b.rating.is_none())
            .unwrap()
            .goodreads_id
            .clone()
    }

    #[test]
    fn importing_the_same_export_again_changes_nothing() {
        let (_dir, mut lib) = library();
        let books = export();
        let first = goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        assert_eq!((first.added, first.skipped), (283, 0));
        let before = goodreads::saved(&lib).unwrap();

        let plan = goodreads::plan(&lib, &books).unwrap();
        assert_eq!((plan.unchanged, plan.added, plan.updated), (283, 0, 0));
        assert!(!plan.needs_decisions());
        let second = goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        assert_eq!(
            (second.unchanged, second.added, second.updated),
            (283, 0, 0)
        );
        assert_eq!(goodreads::saved(&lib).unwrap(), before);
    }

    #[test]
    fn a_book_new_to_the_export_is_added_and_a_changed_one_updated() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let changed = books.iter_mut().find(|b| b.goodreads_id == id).unwrap();
        changed.rating = Some(3);
        changed.pages = Some(999);
        let mut new = books[0].clone();
        new.goodreads_id = "99999999990".into();
        books.push(new);

        let plan = goodreads::plan(&lib, &books).unwrap();
        assert_eq!((plan.added, plan.updated, plan.unchanged), (1, 1, 282));
        assert!(!plan.needs_decisions());
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let book = saved(&lib, &id);
        assert_eq!((book.rating, book.pages), (Some(3), Some(999)));
        assert_eq!(goodreads::saved(&lib).unwrap().len(), 284);
    }

    #[test]
    fn your_edit_survives_an_export_that_didnt_touch_it() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let edit = Edit {
            shelf: Some(Shelf::Read),
            rating: Some(5),
            ..Edit::default()
        };
        let mine = goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap();
        assert_eq!(
            (mine.shelf, mine.rating, mine.read_count),
            (Shelf::Read, Some(5), 1)
        );
        assert_eq!(mine.date_read, Some(TODAY));

        // The export changes something else about the book.
        books
            .iter_mut()
            .find(|b| b.goodreads_id == id)
            .unwrap()
            .pages = Some(1234);
        let plan = goodreads::plan(&lib, &books).unwrap();
        assert!(plan.conflicts.is_empty(), "only the export changed pages");
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let book = saved(&lib, &id);
        assert_eq!(
            (book.shelf, book.rating, book.pages),
            (Shelf::Read, Some(5), Some(1234))
        );

        // And importing it again is still a no-op.
        let again = goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        assert_eq!(again.updated, 0);
    }

    #[test]
    fn a_conflict_is_asked_about_and_settled_for_good() {
        for (pick, rating) in [(Pick::Mine, Some(4)), (Pick::Goodreads, Some(2))] {
            let (_dir, mut lib) = library();
            let mut books = export();
            goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
            let id = first_to_read(&books);
            let edit = Edit {
                shelf: Some(Shelf::Read),
                rating: Some(4),
                ..Edit::default()
            };
            goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap();
            books
                .iter_mut()
                .find(|b| b.goodreads_id == id)
                .unwrap()
                .rating = Some(2);

            let plan = goodreads::plan(&lib, &books).unwrap();
            assert!(plan.needs_decisions());
            let [conflict] = plan.conflicts.as_slice() else {
                panic!("one conflict")
            };
            assert_eq!(conflict.goodreads_id, id);
            let [change] = conflict.changes.as_slice() else {
                panic!("one field")
            };
            assert_eq!(change.field, Field::Rating);
            assert_eq!(
                (change.mine.as_str(), change.theirs.as_str()),
                ("4 stars", "2 stars")
            );

            let mut resolutions = Resolutions::default();
            resolutions.conflicts.insert(id.clone(), pick);
            goodreads::import(&mut lib, &books, &resolutions).unwrap();
            let book = saved(&lib, &id);
            assert_eq!(book.rating, rating);
            assert_eq!(book.shelf, Shelf::Read, "your other edits stay either way");

            // Settled: the same export doesn't ask again.
            assert!(!goodreads::plan(&lib, &books).unwrap().needs_decisions());
        }
    }

    #[test]
    fn recommended_choices_keep_your_changes_and_missing_books() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let edit = Edit {
            rating: Some(4),
            ..Edit::default()
        };
        goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap();
        books
            .iter_mut()
            .find(|b| b.goodreads_id == id)
            .unwrap()
            .rating = Some(2);
        let gone = books.remove(5).goodreads_id;

        let report = goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        assert_eq!((report.kept_yours, report.kept, report.removed), (1, 1, 0));
        assert_eq!(saved(&lib, &id).rating, Some(4));
        assert_eq!(
            goodreads::saved(&lib).unwrap().len(),
            283,
            "the missing book is still here"
        );
        assert_eq!(saved(&lib, &gone).goodreads_id, gone);
    }

    #[test]
    fn a_kept_missing_book_isnt_asked_about_again_and_can_come_back() {
        let (_dir, mut lib) = library();
        let all = export();
        goodreads::import(&mut lib, &all, &Resolutions::default()).unwrap();
        let mut fewer = all.clone();
        let gone = fewer.remove(5);
        assert_eq!(goodreads::plan(&lib, &fewer).unwrap().missing.len(), 1);
        goodreads::import(&mut lib, &fewer, &Resolutions::default()).unwrap();
        assert!(
            !goodreads::plan(&lib, &fewer).unwrap().needs_decisions(),
            "decided once"
        );

        // It returns in a later export: back to normal, and still asked about if it vanishes again.
        let back = goodreads::import(&mut lib, &all, &Resolutions::default()).unwrap();
        assert_eq!(back.added, 0);
        assert_eq!(saved(&lib, &gone.goodreads_id), gone);
        assert_eq!(goodreads::plan(&lib, &fewer).unwrap().missing.len(), 1);
    }

    #[test]
    fn a_missing_book_can_be_removed() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let gone = books.remove(5).goodreads_id;
        let plan = goodreads::plan(&lib, &books).unwrap();
        assert_eq!(plan.missing[0].goodreads_id, gone);
        let mut resolutions = Resolutions::default();
        resolutions
            .missing
            .insert(gone.clone(), MissingPick::Remove);
        let report = goodreads::import(&mut lib, &books, &resolutions).unwrap();
        assert_eq!((report.removed, report.kept), (1, 0));
        assert_eq!(goodreads::saved(&lib).unwrap().len(), 282);
        assert!(!goodreads::plan(&lib, &books).unwrap().needs_decisions());
    }

    #[test]
    fn an_empty_or_foreign_file_changes_nothing() {
        let (dir, mut lib) = library();
        goodreads::import_file(&mut lib, &fixture(), &Resolutions::default()).unwrap();
        let header = std::fs::read_to_string(fixture())
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned();
        let empty = dir.path().join("empty.csv");
        std::fs::write(&empty, header + "\n").unwrap();
        let refused = goodreads::import_file(&mut lib, &empty, &Resolutions::default());
        assert!(matches!(refused, Err(GoodreadsError::Empty)));
        assert!(matches!(
            goodreads::plan_file(&lib, &empty),
            Err(GoodreadsError::Empty)
        ));
        let other = dir.path().join("other.csv");
        std::fs::write(&other, "Name,Price\nA,1\n").unwrap();
        let foreign = goodreads::import_file(&mut lib, &other, &Resolutions::default());
        assert!(matches!(foreign, Err(GoodreadsError::MissingColumn(_))));
        assert_eq!(goodreads::saved(&lib).unwrap().len(), 283);
    }

    #[test]
    fn the_exports_date_read_replaces_the_day_you_marked_it_read() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let edit = Edit {
            shelf: Some(Shelf::Read),
            ..Edit::default()
        };
        let mine = goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap();
        assert_eq!(mine.date_read, Some(TODAY));
        assert!(mine.shelves.iter().any(|s| s.name == "read"));
        assert!(!mine.shelves.iter().any(|s| s.name == "to-read"));

        // You marked it read on Goodreads too, a few days earlier.
        let real = NaiveDate::from_ymd_opt(2026, 11, 9).unwrap();
        let theirs = books.iter_mut().find(|b| b.goodreads_id == id).unwrap();
        theirs.shelf = Shelf::Read;
        theirs.read_count = 1;
        theirs.date_read = Some(real);
        let plan = goodreads::plan(&lib, &books).unwrap();
        assert!(
            plan.conflicts.is_empty(),
            "no question about a made-up date"
        );
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let book = saved(&lib, &id);
        assert_eq!(book.date_read, Some(real));
        assert!(!book.date_read_guessed);
    }

    #[test]
    fn a_date_you_chose_is_still_asked_about() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let chosen = NaiveDate::from_ymd_opt(2026, 1, 2).unwrap();
        let edit = Edit {
            shelf: Some(Shelf::Read),
            date_read: Some(chosen),
            ..Edit::default()
        };
        goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap();
        let theirs = books.iter_mut().find(|b| b.goodreads_id == id).unwrap();
        theirs.shelf = Shelf::Read;
        theirs.read_count = 1;
        theirs.date_read = NaiveDate::from_ymd_opt(2026, 11, 9);
        let plan = goodreads::plan(&lib, &books).unwrap();
        assert_eq!(plan.conflicts[0].changes[0].field, Field::DateRead);
    }

    #[test]
    fn a_status_cant_be_added_as_your_own_shelf() {
        let (_dir, mut lib) = library();
        let books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let edit = Edit {
            add_shelves: vec!["read".into()],
            ..Edit::default()
        };
        let err = goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap_err();
        assert!(matches!(err, GoodreadsError::SystemShelf(_)));
        assert_eq!(saved(&lib, &id).shelf, Shelf::ToRead);
    }

    #[test]
    fn importing_with_one_choice_for_everything() {
        let (_dir, mut lib) = library();
        let mut books = export();
        goodreads::import(&mut lib, &books, &Resolutions::default()).unwrap();
        let id = first_to_read(&books);
        let edit = Edit {
            rating: Some(4),
            ..Edit::default()
        };
        goodreads::edit(&mut lib, &id, &edit, TODAY).unwrap();
        books
            .iter_mut()
            .find(|b| b.goodreads_id == id)
            .unwrap()
            .rating = Some(2);
        books.remove(5);
        let report =
            goodreads::import_choosing(&mut lib, &books, Pick::Goodreads, MissingPick::Remove)
                .unwrap();
        assert_eq!((report.took_goodreads, report.removed), (1, 1));
        assert_eq!(saved(&lib, &id).rating, Some(2));
        assert_eq!(goodreads::saved(&lib).unwrap().len(), 282);
    }

    #[test]
    fn editing_an_unknown_book_fails() {
        let (_dir, mut lib) = library();
        goodreads::import(&mut lib, &export(), &Resolutions::default()).unwrap();
        let err = goodreads::edit(&mut lib, "nope", &Edit::default(), TODAY).unwrap_err();
        assert!(matches!(err, GoodreadsError::NotSaved(_)));
    }
}
