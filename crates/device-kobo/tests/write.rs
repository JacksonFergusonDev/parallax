use std::fs;
use std::path::{Path, PathBuf};

use parallax_device_kobo::{
    BACKUPS_KEPT, BackupPolicy, BookCheck, DisplayMetadata, Error, Keep, KoboDevice, LOG_KEPT,
    LogEntry, PARALLAX_VERSION, Record, backups_size, delete_all_backups, image_id_for, qhash,
    remove_book, replace_book, restore, tidy_apple_double,
};
use rusqlite::Connection;

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";
const BOOK: &str = "KEpub/Fantasy/Lies - Scott Lynch.kepub.epub";
const OTHER: &str = "KEpub/Other - Someone.kepub.epub";

fn cid(path: &str) -> String {
    format!("file:///mnt/onboard/{path}")
}

fn count(db: &Path, sql: &str) -> i64 {
    Connection::open(db)
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

fn cache_dir(mount: &Path, image_id: &str) -> PathBuf {
    let h = qhash(image_id);
    mount
        .join(".kobo-images")
        .join((h & 0xFF).to_string())
        .join(((h >> 8) & 0xFF).to_string())
}

/// A device in the state Nickel leaves it: WAL-mode DB with no side files,
/// one imported book with chapters, bookmarks, shelf membership, ratings and
/// cached covers, plus a second untouched book.
fn fake_kobo(root: &Path) -> (KoboDevice, PathBuf) {
    let mount = root.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let db = mount.join(".kobo/KoboReader.sqlite");
    let conn = Connection::open(&db).unwrap();
    conn.execute_batch(SCHEMA).unwrap();
    for path in [BOOK, OTHER] {
        let id = cid(path);
        let image = image_id_for(&id);
        conn.execute_batch(&format!(
            "INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, Title, Attribution, ImageId, Accessibility, IsDownloaded, ReadStatus, ___PercentRead)
               VALUES ('{id}', 6, 'application/x-kobo-epub+zip', 'kepub_user', 'Old Title', 'Old Author', '{image}', -1, 'true', 1, 37);
             INSERT INTO content (ContentID, ContentType, MimeType, ___UserID, BookID, Title)
               VALUES ('{id}!!ch1', 9, 'application/xhtml+xml', 'kepub_user', '{id}', 'Chapter 1');
             INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex, StartOffset, EndContainerPath, EndContainerChildIndex, EndOffset) VALUES ('bm-{image}', '{id}', '{id}!!ch1', 'p', 0, 0, 'p', 0, 1);
             INSERT INTO ShelfContent (ShelfName, ContentId, _IsDeleted, _IsSynced) VALUES ('Fantasy', '{id}', 'false', 'false');
             INSERT INTO content_settings (ContentID, ContentType, DateModified) VALUES ('{id}', 6, '2026-10-31');
             INSERT INTO ratings (ContentID, Rating, DateModified) VALUES ('{id}', 5, '2026-10-31');"
        ))
        .unwrap();
        let file = mount.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, b"old book").unwrap();
        let cache = cache_dir(&mount, &image);
        fs::create_dir_all(&cache).unwrap();
        for ending in ["N3_FULL", "N3_LIBRARY_FULL", "N3_LIBRARY_GRID"] {
            fs::write(cache.join(format!("{image} - {ending}.parsed")), b"img").unwrap();
        }
    }
    conn.pragma_update(None, "journal_mode", "wal").unwrap();
    drop(conn);
    assert_no_side_files(&db);
    (KoboDevice::open(&mount).unwrap(), db)
}

fn assert_no_side_files(db: &Path) {
    for suffix in ["-wal", "-shm", "-journal"] {
        let side = PathBuf::from(format!("{}{suffix}", db.display()));
        assert!(!side.exists(), "{} left behind", side.display());
    }
}

