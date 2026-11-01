//! Read access to `KoboReader.sqlite`.
//!
//! Queries are ported from calibre's KoboTouch driver
//! (src/calibre/devices/kobo/driver.py, GPLv3), which has tracked Nickel's
//! schema across a decade of firmware.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, NaiveDateTime, Utc};
use erc_device::{Collection, DeviceBook, ReadStatus};
use rusqlite::{Connection, OpenFlags, Row};

use crate::error::{Error, Result};

/// calibre `KOBOTOUCH.min_supported_dbversion`.
pub const MIN_SUPPORTED_DB_VERSION: u32 = 53;
/// calibre `KOBOTOUCH.supported_dbversion`: newest schema calibre has been
/// validated against. Newer databases are read but not written.
pub const MAX_TESTED_DB_VERSION: u32 = 222;

/// `ContentID` prefix of books in the device's internal storage.
pub const ONBOARD_PREFIX: &str = "file:///mnt/onboard/";
const SD_PREFIX: &str = "file:///mnt/sd/";

pub struct KoboDb {
    conn: Connection,
    version: u32,
}

impl KoboDb {
    /// Opens the database without creating `-wal`/`-shm` files or taking
    /// locks, which is safe while the device is in USB mode (Nickel is not
    /// writing). Never use this connection after writing through another.
    pub fn open_read_only(path: &Path) -> Result<Self> {
        let uri = format!("file:{}?immutable=1", uri_escape(&path.to_string_lossy()));
        let conn = Connection::open_with_flags(
            uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY
                | OpenFlags::SQLITE_OPEN_URI
                | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        let version: u32 = conn.query_row("SELECT version FROM DbVersion", [], |r| r.get(0))?;
        if version < MIN_SUPPORTED_DB_VERSION {
            return Err(Error::UnsupportedDbVersion(version));
        }
        Ok(Self { conn, version })
    }

    /// The read-only connection, for queries beyond the ones here.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn db_version(&self) -> u32 {
        self.version
    }

    /// Whether this schema is one calibre has validated writes against.
    pub fn writes_supported(&self) -> bool {
        self.version <= MAX_TESTED_DB_VERSION
    }

