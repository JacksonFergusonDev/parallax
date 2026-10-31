use std::fs;
use std::path::Path;

use erc_device_kobo::{
    Error, KoboDevice, add_to_collection, create_collection, delete_collection,
    remove_from_collection, rename_collection,
};
use rusqlite::Connection;

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn cid(name: &str) -> String {
    format!("file:///mnt/onboard/KEpub/{name}.kepub.epub")
}

fn fake_kobo(root: &Path) -> KoboDevice {
    let mount = root.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    for name in ["Dune", "Emma"] {
        conn.execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Accessibility, IsDownloaded)
             VALUES (?1, 6, 'application/x-kobo-epub+zip', 'kepub_user', ?2, -1, 'true')",
            [cid(name), name.to_owned()],
        )
        .unwrap();
    }
    // A shelf that came from the Kobo account, and Nickel's own Wishlist.
    conn.execute_batch(
        "INSERT INTO Shelf (Id, InternalName, Name, Type, _IsDeleted, _IsVisible, _IsSynced)
           VALUES ('uuid-1', 'Synced', 'Synced', 'UserTag', 'false', 'true', 'true'),
                  ('Wishlist', 'Wishlist', 'Wishlist', 'SystemTag', 'false', 'true', 'true');",
    )
    .unwrap();
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    KoboDevice::open(mount).unwrap()
}

fn with_writer<T>(
    kobo: &KoboDevice,
    dir: &Path,
    f: impl FnOnce(&rusqlite::Transaction) -> erc_device_kobo::Result<T>,
) -> erc_device_kobo::Result<T> {
    let mut writer = kobo.open_writer(&dir.join("backups"))?;
    let value = writer.transaction(f);
    writer.close()?;
    value
}

fn names(kobo: &KoboDevice) -> Vec<(String, usize)> {
    kobo.open_db()
        .unwrap()
        .collections()
        .unwrap()
        .into_iter()
        .map(|c| (c.name, c.book_count))
        .collect()
}

#[test]
fn adds_books_including_not_yet_imported() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let ids = vec![cid("Dune"), cid("Emma"), cid("Not Imported Yet")];
    let added = with_writer(&kobo, dir.path(), |tx| {
        add_to_collection(tx, "Classics", &ids)
    })
    .unwrap();
    assert_eq!(added, 3);
    // Adding again is a no-op.
    assert_eq!(
        with_writer(&kobo, dir.path(), |tx| add_to_collection(
            tx, "Classics", &ids
        ))
        .unwrap(),
        0
    );
    assert_eq!(
        names(&kobo),
        [("Classics".to_owned(), 3), ("Synced".to_owned(), 0)]
    );
    let books = kobo.open_db().unwrap().books().unwrap();
    assert!(books.iter().all(|b| b.collections == ["Classics"]));
}

#[test]
fn removes_renames_and_deletes() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let ids = vec![cid("Dune"), cid("Emma")];
    with_writer(&kobo, dir.path(), |tx| add_to_collection(tx, "SF", &ids)).unwrap();
    with_writer(&kobo, dir.path(), |tx| {
        remove_from_collection(tx, "SF", &ids[1..])
    })
    .unwrap();
    assert_eq!(names(&kobo)[0], ("SF".to_owned(), 1));

    with_writer(&kobo, dir.path(), |tx| {
        rename_collection(tx, "SF", "Science Fiction")
    })
    .unwrap();
    assert_eq!(names(&kobo)[0], ("Science Fiction".to_owned(), 1));
    let clash = with_writer(&kobo, dir.path(), |tx| {
        rename_collection(tx, "Science Fiction", "Synced")
    });
    assert!(matches!(clash, Err(Error::InvalidCollection(_))));

    // Never-synced collections are deleted outright...
    with_writer(&kobo, dir.path(), |tx| {
        delete_collection(tx, "Science Fiction")
    })
    .unwrap();
    // ...synced ones are soft-deleted so the deletion syncs.
    with_writer(&kobo, dir.path(), |tx| delete_collection(tx, "Synced")).unwrap();
    assert!(names(&kobo).is_empty());
    let conn = Connection::open(kobo.db_path()).unwrap();
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT Name, _IsDeleted FROM Shelf WHERE Name != 'Wishlist' ORDER BY Name")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    assert_eq!(rows, [("Synced".to_owned(), "true".to_owned())]);
    let members: i64 = conn
        .query_row("SELECT count(*) FROM ShelfContent", [], |r| r.get(0))
        .unwrap();
    assert_eq!(members, 0);
}

#[test]
fn recreating_a_deleted_synced_collection_undeletes_it() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    with_writer(&kobo, dir.path(), |tx| delete_collection(tx, "Synced")).unwrap();
    with_writer(&kobo, dir.path(), |tx| create_collection(tx, "Synced")).unwrap();
    assert_eq!(names(&kobo), [("Synced".to_owned(), 0)]);
}

#[test]
fn protects_system_collections_and_empty_names() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    for result in [
        with_writer(&kobo, dir.path(), |tx| delete_collection(tx, "Wishlist")),
        with_writer(&kobo, dir.path(), |tx| create_collection(tx, "  ")),
        with_writer(&kobo, dir.path(), |tx| {
            rename_collection(tx, "Synced", "shortlist")
        }),
    ] {
        assert!(matches!(result, Err(Error::InvalidCollection(_))));
    }
    // Nickel's Wishlist is never listed as a user collection.
    assert!(names(&kobo).iter().all(|(n, _)| n != "Wishlist"));
}