#[test]
fn removes_book_rows_files_and_covers() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let id = cid(BOOK);

    let mut writer = kobo.open_writer(&backups).unwrap();
    let packed = writer.backup_path().to_path_buf();
    remove_book(&mut writer, &id).unwrap();
    writer.close().unwrap();
    let backup = dir.path().join("unpacked.sqlite");
    let bytes = zstd::decode_all(fs::File::open(&packed).unwrap()).unwrap();
    fs::write(&backup, bytes).unwrap();

    for table in [
        "content",
        "Bookmark",
        "ShelfContent",
        "content_settings",
        "ratings",
    ] {
        let col = if table == "Bookmark" {
            "VolumeID"
        } else {
            "ContentID"
        };
        assert_eq!(
            count(
                &db,
                &format!("SELECT count(*) FROM {table} WHERE {col} = '{id}'")
            ),
            0,
            "{table}"
        );
    }
    assert_eq!(
        count(
            &db,
            &format!("SELECT count(*) FROM content WHERE BookID = '{id}'")
        ),
        0
    );
    assert!(!kobo.mount().join(BOOK).exists());
    assert!(
        !kobo.mount().join("KEpub/Fantasy").exists(),
        "empty folder should go"
    );
    assert_eq!(
        fs::read_dir(cache_dir(kobo.mount(), &image_id_for(&id)))
            .unwrap()
            .count(),
        0
    );
    // The other book is untouched.
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT count(*) FROM content WHERE ContentID = '{}'",
                cid(OTHER)
            )
        ),
        1
    );
    assert!(kobo.mount().join(OTHER).exists());
    assert_no_side_files(&db);
    // The backup still has the removed book.
    assert_eq!(
        count(
            &backup,
            &format!("SELECT count(*) FROM content WHERE ContentID = '{id}'")
        ),
        1
    );
}

#[test]
fn replaces_book_keeping_progress() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let src = dir.path().join("new.kepub.epub");
    fs::write(&src, b"new book bytes").unwrap();

    let mut writer = kobo.open_writer(&dir.path().join("backups")).unwrap();
    replace_book(
        &mut writer,
        &src,
        BOOK,
        &DisplayMetadata {
            title: "The Lies of Locke Lamora".into(),
            author: Some("Scott Lynch".into()),
            series: Some("Gentleman Bastard".into()),
            series_number: Some("1".into()),
        },
    )
    .unwrap();
    writer.close().unwrap();

    assert_eq!(
        fs::read(kobo.mount().join(BOOK)).unwrap(),
        b"new book bytes"
    );
    let conn = Connection::open(&db).unwrap();
    let (title, author, series, seriesid, pct): (String, String, String, String, i64) = conn
        .query_row(
            "SELECT Title, Attribution, Series, SeriesID, ___PercentRead FROM content WHERE ContentID = ?1",
            [cid(BOOK)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
        )
        .unwrap();
    assert_eq!(
        (
            title.as_str(),
            author.as_str(),
            series.as_str(),
            seriesid.as_str(),
            pct
        ),
        (
            "The Lies of Locke Lamora",
            "Scott Lynch",
            "Gentleman Bastard",
            "Gentleman Bastard",
            37
        )
    );
    assert_eq!(
        count(
            &db,
            &format!(
                "SELECT count(*) FROM content WHERE BookID = '{}'",
                cid(BOOK)
            )
        ),
        1
    );
    assert_eq!(
        fs::read_dir(cache_dir(kobo.mount(), &image_id_for(&cid(BOOK))))
            .unwrap()
            .count(),
        0
    );
    drop(conn);
    assert_no_side_files(&db);
}

#[test]
fn removes_book_not_yet_imported() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let fresh = "KEpub/Fresh.kepub.epub";
    fs::write(kobo.mount().join(fresh), b"x").unwrap();
    let mut writer = kobo.open_writer(&dir.path().join("backups")).unwrap();
    remove_book(&mut writer, &cid(fresh)).unwrap();
    writer.close().unwrap();
    assert!(!kobo.mount().join(fresh).exists());
}

#[test]
fn refuses_unclean_or_untested_databases() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let wal = PathBuf::from(format!("{}-wal", db.display()));
    fs::write(&wal, b"").unwrap();
    assert!(matches!(
        kobo.open_writer(dir.path()),
        Err(Error::DatabaseBusy(_))
    ));
    fs::remove_file(&wal).unwrap();

    let conn = Connection::open(&db).unwrap();
    conn.execute("UPDATE DbVersion SET version = 999", [])
        .unwrap();
    drop(conn);
    assert!(matches!(
        kobo.open_writer(dir.path()),
        Err(Error::UntestedDbVersion(999))
    ));
}

#[test]
fn failed_transaction_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let mut writer = kobo.open_writer(&dir.path().join("backups")).unwrap();
    let result: Result<(), Error> = writer.transaction(|tx| {
        tx.execute("DELETE FROM content", [])?;
        Err(Error::Eject("simulated failure mid-change".into()))
    });
    assert!(result.is_err());
    writer.close().unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );
}

