use std::fs;
use std::path::{Path, PathBuf};

use erc_device::ReadStatus;
use erc_device_kobo::{
    DeviceReport, Error, KoboDevice, add_to_collection, create_collection, remove_orphaned_images,
    set_read_status, set_series,
};
use rusqlite::{Connection, params};

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const LIBRA2: &str = "00000000-0000-0000-0000-000000000388";
const BOOK: &str = "file:///mnt/onboard/KEpub/Book.kepub.epub";

/// A Kobo with `schema`, the given model id and firmware, and one book.
fn kobo(root: &Path, schema: &str, model_id: &str, firmware: &str) -> KoboDevice {
    let mount = root.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(
        mount.join(".kobo/version"),
        format!("N000000000001,4.1.15,{firmware},4.1.15,4.1.15,{model_id}"),
    )
    .unwrap();
    let conn = Connection::open(mount.join(".kobo/KoboReader.sqlite")).unwrap();
    conn.execute_batch(schema).unwrap();
    insert_book(&conn);
    drop(conn);
    KoboDevice::open(mount).unwrap()
}

/// Inserts a sideloaded book, filling whatever other required columns this
/// schema has, so it works for any schema a device report brings.
fn insert_book(conn: &Connection) {
    let mut stmt = conn
        .prepare("SELECT name FROM pragma_table_info('content') WHERE \"notnull\" AND dflt_value IS NULL")
        .unwrap();
    let required: Vec<String> = stmt
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    let mut values: Vec<(String, String)> = [
        ("ContentID", BOOK),
        ("ContentType", "6"),
        ("MimeType", "application/x-kobo-epub+zip"),
        ("Title", "Book"),
        ("ReadStatus", "1"),
        ("FirstTimeReading", "false"),
    ]
    .into_iter()
    .map(|(n, v)| (n.to_owned(), v.to_owned()))
    .collect();
    for name in required {
        if !values.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
            values.push((name, String::new()));
        }
    }
    let cols: Vec<String> = conn
        .prepare("SELECT lower(name) FROM pragma_table_info('content')")
        .unwrap()
        .query_map([], |r| r.get(0))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap();
    values.retain(|(n, _)| cols.contains(&n.to_ascii_lowercase()));
    let names: Vec<&str> = values.iter().map(|(n, _)| n.as_str()).collect();
    let marks: Vec<String> = (1..=values.len()).map(|i| format!("?{i}")).collect();
    conn.execute(
        &format!(
            "INSERT INTO content ({}) VALUES ({})",
            names.join(", "),
            marks.join(", ")
        ),
        rusqlite::params_from_iter(values.iter().map(|(_, v)| v)),
    )
    .unwrap();
}

/// Every kind of change ERC makes, in one session; the writer's integrity
/// check runs on close.
fn exercise_writes(kobo: &KoboDevice, backups: &Path) {
    let mut writer = kobo.open_writer(backups).unwrap();
    writer
        .transaction(|tx| {
            create_collection(tx, "Fantasy")?;
            add_to_collection(tx, "Fantasy", &[BOOK.to_owned()])?;
            set_series(tx, BOOK, Some("Saga"), Some("2"))
        })
        .unwrap();
    set_read_status(
        &mut writer,
        &[BOOK.to_owned()],
        ReadStatus::Unread,
        true,
        true,
    )
    .unwrap();
    writer.close().unwrap();
    kobo.open_db().unwrap().annotations(None).unwrap();
}

fn assert_read_only(kobo: &KoboDevice, dir: &Path, reason: &str) {
    let support = kobo.support();
    let why = support.read_only.expect("should be read-only");
    assert!(why.contains(reason), "{why}");
    assert_eq!(kobo.info().read_only.as_deref(), Some(why.as_str()));
    assert!(matches!(
        kobo.open_writer(&dir.join("backups")),
        Err(Error::UnsupportedDevice(_))
    ));
    let src = dir.join("book.kepub.epub");
    fs::write(&src, b"book").unwrap();
    assert!(matches!(
        kobo.send_file(&src, "KEpub/New.kepub.epub"),
        Err(Error::UnsupportedDevice(_))
    ));
    assert!(!kobo.mount().join("KEpub/New.kepub.epub").exists());
    assert!(matches!(
        remove_orphaned_images(kobo),
        Err(Error::UnsupportedDevice(_))
    ));
    // Reading still works.
    assert_eq!(kobo.open_db().unwrap().books().unwrap().len(), 1);
}

