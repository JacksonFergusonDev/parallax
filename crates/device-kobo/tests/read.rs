use std::fs;
use std::path::{Path, PathBuf};

use erc_device::ReadStatus;
use erc_device_kobo::{Error, KoboDb, KoboDevice};
use rusqlite::Connection;

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn insert_content(conn: &Connection, id: &str, title: &str, extra: &str) {
    // Defaults mirror a sideloaded, downloaded book; `extra` overrides columns.
    conn.execute_batch(&format!(
        "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Accessibility, IsDownloaded)
         VALUES ('{id}', 6, 'application/x-kobo-epub+zip', 'kepub_user', '{title}', -1, 'true');
         {extra}"
    ))
    .unwrap();
}

/// A device volume with a synthetic database built from the real fw 4.38 schema.
fn fake_kobo(dir: &Path) -> PathBuf {
    let mount = dir.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute_batch(SCHEMA).unwrap();

    let a = "file:///mnt/onboard/KEpub/Fantasy/Lies.kepub.epub";
    insert_content(
        &conn,
        a,
        "The Lies of Locke Lamora",
        &format!(
            "UPDATE content SET Attribution='Scott Lynch', ReadStatus=1, ___PercentRead=42,
             DateLastRead='2024-10-10T06:36:38.327', TimeSpentReading=3600, DateCreated='2024-10-09T15:27:08',
             Series='Gentlemen Bastard', SeriesNumber='1', ___FileSize=3446429 WHERE ContentID='{a}';"
        ),
    );
    let b = "file:///mnt/onboard/KEpub/Horror/Dracula.kepub.epub";
    insert_content(
        &conn,
        b,
        "Dracula",
        &format!("UPDATE content SET ReadStatus=2, ___PercentRead=100 WHERE ContentID='{b}';"),
    );
    insert_content(&conn, "file:///mnt/onboard/new.epub", "Unread Book", "");
    // Store book that is downloaded.
    insert_content(
        &conn,
        "store-uuid-1",
        "Store Downloaded",
        "UPDATE content SET Accessibility=1 WHERE ContentID='store-uuid-1';",
    );
    // Excluded: store book in the cloud only.
    insert_content(
        &conn,
        "store-uuid-2",
        "Store Cloud",
        "UPDATE content SET IsDownloaded='false' WHERE ContentID='store-uuid-2';",
    );
    // Excluded: chapter row (has a BookID).
    insert_content(
        &conn,
        &format!("{a}#ch1"),
        "Chapter 1",
        &format!("UPDATE content SET BookID='{a}', ContentType=9 WHERE ContentID='{a}#ch1';"),
    );
    // Excluded: SD card.
    insert_content(&conn, "file:///mnt/sd/sd.epub", "On SD", "");

    conn.execute_batch(&format!(
        "INSERT INTO Shelf (Id, InternalName, Name, Type, _IsDeleted, _IsVisible, _IsSynced)
           VALUES ('Fantasy', 'Fantasy', 'Fantasy', 'UserTag', 'false', 'true', 'false'),
                  ('Old', 'Old', 'Old', 'UserTag', 'true', 'true', 'false');
         INSERT INTO ShelfContent (ShelfName, ContentId, _IsDeleted, _IsSynced)
           VALUES ('Fantasy', '{a}', 'false', 'false'),
                  ('Old', '{b}', 'false', 'false'),
                  ('Fantasy', '{b}', 'true', 'false');"
    ))
    .unwrap();
    mount
}

#[test]
fn detects_only_kobo_volumes() {
    let dir = tempfile::tempdir().unwrap();
    fake_kobo(dir.path());
    fs::create_dir_all(dir.path().join("USB Stick")).unwrap();
    let found = KoboDevice::detect(dir.path());
    assert_eq!(found.len(), 1);
    let info = found[0].info();
    assert_eq!(info.model, "Kobo Libra 2");
    assert_eq!(info.firmware, "4.38.23697");
}

#[test]
fn lists_books_like_calibre() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = KoboDevice::open(fake_kobo(dir.path())).unwrap();
    let db = kobo.open_db().unwrap();
    assert_eq!(db.db_version(), 174);
    assert!(db.writes_supported());

    let books = db.books().unwrap();
    let titles: Vec<&str> = books.iter().map(|b| b.title.as_str()).collect();
    assert_eq!(
        titles,
        [
            "Dracula",
            "Store Downloaded",
            "The Lies of Locke Lamora",
            "Unread Book"
        ]
    );

    let lies = books
        .iter()
        .find(|b| b.title.starts_with("The Lies"))
        .unwrap();
    assert_eq!(lies.author.as_deref(), Some("Scott Lynch"));
    assert_eq!(lies.path.as_deref(), Some("KEpub/Fantasy/Lies.kepub.epub"));
    assert!(lies.sideloaded);
    assert_eq!(lies.read_status, ReadStatus::Reading);
    assert_eq!(lies.percent_read, 42);
    assert_eq!(lies.reading_time_secs, Some(3600));
    assert_eq!(lies.series.as_deref(), Some("Gentlemen Bastard"));
    assert_eq!(lies.series_number.as_deref(), Some("1"));
    assert_eq!(lies.file_size, Some(3446429));
    assert!(lies.last_read.is_some() && lies.added.is_some());
    assert_eq!(lies.collections, ["Fantasy"]);

    let dracula = books.iter().find(|b| b.title == "Dracula").unwrap();
    assert_eq!(dracula.read_status, ReadStatus::Finished);
    // Membership of a soft-deleted shelf row is ignored; the 'Old' shelf row
    // itself is live in ShelfContent, as calibre reads it.
    assert_eq!(dracula.collections, ["Old"]);

    let store = books
        .iter()
        .find(|b| b.title == "Store Downloaded")
        .unwrap();
    assert!(!store.sideloaded && store.path.is_none());
}