#[test]
fn restores_backup_and_prunes_old_ones() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let first = {
        let writer = kobo.open_writer(&backups).unwrap();
        let p = writer.backup_path().to_path_buf();
        writer.close().unwrap();
        p
    };
    let first_name = first.file_name().unwrap().to_string_lossy().into_owned();
    Connection::open(&db)
        .unwrap()
        .execute("DELETE FROM content", [])
        .unwrap();
    restore(&db, &first).unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );

    for _ in 0..BACKUPS_KEPT + 2 {
        kobo.open_writer(&backups).unwrap().close().unwrap();
    }
    // The newest ones, plus the day's earliest.
    assert_eq!(kobo.backups(&backups).len(), BACKUPS_KEPT + 1);
    assert_eq!(kobo.backups(&backups).last().unwrap().file, first_name);
}

/// The backup taken for a session that removes `path`.
fn remove_with_backup(kobo: &KoboDevice, backups: &Path, path: &str) -> String {
    let mut writer = kobo.open_writer(backups).unwrap();
    let name = writer.backup_path().file_name().unwrap().to_owned();
    remove_book(&mut writer, &cid(path)).unwrap();
    writer.close().unwrap();
    name.to_string_lossy().into_owned()
}

#[test]
fn going_back_brings_removed_books_back_with_their_rows() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let file = kobo.mount().join(BOOK);
    let bytes = fs::read(&file).unwrap();

    let backup = remove_with_backup(&kobo, &backups, BOOK);
    remove_with_backup(&kobo, &backups, OTHER);
    assert!(!file.exists());

    let listed = kobo.backups(&backups);
    let entry = listed.iter().find(|b| b.file == backup).unwrap();
    assert_eq!(entry.entries[0].op, "remove");
    assert_eq!(entry.removed_files, 1);
    assert_eq!(entry.blocked, None);

    let outcome = kobo.restore_backup(&backups, Path::new(&backup)).unwrap();
    // This change's file and the later one's.
    assert_eq!(outcome.files_restored, 2);
    assert_eq!(fs::read(&file).unwrap(), bytes);
    assert!(kobo.mount().join(OTHER).exists());
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );
    assert_eq!(count(&db, "SELECT count(*) FROM Bookmark"), 2);
    assert_eq!(kobo.parallax_state().log[0].op, "restore");
}

#[test]
fn going_back_can_be_undone() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let backup = remove_with_backup(&kobo, &backups, BOOK);

    let outcome = kobo.restore_backup(&backups, Path::new(&backup)).unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );
    let safety = kobo
        .backups(&backups)
        .into_iter()
        .find(|b| b.file == outcome.safety_backup)
        .unwrap();
    assert_eq!(safety.entries[0].op, "restore");

    kobo.restore_backup(&backups, Path::new(&outcome.safety_backup))
        .unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        1
    );
}

#[test]
fn going_back_leaves_a_book_sent_again_alone() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let backup = remove_with_backup(&kobo, &backups, BOOK);
    let file = kobo.mount().join(BOOK);
    fs::create_dir_all(file.parent().unwrap()).unwrap();
    fs::write(&file, b"newer copy").unwrap();

    let outcome = kobo.restore_backup(&backups, Path::new(&backup)).unwrap();
    assert_eq!(outcome.files_restored, 0);
    assert_eq!(fs::read(&file).unwrap(), b"newer copy");
}

#[test]
fn refuses_a_backup_from_another_database_version() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let backup = remove_with_backup(&kobo, &backups, BOOK);
    Connection::open(&db)
        .unwrap()
        .execute("UPDATE DbVersion SET version = version + 1", [])
        .unwrap();
    let before = fs::read(&db).unwrap();

    assert!(kobo.backups(&backups)[0].blocked.is_some());
    let result = kobo.restore_backup(&backups, Path::new(&backup));
    assert!(matches!(result, Err(Error::CannotRestore(_))));
    assert_eq!(fs::read(&db).unwrap(), before);
    assert!(!kobo.mount().join(BOOK).exists());
}

#[test]
fn refuses_a_backup_from_another_kobo_and_other_files() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let backup = remove_with_backup(&kobo, &backups, BOOK);

    let elsewhere = backups.join("N999999999999");
    fs::create_dir_all(&elsewhere).unwrap();
    let foreign = elsewhere.join(&backup);
    fs::copy(backups.join("N000000000001").join(&backup), &foreign).unwrap();
    for path in [
        foreign.as_path(),
        Path::new("KoboReader-none.sqlite"),
        Path::new("notes.txt"),
    ] {
        assert!(matches!(
            kobo.restore_backup(&backups, path),
            Err(Error::CannotRestore(_))
        ));
    }
}