    /// Books on internal storage, as calibre lists them with default settings
    /// (no archived store books, previews or recommendations).
    pub fn books(&self) -> Result<Vec<DeviceBook>> {
        let shelves = self.memberships()?;
        let cols = self.content_columns()?;
        let col = |name: &str| {
            if cols.iter().any(|c| c.eq_ignore_ascii_case(name)) {
                name.to_owned()
            } else {
                format!("NULL AS {name}")
            }
        };
        // calibre KOBOTOUCH.books(): the supports_kobo_archive/overdrive branch.
        let sql = format!(
            "SELECT ContentID, Title, Attribution, ReadStatus, ___PercentRead, DateLastRead, \
                    {time}, DateCreated, ___FileSize, {series}, {series_number} \
             FROM content \
             WHERE BookID IS NULL \
               AND ((Accessibility = -1 AND IsDownloaded IN ('true', 1)) \
                    OR (Accessibility IN (1, 2, 8, 9) AND IsDownloaded IN ('true', 1))) \
               AND ({external_id} IS NULL OR {external_id} = '') \
               AND ContentID NOT LIKE '{SD_PREFIX}%' \
             ORDER BY Title COLLATE NOCASE",
            time = col("TimeSpentReading"),
            series = col("Series"),
            series_number = col("SeriesNumber"),
            external_id = if cols.iter().any(|c| c == "ExternalId") {
                "ExternalId"
            } else {
                "NULL"
            },
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let books = stmt
            .query_map([], |row| book_from_row(row, &shelves))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(books)
    }

    /// Nickel's cover cache identifiers, which can differ from the ContentID.
    pub fn image_ids(&self) -> Result<HashMap<String, String>> {
        let mut stmt = self.conn.prepare(
            "SELECT ContentID, ImageId FROM content WHERE BookID IS NULL AND ImageId IS NOT NULL AND ImageId != ''",
        )?;
        Ok(stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Collections that are not soft-deleted, with their member counts.
    pub fn collections(&self) -> Result<Vec<Collection>> {
        let mut stmt = self.conn.prepare(
            "SELECT s.Name, \
                    (SELECT count(*) FROM ShelfContent sc \
                     WHERE sc.ShelfName = s.Name AND NOT (sc._IsDeleted = 'true' OR sc._IsDeleted = 1)) \
             FROM Shelf s \
             WHERE NOT (s._IsDeleted = 'true' OR s._IsDeleted = 1) AND s.Name IS NOT NULL \
               AND s.InternalName NOT IN ('Shortlist', 'Wishlist') \
               AND (s.Type IS NULL OR s.Type != 'SystemTag') \
             ORDER BY s.Name COLLATE NOCASE",
        )?;
        let collections = stmt
            .query_map([], |row| {
                Ok(Collection {
                    name: row.get(0)?,
                    book_count: row.get::<_, i64>(1)? as usize,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(collections)
    }

    /// Collection memberships by ContentID, including books Nickel hasn't
    /// imported yet. calibre KOBOTOUCH.get_bookshelvesforbook, batched.
    pub fn memberships(&self) -> Result<HashMap<String, Vec<String>>> {
        let mut stmt = self.conn.prepare(
            "SELECT ContentId, ShelfName FROM ShelfContent \
             WHERE NOT (_IsDeleted = 'true' OR _IsDeleted = 1) AND ShelfName IS NOT NULL \
             ORDER BY ShelfName COLLATE NOCASE",
        )?;
        let mut map: HashMap<String, Vec<String>> = HashMap::new();
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for row in rows {
            let (content_id, shelf) = row?;
            map.entry(content_id).or_default().push(shelf);
        }
        Ok(map)
    }

    fn content_columns(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT name FROM pragma_table_info('content')")?;
        let cols = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(cols)
    }
}

fn book_from_row(
    row: &Row,
    shelves: &HashMap<String, Vec<String>>,
) -> rusqlite::Result<DeviceBook> {
    let id: String = row.get("ContentID")?;
    let path = id.strip_prefix(ONBOARD_PREFIX).map(str::to_owned);
    let percent: Option<i64> = row.get("___PercentRead")?;
    Ok(DeviceBook {
        title: row.get::<_, Option<String>>("Title")?.unwrap_or_default(),
        author: non_empty(row.get("Attribution")?),
        sideloaded: path.is_some(),
        path,
        series: non_empty(row.get("Series")?),
        series_number: non_empty(row.get("SeriesNumber")?),
        read_status: match row.get::<_, Option<i64>>("ReadStatus")? {
            Some(1) => ReadStatus::Reading,
            Some(2) => ReadStatus::Finished,
            _ => ReadStatus::Unread,
        },
        percent_read: percent.unwrap_or(0).clamp(0, 100) as u8,
        last_read: parse_kobo_date(row.get("DateLastRead")?),
        reading_time_secs: row
            .get::<_, Option<i64>>("TimeSpentReading")?
            .filter(|&t| t > 0)
            .map(|t| t as u64),
        added: parse_kobo_date(row.get("DateCreated")?),
        file_size: row
            .get::<_, Option<i64>>("___FileSize")?
            .filter(|&s| s > 0)
            .map(|s| s as u64),
        collections: shelves.get(&id).cloned().unwrap_or_default(),
        id,
    })
}

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}

/// Nickel stores UTC timestamps such as `2024-10-10T06:36:38.327`, sometimes
/// with a `Z` or offset suffix.
pub fn parse_kobo_date(s: Option<String>) -> Option<DateTime<Utc>> {
    let s = s?;
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&Utc));
    }
    NaiveDateTime::parse_from_str(s.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S%.f")
        .ok()
        .map(|naive| naive.and_utc())
}

/// Escapes characters that are special in SQLite URI filenames.
fn uri_escape(path: &str) -> String {
    path.replace('%', "%25")
        .replace('?', "%3f")
        .replace('#', "%23")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nickel_dates() {
        let d = parse_kobo_date(Some("2024-10-10T06:36:38.327".into())).unwrap();
        assert_eq!(d.to_rfc3339(), "2024-10-10T06:36:38.327+00:00");
        assert!(parse_kobo_date(Some("2024-10-09T15:27:08Z".into())).is_some());
        assert!(parse_kobo_date(Some("".into())).is_none());
        assert!(parse_kobo_date(Some("garbage".into())).is_none());
    }

    #[test]
    fn escapes_uri_specials() {
        assert_eq!(uri_escape("/a?b#c%d"), "/a%3fb%23c%25d");
    }
}
