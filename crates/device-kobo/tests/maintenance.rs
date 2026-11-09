use std::fs;
use std::path::{Path, PathBuf};

use parallax_device::ReadStatus;
use parallax_device_kobo::{
    KoboDevice, duplicate_collections, files_not_in_database, image_id_for,
    merge_duplicate_collections, missing_files, orphaned_images, qhash, remove_orphaned_images,
    set_read_status,
};
use rusqlite::{Connection, params};

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";
const READING: &str = "KEpub/Reading.kepub.epub";
const GONE: &str = "KEpub/Gone.kepub.epub";

fn cid(path: &str) -> String {
    format!("file:///mnt/onboard/{path}")
}

/// One book being read, one whose file was deleted outside the Kobo, and a
/// file Nickel hasn't imported.
fn fake_kobo(root: &Path) -> (KoboDevice, PathBuf) {
    let mount = root.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::create_dir_all(mount.join("KEpub/.hidden")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    fs::write(mount.join(READING), b"book").unwrap();
    fs::write(mount.join("KEpub/New.epub"), b"new").unwrap();
    fs::write(mount.join("KEpub/.hidden/Ignored.epub"), b"x").unwrap();
    fs::write(mount.join("KEpub/notes.docx"), b"x").unwrap();
    let db = mount.join(".kobo/KoboReader.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    for (path, title) in [(READING, "Reading"), (GONE, "Gone")] {
        conn.execute(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, ImageId,
                 ReadStatus, ___PercentRead, DateLastRead, ChapterIDBookmarked, FirstTimeReading,
                 TimeSpentReading, RestOfBookEstimate, Accessibility, IsDownloaded)
             VALUES (?1, 6, 'application/x-kobo-epub+zip', 'kepub_user', ?2, ?3,
                 1, 37, '2026-10-30T20:00:00Z', 'ch3.xhtml#kobo.4.1', 'false', 5400, 1200, -1, 'true')",
            params![cid(path), title, image_id_for(&cid(path))],
        )
        .unwrap();
    }
    drop(conn);
    (KoboDevice::open(&mount).unwrap(), db)
}

#[test]
fn reading_status_follows_kobo_utilities() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let row = |db: &Path| -> (i64, i64, Option<String>, Option<String>, String, i64) {
        Connection::open(db)
            .unwrap()
            .query_row(
                "SELECT ReadStatus, ___PercentRead, DateLastRead, ChapterIDBookmarked, FirstTimeReading, TimeSpentReading
                 FROM content WHERE ContentID = ?1",
                [cid(READING)],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
            )
            .unwrap()
    };

    // Finished: status only; where the reader was is kept.
    let mut w = kobo.open_writer(&backups).unwrap();
    assert_eq!(
        set_read_status(&mut w, &[cid(READING)], ReadStatus::Finished, false, false).unwrap(),
        1
    );
    w.close().unwrap();
    let (status, pct, last, mark, _, time) = row(&db);
    assert_eq!((status, pct, time), (2, 37, 5400));
    assert!(last.is_some() && mark.is_some());

    // Unread, starting over, reading time cleared.
    let mut w = kobo.open_writer(&backups).unwrap();
    set_read_status(&mut w, &[cid(READING)], ReadStatus::Unread, true, true).unwrap();
    w.close().unwrap();
    assert_eq!(row(&db), (0, 0, None, None, "true".into(), 0));

    let log = kobo.parallax_state().log;
    assert_eq!(log[0].op, "reading status");
    assert_eq!(
        log[0].detail.as_deref(),
        Some("marked unread, position reset, reading time reset")
    );
    assert!(log[0].backup.is_some());
}