#[test]
fn refuses_a_damaged_backup_without_touching_the_kobo() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let backup = remove_with_backup(&kobo, &backups, BOOK);
    fs::write(
        backups.join("N000000000001").join(&backup),
        b"not a database",
    )
    .unwrap();
    let before = fs::read(&db).unwrap();

    assert!(kobo.restore_backup(&backups, Path::new(&backup)).is_err());
    assert_eq!(fs::read(&db).unwrap(), before);
}

#[test]
fn going_back_repairs_a_damaged_database_and_keeps_it() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let backup = remove_with_backup(&kobo, &backups, BOOK);
    fs::write(&db, b"this database was damaged").unwrap();

    let outcome = kobo.restore_backup(&backups, Path::new(&backup)).unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );
    let kept = backups.join("N000000000001").join(&outcome.safety_backup);
    let kept = zstd::decode_all(fs::File::open(kept).unwrap()).unwrap();
    assert_eq!(kept, b"this database was damaged");
}

#[test]
fn a_failed_copy_leaves_the_database_and_its_side_files() {
    let dir = tempfile::tempdir().unwrap();
    let (_kobo, db) = fake_kobo(dir.path());
    let wal = db.with_extension("sqlite-wal");
    fs::write(&wal, b"unapplied changes").unwrap();

    assert!(restore(&db, &dir.path().join("missing.sqlite")).is_err());
    assert!(wal.exists());
    assert!(!db.with_extension("sqlite.parallax-restore").exists());
}

/// A backup as older versions wrote it: an uncompressed `.sqlite`, with no
/// sidecar. Returns its name.
fn plain_backup(kobo: &KoboDevice, backups: &Path, stamp: &str) -> String {
    let writer = kobo.open_writer(backups).unwrap();
    let packed = writer.backup_path().to_path_buf();
    writer.close().unwrap();
    let name = format!("KoboReader-{stamp}.sqlite");
    let bytes = zstd::decode_all(fs::File::open(&packed).unwrap()).unwrap();
    fs::write(packed.with_file_name(&name), bytes).unwrap();
    fs::remove_file(&packed).unwrap();
    let stem = packed
        .file_name()
        .unwrap()
        .to_string_lossy()
        .replace(".sqlite.zst", "");
    fs::remove_file(packed.with_file_name(format!("{stem}.json"))).unwrap();
    name
}

#[test]
fn backups_are_compressed_and_list_without_being_unpacked() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let name = remove_with_backup(&kobo, &backups, BOOK);
    assert!(name.ends_with(".sqlite.zst"));

    let folder = backups.join("N000000000001");
    let packed = folder.join(&name);
    assert!(fs::metadata(&packed).unwrap().len() < fs::metadata(&db).unwrap().len());
    // Unreadable now, so a version can only have come from the sidecar.
    fs::write(&packed, b"damaged").unwrap();
    assert!(kobo.backups(&backups)[0].db_version.is_some());

    let leftovers: Vec<_> = fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|e| e == "tmp" || e == "unpacked" || e == "sqlite")
        })
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_cut_off_backup_is_refused_and_leaves_no_temporary_file() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let name = remove_with_backup(&kobo, &backups, BOOK);
    let folder = backups.join("N000000000001");
    let bytes = fs::read(folder.join(&name)).unwrap();
    fs::write(folder.join(&name), &bytes[..bytes.len() / 2]).unwrap();
    let before = fs::read(&db).unwrap();

    assert!(matches!(
        kobo.restore_backup(&backups, Path::new(&name)),
        Err(Error::CannotRestore(_))
    ));
    assert_eq!(fs::read(&db).unwrap(), before);
    let unpacked = fs::read_dir(&folder)
        .unwrap()
        .flatten()
        .any(|e| e.path().extension().is_some_and(|x| x == "unpacked"));
    assert!(!unpacked);
}

#[test]
fn an_uncompressed_backup_still_restores() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let name = plain_backup(&kobo, &backups, "20261101-120000.000");
    Connection::open(&db)
        .unwrap()
        .execute("DELETE FROM content", [])
        .unwrap();

    assert!(kobo.backups(&backups).iter().any(|b| b.file == name));
    kobo.restore_backup(&backups, Path::new(&name)).unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );
}

