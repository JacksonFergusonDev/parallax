//! Enhance and organise for books: a description fills only a gap.

use std::path::Path;

use parallax_core::enhance::{self, Accepted, DeviceClues};
use parallax_library::{ImportOutcome, Library};
use parallax_metadata::{Fetch, FetchError};

/// Wikidata and Open Library know nothing; Apple Books has the book.
struct AppleOnly;

impl Fetch for AppleOnly {
    fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<String, FetchError> {
        Ok(if url.contains("itunes") {
            r#"{"results":[
                {"trackName":"Parallax Test Book","artistName":"Test Author","genres":["Fantasy"],
                 "description":"<b>A test.</b> Of descriptions."},
                {"trackName":"Parallax Test Book","artistName":"Test Author",
                 "description":"Another edition, listed second."}
            ]}"#
        } else {
            "{}"
        }
        .into())
    }
}

#[test]
fn a_book_without_a_description_gets_one() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let epub = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/epub/minimal.epub");
    let ImportOutcome::Added { book, .. } = library.import(&epub).unwrap() else {
        panic!("expected a book");
    };
    assert_eq!(book.description, None);

    let books = [book];
    let propose = |books: &[parallax_library::Book], library: &Library| {
        enhance::propose(
            books,
            &enhance::options(library, books),
            &AppleOnly,
            &DeviceClues::default(),
            |_, _, _| {},
        )
        .remove(0)
    };
    let proposal = propose(&books, &library);
    let details = proposal.details.as_ref().unwrap();
    assert!(details.confident);
    assert_eq!(details.source.as_deref(), Some("Apple Books"));
    assert_eq!(
        details.values.description.as_deref(),
        Some("<b>A test.</b> Of descriptions.")
    );
    // Nothing else is proposed from a book's details.
    assert!(details.values.authors.is_empty() && details.values.publisher.is_none());

    let applied = enhance::apply(
        &mut library,
        &[Accepted {
            book_id: books[0].id,
            details: Some(details.values.clone()),
            ..Accepted::default()
        }],
    )
    .unwrap();
    assert_eq!(
        applied[0].description.as_deref(),
        Some("<b>A test.</b> Of descriptions.")
    );
    assert_eq!(applied[0].authors, ["Test Author"]);

    // With a description, none is proposed.
    let again = propose(&applied, &library);
    assert!(again.details.is_none(), "{again:?}");
}