#[test]
fn libra2_is_supported() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = kobo(dir.path(), SCHEMA, LIBRA2, "4.38.23697");
    let support = kobo.support();
    assert_eq!(support.read_only, None);
    assert_eq!(support.db_version, Some(174));
    assert!(support.series && support.reading_time);
    exercise_writes(&kobo, &dir.path().join("backups"));
}

#[test]
fn tolino_readers_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    // Even on 4.x firmware; the model decides.
    let kobo = kobo(
        dir.path(),
        SCHEMA,
        "00000000-0000-0000-0000-000000000693",
        "4.38.23697",
    );
    assert_read_only(&kobo, dir.path(), "Tolino");
}

#[test]
fn firmware_5_is_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = kobo(dir.path(), SCHEMA, LIBRA2, "5.1.0");
    assert_read_only(&kobo, dir.path(), "firmware 5.1.0");
}

#[test]
fn numeric_yes_no_values_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = kobo(dir.path(), SCHEMA, LIBRA2, "4.38.23697");
    Connection::open(kobo.db_path())
        .unwrap()
        .execute("UPDATE content SET FirstTimeReading = 0", [])
        .unwrap();
    assert_read_only(&kobo, dir.path(), "as numbers");
}

#[test]
fn missing_columns_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let schema = SCHEMA.replace("_IsSynced\tBOOL, _SyncTime", "_SyncTime");
    assert_ne!(schema, SCHEMA);
    let kobo = kobo(dir.path(), &schema, LIBRA2, "4.38.23697");
    assert_read_only(&kobo, dir.path(), "Shelf._IsSynced");
}

#[test]
fn untested_database_versions_are_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = kobo(dir.path(), SCHEMA, LIBRA2, "4.38.23697");
    Connection::open(kobo.db_path())
        .unwrap()
        .execute("UPDATE DbVersion SET version = 999", [])
        .unwrap();
    let why = kobo.support().read_only.unwrap();
    assert!(why.contains("newer than any tested"), "{why}");
    assert!(matches!(
        kobo.send_file(&dir.path().join("x"), "KEpub/x.kepub.epub"),
        Err(Error::UntestedDbVersion(999))
    ));
}