#[test]
fn existing_backups_are_compressed_and_keep_working_under_their_old_name() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let name = plain_backup(&kobo, &backups, "20261101-120000.000");
    let folder = backups.join("N000000000001");

    kobo.open_writer(&backups).unwrap().close().unwrap();
    assert!(!folder.join(&name).exists());
    let packed = format!("{name}.zst");
    let listed = kobo.backups(&backups);
    let entry = listed.iter().find(|b| b.file == packed).unwrap();
    assert!(entry.db_version.is_some());
    assert_eq!(entry.blocked, None);

    Connection::open(&db)
        .unwrap()
        .execute("DELETE FROM content", [])
        .unwrap();
    // A name listed just before it was compressed still finds it.
    kobo.restore_backup(&backups, Path::new(&name)).unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE BookID IS NULL"),
        2
    );
}

#[test]
fn an_interrupted_compression_is_finished_not_duplicated() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let name = plain_backup(&kobo, &backups, "20261101-120000.000");
    let folder = backups.join("N000000000001");
    let bytes = fs::read(folder.join(&name)).unwrap();
    let packed = zstd::encode_all(&bytes[..], 3).unwrap();
    fs::write(folder.join(format!("{name}.zst")), packed).unwrap();

    assert_eq!(kobo.backups(&backups).len(), 1);
    kobo.open_writer(&backups).unwrap().close().unwrap();
    assert!(!folder.join(&name).exists());
    assert_eq!(kobo.backups(&backups).len(), 2);
}

#[test]
fn keeps_backups_from_before_a_removal_past_the_recent_ones() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    // Not the day's earliest, which is kept anyway.
    kobo.open_writer(&backups).unwrap().close().unwrap();
    let removal = remove_with_backup(&kobo, &backups, BOOK);
    for _ in 0..BACKUPS_KEPT + 3 {
        let mut writer = kobo.open_writer(&backups).unwrap();
        writer.log(LogEntry::new("collection"));
        writer.close().unwrap();
    }

    let listed = kobo.backups(&backups);
    assert!(listed.iter().any(|b| b.file == removal));
    assert_eq!(listed.len(), BACKUPS_KEPT + 2);
}

#[test]
fn keeping_only_the_latest_leaves_one_backup_and_no_unreachable_files() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    remove_with_backup(&kobo, &backups, BOOK);
    for _ in 0..3 {
        kobo.open_writer(&backups).unwrap().close().unwrap();
    }
    assert!(kobo.backups(&backups).len() > 1);
    let held = backups.join("N000000000001/removed");
    assert_eq!(fs::read_dir(&held).unwrap().count(), 1);

    // Saving applies at once, without a write session.
    let latest = BackupPolicy {
        keep: Keep::Latest,
        max_bytes: None,
    };
    latest.save(&backups).unwrap();
    assert_eq!(BackupPolicy::load(&backups), latest);
    assert_eq!(kobo.backups(&backups).len(), 1);
    // The removal is older than every backup left, so it can't come back.
    assert_eq!(fs::read_dir(&held).unwrap().count(), 0);

    kobo.open_writer(&backups).unwrap().close().unwrap();
    assert_eq!(kobo.backups(&backups).len(), 1);
}

#[test]
fn a_size_limit_drops_the_oldest_backups_but_never_the_newest() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    for _ in 0..4 {
        kobo.open_writer(&backups).unwrap().close().unwrap();
    }
    let listed = kobo.backups(&backups);
    assert_eq!(listed.len(), 4);
    let newest = listed[0].file.clone();
    let one = backups_size(&backups) / 4;

    // Room for about two: the oldest go first.
    BackupPolicy {
        keep: Keep::History,
        max_bytes: Some(one * 2 + one / 2),
    }
    .save(&backups)
    .unwrap();
    let names: Vec<String> = kobo.backups(&backups).into_iter().map(|b| b.file).collect();
    assert_eq!(
        names,
        listed[..2]
            .iter()
            .map(|b| b.file.clone())
            .collect::<Vec<_>>()
    );
    assert!(backups_size(&backups) <= one * 2 + one / 2);

    // Even a limit nothing fits under keeps the newest.
    BackupPolicy {
        keep: Keep::History,
        max_bytes: Some(1),
    }
    .save(&backups)
    .unwrap();
    let left = kobo.backups(&backups);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].file, newest);
}

