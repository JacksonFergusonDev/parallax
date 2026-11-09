//! "Tidy genres" on a library and a fixture Kobo database.

use std::fs;
use std::path::Path;

use parallax_core::collections::{CollectionChange, apply};
use parallax_core::genres::{TidyChoice, apply_tidy, plan};
use parallax_device_kobo::KoboDevice;
use parallax_library::{ImportOutcome, Library};

const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn kobo(dir: &Path) -> KoboDevice {
    let mount = dir.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    rusqlite::Connection::open(mount.join(".kobo/KoboReader.sqlite"))
        .unwrap()
        .execute_batch(include_str!(
            "../../../tests/fixtures/kobo/schema-db174.sql"
        ))
        .unwrap();
    KoboDevice::open(&mount).unwrap()
}

#[test]
fn renames_and_merges_genre_tags_and_collections() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = kobo(dir.path());
    let backups = dir.path().join("backups");
    let add = |name: &str, ids: &[&str]| CollectionChange::Add {
        name: name.into(),
        content_ids: ids
            .iter()
            .map(|s| format!("file:///mnt/onboard/{s}"))
            .collect(),
    };
    apply(
        &kobo,
        &backups,
        &[
            add("Sci-Fi", &["a.epub", "b.epub"]),
            add("SciFi", &["c.epub"]),
            add("Non-Fiction", &["d.epub"]),
            add("Fantasy", &["e.epub"]),
            add("Favourites", &["a.epub"]),
        ],
    )
    .unwrap();

    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let epub = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/epub/minimal.epub");
    let ImportOutcome::Added { book, .. } = library.import(&epub).unwrap() else {
        panic!();
    };
    let mut edit = book.edit();
    edit.tags = vec![
        "sci-fi".into(),
        "Science Fiction".into(),
        "Space Opera".into(),
        "Favourites".into(),
    ];
    let book = library.update(book.id, &edit).unwrap();

    let tidy = plan(&library, Some(&kobo)).unwrap();
    let tags: Vec<(&str, &str, bool)> = tidy
        .tags
        .iter()
        .map(|t| (t.from.as_str(), t.to.as_str(), t.exact))
        .collect();
    assert_eq!(
        tags,
        [
            ("sci-fi", "Science Fiction", true),
            ("Space Opera", "Science Fiction", false)
        ]
    );
    let collections: Vec<(&str, &str, bool)> = tidy
        .collections
        .iter()
        .map(|c| (c.from.as_str(), c.to.as_str(), c.merge))
        .collect();
    assert_eq!(
        collections,
        [
            ("Non-Fiction", "Nonfiction", false),
            ("Sci-Fi", "Science Fiction", false),
            ("SciFi", "Science Fiction", true),
        ]
    );

    // The user takes the exact ones only.
    let choice = TidyChoice {
        tags: vec![("sci-fi".into(), "Science Fiction".into())],
        collections: tidy
            .collections
            .iter()
            .map(|c| (c.from.clone(), c.to.clone()))
            .collect(),
    };
    let result = apply_tidy(&mut library, Some(&kobo), &backups, &choice).unwrap();
    assert_eq!((result.books, result.collections), (1, 3));

    let after = library.book(book.id).unwrap();
    assert_eq!(after.tags, ["Science Fiction", "Space Opera", "Favourites"]);
    // Not marked edited: the Kobo copy needn't be updated for a tag rename.
    assert_eq!(after.modified_at, book.modified_at);

    let on_kobo: Vec<(String, usize)> = kobo
        .open_db()
        .unwrap()
        .collections()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.book_count))
        .collect();
    assert_eq!(
        on_kobo,
        [
            ("Fantasy".to_owned(), 1),
            ("Favourites".to_owned(), 1),
            ("Nonfiction".to_owned(), 1),
            ("Science Fiction".to_owned(), 3),
        ]
    );
    // Nothing left to tidy but the optional "Space Opera".
    let again = plan(&library, Some(&kobo)).unwrap();
    assert!(again.collections.is_empty());
    assert!(again.tags.iter().all(|t| !t.exact));
}
