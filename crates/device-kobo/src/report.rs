//! A device report: what someone with a Kobo ERC doesn't know yet can send
//! so support for it can be added and tested without the device.
//!
//! It holds the model id, firmware, database version and schema (the
//! `CREATE` statements, no rows), how yes/no values are stored, row counts,
//! the cover-cache file kinds with their pixel sizes, and ERC's own verdict.
//! It never holds the serial number, book titles, file names, reading data,
//! highlights or account details. Gathering it only reads: the database is
//! opened immutable and nothing on the device is written.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};

use crate::KoboDevice;
use crate::db::{columns, open_immutable};
use crate::state::ERC_VERSION;

/// Bumped when fields change meaning.
pub const REPORT_FORMAT: u32 = 1;

/// Yes/no columns whose storage type (text or integer) matters to writes.
const BOOLEAN_COLUMNS: [(&str, &str); 6] = [
    ("content", "IsDownloaded"),
    ("content", "FirstTimeReading"),
    ("content", "IsEncrypted"),
    ("Shelf", "_IsDeleted"),
    ("ShelfContent", "_IsDeleted"),
    ("Bookmark", "Hidden"),
];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceReport {
    pub format: u32,
    pub erc_version: String,
    pub created: String,
    /// From `.kobo/version`, e.g. `00000000-0000-0000-0000-000000000388`.
    pub model_id: String,
    /// ERC's name for it; "Kobo eReader" when the id is new to ERC.
    pub model_name: String,
    pub firmware: String,
    pub db_version: Option<u32>,
    /// Why ERC would only read this Kobo; `None` when it would change it.
    pub read_only: Option<String>,
    /// `CREATE` statements for the database's tables, indexes and triggers.
    pub schema: Vec<String>,
    /// Rows per table.
    pub rows: BTreeMap<String, u64>,
    /// `table.column` → SQLite storage type → rows.
    pub value_types: BTreeMap<String, BTreeMap<String, u64>>,
    /// `content.ContentType` → rows (6 book, 9 chapter, 899 contents entry…).
    pub content_types: BTreeMap<String, u64>,
    /// `Bookmark.Type` → rows (highlight, note, dogear, markup…).
    pub annotation_types: BTreeMap<String, u64>,
    /// Cover-cache kind (`N3_FULL`…) → files and the pixel sizes seen.
    pub cover_files: BTreeMap<String, CoverFiles>,
    /// Dot folders at the top of the device, which belong to the system or
    /// to apps (`.kobo`, `.adds`…); other folders are the owner's.
    pub system_folders: Vec<String>,
    /// What couldn't be read, so a partial report still helps.
    pub problems: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CoverFiles {
    pub files: u64,
    /// Distinct (width, height), at most a few.
    pub sizes: Vec<(u32, u32)>,
}

impl KoboDevice {
    /// Gathers a [`DeviceReport`]. Never fails: whatever can't be read is
    /// listed in `problems`.
    pub fn report(&self) -> DeviceReport {
        let version = self.version();
        let mut report = DeviceReport {
            format: REPORT_FORMAT,
            erc_version: ERC_VERSION.to_owned(),
            created: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            model_id: version.model_id.clone(),
            model_name: version.model_name().to_owned(),
            firmware: version.firmware.to_string(),
            db_version: None,
            read_only: None,
            schema: Vec::new(),
            rows: BTreeMap::new(),
            value_types: BTreeMap::new(),
            content_types: BTreeMap::new(),
            annotation_types: BTreeMap::new(),
            cover_files: BTreeMap::new(),
            system_folders: Vec::new(),
            problems: Vec::new(),
        };
        let support = self.support();
        report.db_version = support.db_version;
        report.read_only = support.read_only;
        match open_immutable(&self.db_path()) {
            Ok(conn) => {
                if let Err(e) = read_database(&conn, &mut report) {
                    report.problems.push(format!("database: {e}"));
                }
            }
            Err(e) => report.problems.push(format!("database: {e}")),
        }
        report.cover_files = cover_files(&self.mount().join(".kobo-images"));
        if let Ok(entries) = fs::read_dir(self.mount()) {
            report.system_folders = entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with('.') && !n.starts_with("._"))
                .collect();
            report.system_folders.sort();
        }
        report
    }
}