#[test]
fn deleting_every_backup_keeps_the_policy() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    let policy = BackupPolicy {
        keep: Keep::Latest,
        max_bytes: Some(1_000_000_000),
    };
    policy.save(&backups).unwrap();
    remove_with_backup(&kobo, &backups, BOOK);
    delete_all_backups(&backups).unwrap();
    assert!(kobo.backups(&backups).is_empty());
    assert!(!backups.join("N000000000001").exists());
    assert_eq!(BackupPolicy::load(&backups), policy);
}

#[test]
fn tidies_only_real_apple_double_files() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let m = kobo.mount();
    let apple_double = [0x00, 0x05, 0x16, 0x07, 0, 2, 0, 0];
    fs::write(m.join("KEpub/._Other - Someone.kepub.epub"), apple_double).unwrap();
    fs::write(m.join("._metadata.calibre"), apple_double).unwrap();
    fs::write(m.join("KEpub/._not-really"), b"user data").unwrap();
    fs::write(m.join(".kobo/._KoboReader.sqlite"), apple_double).unwrap();

    assert_eq!(tidy_apple_double(&kobo), 2);
    assert!(m.join("KEpub/._not-really").exists());
    assert!(
        m.join(".kobo/._KoboReader.sqlite").exists(),
        ".kobo is never touched"
    );
}

