use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use erc_device_kobo::{
    BACKUPS_KEPT, DisplayMetadata, Error, KoboDevice, image_id_for, qhash, remove_book,
    replace_book, restore, tidy_apple_double,
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
    let backup = writer.backup_path().to_path_buf();
    remove_book(&mut writer, &id).unwrap();
    writer.close().unwrap();

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
    let kept = fs::read_dir(backups.join("N000000000001")).unwrap().count();
    assert_eq!(kept, BACKUPS_KEPT);
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
#[ignore]
fn writes_cleanly_on_fat32() {
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
        let mut writer = kobo.open_writer(&dir.path().join("backups")).unwrap();
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
        let mut leftovers = Vec::new();
        for sub in ["", ".kobo", "KEpub"] {
            for e in fs::read_dir(mnt.join(sub)).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("._")
                    || name.ends_with(".erc-part")
                    || name.ends_with(".erc-restore")
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