fn read_database(conn: &Connection, report: &mut DeviceReport) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare(
        "SELECT name, type, sql FROM sqlite_master
         WHERE sql IS NOT NULL AND name NOT LIKE 'sqlite_%'
         ORDER BY CASE type WHEN 'table' THEN 0 WHEN 'index' THEN 1 ELSE 2 END, name",
    )?;
    let objects: Vec<(String, String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<_>>()?;
    for (name, kind, sql) in objects {
        if kind == "table" {
            let n = conn.query_row(&format!("SELECT count(*) FROM \"{name}\""), [], |r| {
                r.get::<_, u64>(0)
            })?;
            report.rows.insert(name, n);
        }
        report.schema.push(sql);
    }
    for (table, column) in BOOLEAN_COLUMNS {
        if columns(conn, table)?.contains(&column.to_ascii_lowercase()) {
            report.value_types.insert(
                format!("{table}.{column}"),
                grouped(
                    conn,
                    &format!("SELECT typeof({column}), count(*) FROM {table} GROUP BY 1"),
                )?,
            );
        }
    }
    if columns(conn, "content")?.contains("contenttype") {
        report.content_types = grouped(
            conn,
            "SELECT coalesce(ContentType, 'NULL'), count(*) FROM content GROUP BY 1",
        )?;
    }
    if columns(conn, "Bookmark")?.contains("type") {
        report.annotation_types = grouped(
            conn,
            "SELECT coalesce(Type, 'NULL'), count(*) FROM Bookmark GROUP BY 1",
        )?;
    }
    Ok(())
}

/// Runs a `SELECT key, count` query into a map.
fn grouped(conn: &Connection, sql: &str) -> rusqlite::Result<BTreeMap<String, u64>> {
    let mut stmt = conn.prepare(sql)?;
    stmt.query_map([], |r| {
        let key: rusqlite::types::Value = r.get(0)?;
        let key = match key {
            rusqlite::types::Value::Text(s) => s,
            rusqlite::types::Value::Integer(i) => i.to_string(),
            other => format!("{other:?}"),
        };
        Ok((key, r.get::<_, u64>(1)?))
    })?
    .collect()
}

/// Kinds of cached cover (`<ImageId> - N3_FULL.parsed` → `N3_FULL`), with
/// the sizes of the first few files of each kind.
fn cover_files(root: &Path) -> BTreeMap<String, CoverFiles> {
    const SIZES_KEPT: usize = 3;
    const SAMPLED: u64 = 5;
    let mut out: BTreeMap<String, CoverFiles> = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(path);
                continue;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some((_, kind)) = name.rsplit_once(" - ") else {
                continue;
            };
            let kind = kind.trim_end_matches(".parsed").to_owned();
            let stats = out.entry(kind).or_default();
            stats.files += 1;
            if stats.files <= SAMPLED
                && stats.sizes.len() < SIZES_KEPT
                && let Some(size) = image_size(&path)
                && !stats.sizes.contains(&size)
            {
                stats.sizes.push(size);
            }
        }
    }
    out
}

/// Width and height of a JPEG or PNG from its header.
fn image_size(path: &Path) -> Option<(u32, u32)> {
    let mut head = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    if head.starts_with(b"\x89PNG\r\n\x1a\n") && head.len() >= 24 {
        let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        return Some((be(&head[16..20]), be(&head[20..24])));
    }
    if !head.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    // Walk the JPEG segments to the first start-of-frame marker.
    let mut i = 2;
    while i + 9 < head.len() {
        if head[i] != 0xff {
            return None;
        }
        let marker = head[i + 1];
        let len = usize::from(u16::from_be_bytes([head[i + 2], head[i + 3]]));
        if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            let h = u16::from_be_bytes([head[i + 5], head[i + 6]]);
            let w = u16::from_be_bytes([head[i + 7], head[i + 8]]);
            return Some((w.into(), h.into()));
        }
        i += 2 + len;
    }
    None
}