/// Removing a real book from a copy of the reference device's database keeps
/// it consistent (skipped when the private fixture is absent).
#[test]
fn removes_from_reference_db_copy() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/private/KoboReader-libra2-fw4.38-db174.sqlite");
    if !fixture.exists() {
        eprintln!("skipping: private fixture not present");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let db = mount.join(".kobo/KoboReader.sqlite");
    fs::copy(&fixture, &db).unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();
    let victim: String = Connection::open(&db)
        .unwrap()
        .query_row(
            "SELECT ContentID FROM content WHERE ContentType = 6 AND ContentID LIKE 'file:///mnt/onboard/%' ORDER BY ContentID LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let books_before = count(&db, "SELECT count(*) FROM content WHERE ContentType = 6");
    let mut writer = kobo.open_writer(&dir.path().join("backups")).unwrap();
    remove_book(&mut writer, &victim).unwrap();
    writer.close().unwrap();
    assert_eq!(
        count(&db, "SELECT count(*) FROM content WHERE ContentType = 6"),
        books_before - 1
    );
    let check: String = Connection::open(&db)
        .unwrap()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
    assert_no_side_files(&db);
}

/// The whole write path on a real FAT32 volume: no `-wal`/`-shm` or `._*`
/// files left, rename-over works. Opt-in (`just test-fat`).
#[cfg(target_os = "macos")]
#[test]
#[ignore = "mounts a disk image: just test-fat"]
fn writes_cleanly_on_fat32() {
    use std::process::Command;

    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("kobo.dmg");
    let mnt = dir.path().join("mnt");
    fs::create_dir(&mnt).unwrap();
    let run = |cmd: &str, args: &[&str]| {
        let out = Command::new(cmd).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{cmd} {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let (image_s, mnt_s) = (image.to_str().unwrap(), mnt.to_str().unwrap());
    run(
        "diskutil",
        &[
            "image",
            "create",
            "blank",
            "--format",
            "RAW",
            "--size",
            "60m",
            "--volumeName",
            "KOBOTEST",
            "--fs",
            "MS-DOS",
            image_s,
        ],
    );
    run(
        "hdiutil",
        &["attach", "-nobrowse", "-mountpoint", mnt_s, image_s],
    );

    let result = std::panic::catch_unwind(|| {
        // Build the device elsewhere, then copy it over with plain writes.
        let staging = tempfile::tempdir().unwrap();
        let (_, staged_db) = fake_kobo(staging.path());
        let src_mount = staged_db.parent().unwrap().parent().unwrap();
        run(
            "cp",
            &["-R", "-X", &format!("{}/.", src_mount.display()), mnt_s],
        );
        run("dot_clean", &["-m", mnt_s]);
        let kobo = KoboDevice::open(&mnt).unwrap();
        let db = kobo.db_path();

        let src = dir.path().join("new.kepub.epub");
        fs::write(&src, b"replacement").unwrap();
        let backups = dir.path().join("backups");
        let mut writer = kobo.open_writer(&backups).unwrap();
        let backup = writer.backup_path().file_name().unwrap().to_owned();
        replace_book(
            &mut writer,
            &src,
            OTHER,
            &DisplayMetadata {
                title: "New".into(),
                ..Default::default()
            },
        )
        .unwrap();
        remove_book(&mut writer, &cid(BOOK)).unwrap();
        writer.close().unwrap();

        assert_eq!(fs::read(mnt.join(OTHER)).unwrap(), b"replacement");
        assert_no_side_files(&db);

        // Going back replaces the database by rename and puts the book back.
        let outcome = kobo.restore_backup(&backups, Path::new(&backup)).unwrap();
        assert_eq!(outcome.files_restored, 1);
        assert!(mnt.join(BOOK).is_file());
        // On a copy: opening it here would itself create a `._` companion.
        let copy = dir.path().join("restored.sqlite");
        fs::write(&copy, fs::read(&db).unwrap()).unwrap();
        assert_eq!(
            count(&copy, "SELECT count(*) FROM content WHERE BookID IS NULL"),
            2
        );
        assert_no_side_files(&db);

        let mut leftovers = Vec::new();
        for sub in ["", ".kobo", "KEpub"] {
            for e in fs::read_dir(mnt.join(sub)).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("._")
                    || name.ends_with(".parallax-part")
                    || name.ends_with(".parallax-restore")
                {
                    leftovers.push(format!("{sub}/{name}"));
                }
            }
        }
        leftovers
    });
    run("hdiutil", &["detach", mnt_s]);
    let leftovers = result.unwrap();
    assert!(leftovers.is_empty(), "left behind: {leftovers:?}");
}

fn meta(title: &str) -> DisplayMetadata {
    DisplayMetadata {
        title: title.into(),
        author: None,
        series: None,
        series_number: None,
    }
}

#[test]
fn records_what_parallax_did() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let backups = dir.path().join("backups");
    assert!(
        kobo.parallax_state().device.is_none(),
        "nothing recorded yet"
    );

    let src = dir.path().join("new.kepub.epub");
    fs::write(&src, b"new book bytes").unwrap();
    let new = "KEpub/New - Author.kepub.epub";
    kobo.send_file_for(&src, new, Some(42)).unwrap();

    // Replacing keeps the library link and counts the write.
    fs::write(&src, b"edited book bytes!").unwrap();
    let mut writer = kobo.open_writer(&backups).unwrap();
    let backup_name = writer
        .backup_path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    replace_book(&mut writer, &src, new, &meta("New")).unwrap();
    remove_book(&mut writer, &cid(BOOK)).unwrap();
    writer.close().unwrap();

    let state = kobo.parallax_state();
    assert!(state.problems.is_empty(), "{:?}", state.problems);
    let device = state.device.unwrap();
    assert_eq!(device.serial, "N000000000001");
    assert_eq!(device.first_write.parallax_version, PARALLAX_VERSION);
    assert_eq!(device.last_write.db_version, Some(174));
    assert_eq!(device.last_write.firmware, "4.38.23697");

    assert_eq!(state.books.len(), 1, "the removed book is forgotten");
    let book = &state.books[0];
    assert_eq!(book.content_id, cid(new));
    assert_eq!(book.library_book, Some(42));
    assert_eq!(book.writes, 2);
    assert_eq!(book.size, 18);
    assert_eq!(book.sha256.len(), 64);

    let ops: Vec<&str> = state.log.iter().map(|e| e.op.as_str()).collect();
    assert_eq!(ops, ["remove", "replace", "send"], "newest first");
    assert_eq!(state.log[0].backup.as_deref(), Some(backup_name.as_str()));
    assert!(state.log.iter().all(|e| e.error.is_none()));

    // Only dot files under /.parallax, and no temporary files left.
    let mut names: Vec<String> = fs::read_dir(kobo.mount().join(".parallax"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    assert_eq!(names, ["books.json", "device.json", "log.jsonl"]);

    // A second session keeps the device id and first-write stamp.
    kobo.record(Record {
        entries: vec![LogEntry::new("tidy")],
        books: Vec::new(),
    });
    let again = kobo.parallax_state().device.unwrap();
    assert_eq!(again.device_id, device.device_id);
    assert_eq!(again.first_write, device.first_write);
}

#[test]
fn upgrades_device_records_on_write_and_reads_them_beforehand() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let src = dir.path().join("a.kepub.epub");
    fs::write(&src, b"abc").unwrap();
    kobo.send_file_for(&src, "KEpub/A.kepub.epub", Some(42))
        .unwrap();
    let current = kobo.mount().join(".parallax");
    let legacy = kobo.mount().join(".erc");
    for name in ["device.json", "books.json", "log.jsonl"] {
        let path = current.join(name);
        let json = fs::read_to_string(&path).unwrap();
        fs::write(&path, json.replace("parallax_version", "erc_version")).unwrap();
    }
    fs::rename(&current, &legacy).unwrap();

    let before = kobo.parallax_state();
    assert!(before.problems.is_empty());
    assert_eq!(kobo.managed_books().unwrap()[0].library_book, Some(42));
    assert_eq!(before.log[0].op, "send");
    assert!(!current.exists(), "reading does not migrate the folder");

    kobo.send_file(&src, "KEpub/B.kepub.epub").unwrap();
    let after = kobo.parallax_state();
    assert!(after.problems.is_empty());
    assert!(!legacy.exists());
    assert_eq!(
        after.device.unwrap().device_id,
        before.device.unwrap().device_id
    );
    assert_eq!(after.books[0], before.books[0]);
    assert_eq!(after.log[1], before.log[0]);
    for name in ["device.json", "books.json", "log.jsonl"] {
        let json = fs::read_to_string(current.join(name)).unwrap();
        assert!(json.contains("parallax_version"));
        assert!(!json.contains("erc_version"));
    }
}

#[test]
fn spots_books_changed_or_removed_outside_parallax() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let src = dir.path().join("a.kepub.epub");
    fs::write(&src, b"abc").unwrap();
    kobo.send_file(&src, "KEpub/A.kepub.epub").unwrap();
    kobo.send_file(&src, "KEpub/B.kepub.epub").unwrap();
    kobo.send_file(&src, "KEpub/C.kepub.epub").unwrap();
    fs::write(kobo.mount().join("KEpub/B.kepub.epub"), b"changed").unwrap();
    fs::remove_file(kobo.mount().join("KEpub/C.kepub.epub")).unwrap();

    let checks = kobo.parallax_state().check(kobo.mount());
    let by_name: Vec<(&str, BookCheck)> = checks
        .iter()
        .map(|(id, c)| (id.rsplit('/').next().unwrap(), *c))
        .collect();
    assert_eq!(
        by_name,
        [
            ("A.kepub.epub", BookCheck::Unchanged),
            ("B.kepub.epub", BookCheck::Changed),
            ("C.kepub.epub", BookCheck::Missing),
        ]
    );
}

#[test]
fn unreadable_records_never_block_changes() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let parallax = kobo.mount().join(".parallax");
    fs::create_dir_all(&parallax).unwrap();
    fs::write(parallax.join("device.json"), b"{not json").unwrap();
    fs::write(parallax.join("books.json"), b"[]").unwrap();
    fs::write(parallax.join("log.jsonl"), b"garbage\n").unwrap();

    let state = kobo.parallax_state();
    assert_eq!(state.problems.len(), 3, "{:?}", state.problems);

    let src = dir.path().join("a.kepub.epub");
    fs::write(&src, b"abc").unwrap();
    kobo.send_file(&src, "KEpub/A.kepub.epub").unwrap();

    let state = kobo.parallax_state();
    assert!(state.device.is_some());
    assert_eq!(state.books.len(), 1);
    assert_eq!(state.log.len(), 1, "the bad log line is dropped on rewrite");
    // The unreadable files are kept beside the new ones for debugging.
    let aside = fs::read_dir(&parallax)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".unreadable-")
        })
        .count();
    assert_eq!(aside, 2);
}

#[test]
fn log_is_capped() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    let entries = (0..LOG_KEPT + 5)
        .map(|i| LogEntry::new("test").detail(i.to_string()))
        .collect();
    kobo.record(Record {
        entries,
        books: Vec::new(),
    });
    let log = kobo.parallax_state().log;
    assert_eq!(log.len(), LOG_KEPT);
    assert_eq!(log[0].detail.as_deref(), Some(&*(LOG_KEPT + 4).to_string()));
}

#[test]
fn refused_changes_are_not_logged() {
    let dir = tempfile::tempdir().unwrap();
    let (kobo, _db) = fake_kobo(dir.path());
    // remove_book refuses store books before touching anything, so the
    // session has nothing to record.
    let mut writer = kobo.open_writer(&dir.path().join("backups")).unwrap();
    assert!(remove_book(&mut writer, "store-book-id").is_err());
    writer.close().unwrap();
    assert!(kobo.parallax_state().log.is_empty());
}