#[test]
fn skips_soft_deleted_collections() {
    let dir = tempfile::tempdir().unwrap();
    let db = KoboDevice::open(fake_kobo(dir.path()))
        .unwrap()
        .open_db()
        .unwrap();
    let collections = db.collections().unwrap();
    assert_eq!(collections.len(), 1);
    assert_eq!(collections[0].name, "Fantasy");
    assert_eq!(collections[0].book_count, 1);
}

#[test]
fn reading_leaves_no_side_files() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = KoboDevice::open(fake_kobo(dir.path())).unwrap();
    let db_path = kobo.db_path();
    // Convert to WAL like the real device, then read.
    Connection::open(&db_path)
        .unwrap()
        .pragma_update(None, "journal_mode", "wal")
        .unwrap();
    let before = fs::read(&db_path).unwrap();
    kobo.open_db().unwrap().books().unwrap();
    let wal = db_path.with_file_name("KoboReader.sqlite-wal");
    let shm = db_path.with_file_name("KoboReader.sqlite-shm");
    assert!(!wal.exists() && !shm.exists());
    assert_eq!(before, fs::read(&db_path).unwrap());
}

#[test]
fn rejects_ancient_schema() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("old.sqlite");
    Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TABLE DbVersion (version INTEGER); INSERT INTO DbVersion VALUES (40);",
        )
        .unwrap();
    assert!(matches!(
        KoboDb::open_read_only(&path),
        Err(Error::UnsupportedDbVersion(40))
    ));
}

/// Runs against the gitignored copy of the reference device's database when
/// present (see docs/device-notes.md); skipped elsewhere, e.g. CI.
#[test]
fn reads_reference_device_db() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/private/KoboReader-libra2-fw4.38-db174.sqlite");
    if !path.exists() {
        eprintln!("skipping: {} not present", path.display());
        return;
    }
    let db = KoboDb::open_read_only(&path).unwrap();
    assert_eq!(db.db_version(), 174);
    let books = db.books().unwrap();
    assert_eq!(books.len(), 93);
    assert!(books.iter().all(|b| b.sideloaded));
    assert_eq!(
        books
            .iter()
            .filter(|b| b.read_status == ReadStatus::Finished)
            .count(),
        4
    );
}

#[test]
fn reads_database_image_ids() {
    let dir = tempfile::tempdir().unwrap();
    let mount = fake_kobo(dir.path());
    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute(
        "UPDATE content SET ImageId = 'custom-cover-id' WHERE ContentID = 'store-uuid-1'",
        [],
    )
    .unwrap();
    drop(conn);
    let images = KoboDevice::open(&mount)
        .unwrap()
        .open_db()
        .unwrap()
        .image_ids()
        .unwrap();
    assert_eq!(images.get("store-uuid-1").unwrap(), "custom-cover-id");
    assert!(!images.contains_key("file:///mnt/onboard/new.epub"));
}

/// Opt-in, read-only verification against a connected Kobo. Writes only to a
/// temporary host directory. Run with KOBO_TEST_MOUNT set to the mounted volume.
#[test]
fn connected_kobo_covers() {
    use sha2::{Digest, Sha256};

    let Some(mount) = std::env::var_os("KOBO_TEST_MOUNT") else {
        return;
    };
    let device = KoboDevice::open(PathBuf::from(mount)).unwrap();
    let before = Sha256::digest(fs::read(device.db_path()).unwrap());
    let db = device.open_db().unwrap();
    let images = db.image_ids().unwrap();
    let books = db.books().unwrap();
    assert!(!books.is_empty());
    let cache = tempfile::tempdir().unwrap();
    let mut covered = 0;
    for book in &books {
        if images
            .get(&book.id)
            .and_then(|id| device.cache_cover(id, cache.path()))
            .is_some()
        {
            covered += 1;
        }
    }
    println!("Loaded covers for {covered}/{} device books", books.len());
    assert_eq!(covered, books.len(), "some device books lack cached covers");
    assert_eq!(before, Sha256::digest(fs::read(device.db_path()).unwrap()));
}