#[test]
fn older_schemas_skip_columns_they_lack() {
    let dir = tempfile::tempdir().unwrap();
    // Before series (65), reading time (168), shelf ids and bookmark types.
    let mut schema = SCHEMA.to_owned();
    for (from, to) in [
        (" Series TEXT, SeriesNumber TEXT,", ""),
        (" SeriesID TEXT, SeriesNumberFloat REAL,", ""),
        (" TimeSpentReading INTEGER,", ""),
        (" RestOfBookEstimate INTEGER,", ""),
        ("\tId\t\tTEXT,\n", ""),
        ("\tType\t\tTEXT,\n", ""),
        (
            "LastAccessed TEXT,\n\tPRIMARY KEY(Id)",
            "LastAccessed TEXT,\n\tPRIMARY KEY(Name)",
        ),
        ("CREATE INDEX shelf_id_index ON shelf (Id);", ""),
        ("ContextString TEXT, Type TEXT,", "ContextString TEXT,"),
    ] {
        assert!(schema.contains(from), "{from}");
        schema = schema.replacen(from, to, 1);
    }
    let kobo = kobo(dir.path(), &schema, LIBRA2, "4.38.23697");
    let support = kobo.support();
    assert_eq!(support.read_only, None);
    assert!(!support.series && !support.reading_time);
    exercise_writes(&kobo, &dir.path().join("backups"));
    let conn = Connection::open(kobo.db_path()).unwrap();
    let (status, shelf): (i64, String) = conn
        .query_row(
            "SELECT ReadStatus, (SELECT Name FROM Shelf) FROM content WHERE ContentID = ?1",
            [BOOK],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((status, shelf.as_str()), (0, "Fantasy"));
}

#[test]
fn report_describes_the_device_without_personal_data() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = kobo(dir.path(), SCHEMA, LIBRA2, "4.38.23697");
    let conn = Connection::open(kobo.db_path()).unwrap();
    conn.execute(
        "UPDATE content SET Title = 'SECRET TITLE', Attribution = 'SECRET AUTHOR'",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO Bookmark (BookmarkID, VolumeID, ContentID, StartContainerPath, StartContainerChildIndex, StartOffset,
             EndContainerPath, EndContainerChildIndex, EndOffset, Text, Annotation, Hidden, Type)
         VALUES ('b1', ?1, ?1, 'p', 0, 0, 'p', 0, 1, 'SECRET QUOTE', 'SECRET NOTE', 'false', 'highlight')",
        params![BOOK],
    )
    .unwrap();
    drop(conn);
    let mount = kobo.mount();
    fs::create_dir_all(mount.join("KEpub/SECRET FOLDER")).unwrap();
    fs::create_dir_all(mount.join(".adds")).unwrap();
    let covers = mount.join(".kobo-images/12/34");
    fs::create_dir_all(&covers).unwrap();
    let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    png.extend(1264u32.to_be_bytes());
    png.extend(1680u32.to_be_bytes());
    fs::write(covers.join("SECRET_ID - N3_FULL.parsed"), &png).unwrap();
    // A baseline JPEG: SOI, an APP0 segment, then SOF0 (height 530, width 355).
    let jpeg = [
        0xff, 0xd8, 0xff, 0xe0, 0x00, 0x04, 0x00, 0x00, 0xff, 0xc0, 0x00, 0x11, 0x08, 0x02, 0x12,
        0x01, 0x63, 0x03,
    ];
    fs::write(covers.join("SECRET_ID - N3_LIBRARY_FULL.parsed"), jpeg).unwrap();

    let report = kobo.report();
    let json = serde_json::to_string(&report).unwrap();
    for secret in ["SECRET", "N000000000001"] {
        assert!(!json.contains(secret), "{secret} leaked: {json}");
    }
    assert_eq!(report.model_name, "Kobo Libra 2");
    assert_eq!(report.db_version, Some(174));
    assert_eq!(report.read_only, None);
    assert!(
        report
            .schema
            .iter()
            .any(|s| s.starts_with("CREATE TABLE content"))
    );
    assert_eq!(report.rows["content"], 1);
    assert_eq!(report.annotation_types["highlight"], 1);
    assert_eq!(report.value_types["content.FirstTimeReading"]["text"], 1);
    assert_eq!(report.cover_files["N3_FULL"].sizes, [(1264, 1680)]);
    assert_eq!(report.cover_files["N3_LIBRARY_FULL"].sizes, [(355, 530)]);
    assert_eq!(report.system_folders, [".adds", ".kobo", ".kobo-images"]);
    assert!(report.problems.is_empty(), "{:?}", report.problems);
}

fn reports() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/kobo/reports");
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "json"))
        .collect();
    found.sort();
    found
}

/// Rebuilds each reported device's database from its schema and checks ERC
/// reaches the same verdict, then makes every kind of change on the ones it
/// supports. A new report in tests/fixtures/kobo/reports/ is all a new model
/// needs to be covered.
#[test]
fn reported_devices_replay() {
    let reports = reports();
    assert!(!reports.is_empty());
    for path in reports {
        let report: DeviceReport = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        let dir = tempfile::tempdir().unwrap();
        let mut schema = report.schema.join(";\n") + ";\n";
        if let Some(v) = report.db_version {
            schema.push_str(&format!("INSERT INTO DbVersion (version) VALUES ({v});"));
        }
        let kobo = kobo(dir.path(), &schema, &report.model_id, &report.firmware);
        // Store yes/no values the way the device does.
        for column in ["FirstTimeReading", "IsEncrypted"] {
            let numeric = report
                .value_types
                .get(&format!("content.{column}"))
                .is_some_and(|types| types.contains_key("integer"));
            if numeric {
                Connection::open(kobo.db_path())
                    .unwrap()
                    .execute(&format!("UPDATE content SET {column} = 0"), [])
                    .unwrap();
            }
        }
        assert_eq!(kobo.support().read_only, report.read_only, "{name}");
        if report.read_only.is_none() {
            exercise_writes(&kobo, &dir.path().join("backups"));
        }
    }
}