#[test]
fn duplicate_collections_keep_the_oldest_copy() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let conn = Connection::open(&db).unwrap();
    for (id, name, created, synced) in [
        ("a", "Fantasy", "2024-01-01T00:00:00Z", "true"),
        ("b", "Fantasy", "2024-06-01T00:00:00Z", "false"),
        ("c", "Fantasy", "2025-01-01T00:00:00Z", "true"),
        ("d", "Solo", "2024-01-01T00:00:00Z", "false"),
    ] {
        conn.execute(
            "INSERT INTO Shelf (CreationDate, Id, InternalName, LastModified, Name, Type, _IsDeleted, _IsVisible, _IsSynced)
             VALUES (?3, ?1, ?2, ?3, ?2, 'UserTag', 'false', 'true', ?4)",
            params![id, name, created, synced],
        )
        .unwrap();
    }
    conn.execute(
        "INSERT INTO ShelfContent (ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced) VALUES ('Fantasy', ?1, '2024-01-01', 'false', 'false')",
        [cid(READING)],
    )
    .unwrap();
    drop(conn);

    let found = duplicate_collections(kobo.open_db().unwrap().connection()).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].name.as_str(), found[0].copies), ("Fantasy", 3));

    let mut w = kobo.open_writer(&dir.path().join("backups")).unwrap();
    assert_eq!(w.transaction(merge_duplicate_collections).unwrap(), 2);
    w.close().unwrap();

    let conn = Connection::open(&db).unwrap();
    let rows: Vec<(String, String)> = conn
        .prepare("SELECT Id, _IsDeleted FROM Shelf WHERE Name = 'Fantasy' ORDER BY Id")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    // Oldest kept, the synced copy marked deleted so the deletion syncs, the
    // local copy gone.
    assert_eq!(
        rows,
        [("a".into(), "false".into()), ("c".into(), "true".into())]
    );
    let members: i64 = conn
        .query_row(
            "SELECT count(*) FROM ShelfContent WHERE ShelfName = 'Fantasy'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(members, 1);
    assert!(
        duplicate_collections(kobo.open_db().unwrap().connection())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn only_covers_of_vanished_books_are_cleaned() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let cache = |id: &str| {
        let h = qhash(id);
        kobo.mount()
            .join(".kobo-images")
            .join((h & 0xFF).to_string())
            .join(((h >> 8) & 0xFF).to_string())
    };
    let kept_id = image_id_for(&cid(READING));
    let old_id = image_id_for(&cid("KEpub/Removed Long Ago.kepub.epub"));
    for (id, ending) in [
        (&kept_id, "N3_FULL"),
        (&old_id, "N3_FULL"),
        (&old_id, "N3_LIBRARY_GRID"),
    ] {
        fs::create_dir_all(cache(id)).unwrap();
        fs::write(cache(id).join(format!("{id} - {ending}.parsed")), b"img").unwrap();
    }
    // Something that isn't a cover is never touched.
    fs::write(kobo.mount().join(".kobo-images/readme.txt"), b"x").unwrap();

    let orphans = orphaned_images(&kobo).unwrap();
    assert_eq!(orphans.len(), 2);
    assert_eq!(remove_orphaned_images(&kobo).unwrap(), (2, 6));
    assert!(!cache(&old_id).exists(), "empty folders removed");
    assert!(
        cache(&kept_id)
            .join(format!("{kept_id} - N3_FULL.parsed"))
            .exists()
    );
    assert!(kobo.mount().join(".kobo-images/readme.txt").exists());
    assert_eq!(kobo.parallax_state().log[0].op, "clean images");
    assert!(orphaned_images(&kobo).unwrap().is_empty());
}

#[test]
fn database_and_files_disagreements() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let missing = missing_files(&kobo).unwrap();
    assert_eq!(missing.len(), 1);
    assert_eq!(
        (missing[0].path.as_str(), missing[0].title.as_str()),
        (GONE, "Gone")
    );
    let unknown: Vec<String> = files_not_in_database(&kobo)
        .unwrap()
        .into_iter()
        .map(|m| m.path)
        .collect();
    assert_eq!(unknown, ["KEpub/New.epub"]);
}

#[test]
fn real_database_copy_survives_maintenance() {
    let real = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/private/KoboReader-libra2-fw4.38-db174.sqlite");
    if !real.exists() {
        eprintln!("skipping: {} not present", real.display());
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    fs::copy(&real, mount.join(".kobo/KoboReader.sqlite")).unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();
    let books = kobo.open_db().unwrap().books().unwrap();
    let reading: Vec<String> = books
        .iter()
        .filter(|b| b.read_status == ReadStatus::Reading)
        .map(|b| b.id.clone())
        .collect();
    assert!(!reading.is_empty());

    let mut w = kobo.open_writer(&dir.path().join("backups")).unwrap();
    set_read_status(&mut w, &reading, ReadStatus::Finished, false, false).unwrap();
    w.transaction(merge_duplicate_collections).unwrap();
    // close() runs integrity_check and would restore the backup on failure.
    w.close().unwrap();

    let after = kobo.open_db().unwrap().books().unwrap();
    assert_eq!(after.len(), books.len());
    assert!(
        after
            .iter()
            .filter(|b| reading.contains(&b.id))
            .all(|b| b.read_status == ReadStatus::Finished)
    );
    // Every sideloaded file is "missing" here (only the database was copied),
    // which is what the report should say.
    assert_eq!(missing_files(&kobo).unwrap().len(), books.len());
}
