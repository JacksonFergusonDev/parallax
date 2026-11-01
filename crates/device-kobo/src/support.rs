//! Which Kobos ERC changes, decided before anything is written.
//!
//! Reading works on any Kobo whose database is new enough. Changing one also
//! needs a database schema calibre has validated, Kobo's own firmware (not a
//! Tolino reader or firmware 5, which store yes/no values as numbers rather
//! than the `'true'`/`'false'` text every write here uses; calibre
//! `KOBOTOUCH.needs_real_bools`) and the tables and columns those writes use.
//! Optional columns that older schemas lack (series, reading time) turn the
//! feature off instead of the whole device.

use rusqlite::Connection;
use serde::Serialize;

use crate::KoboDevice;
use crate::db::{MAX_TESTED_DB_VERSION, MIN_SUPPORTED_DB_VERSION, columns};
use crate::error::{Error, Result};
use crate::version::VersionInfo;

/// Newest firmware major version ERC writes to. Kobo's own firmware has been
/// 4.x since 2016; 5.x is Tolino's.
pub const MAX_WRITE_FIRMWARE_MAJOR: u32 = 4;

/// Columns every write needs, by table.
const REQUIRED: [(&str, &[&str]); 3] = [
    (
        "content",
        &[
            "ContentID",
            "BookID",
            "Title",
            "Attribution",
            "ImageId",
            "ReadStatus",
        ],
    ),
    (
        "Shelf",
        &[
            "Name",
            "InternalName",
            "CreationDate",
            "LastModified",
            "_IsDeleted",
            "_IsVisible",
            "_IsSynced",
        ],
    ),
    (
        "ShelfContent",
        &[
            "ShelfName",
            "ContentId",
            "DateModified",
            "_IsDeleted",
            "_IsSynced",
        ],
    ),
];

/// What ERC can do with a connected Kobo.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Support {
    pub db_version: Option<u32>,
    /// Why ERC won't change this Kobo; `None` when it will.
    pub read_only: Option<String>,
    /// The database has series columns (schema 65+).
    pub series: bool,
    /// The database tracks reading time (schema 168+).
    pub reading_time: bool,
}

impl KoboDevice {
    /// What ERC can do with this Kobo. Never fails: an unreadable database
    /// makes the device read-only.
    pub fn support(&self) -> Support {
        let unreadable = |reason: String| Support {
            db_version: None,
            read_only: Some(reason),
            series: false,
            reading_time: false,
        };
        match self.open_db() {
            Ok(db) => assess(self.version(), db.connection()).unwrap_or_else(|e| Support {
                db_version: Some(db.db_version()),
                ..unreadable(e.to_string())
            }),
            Err(Error::UnsupportedDbVersion(v)) => Support {
                db_version: Some(v),
                ..unreadable(Error::UnsupportedDbVersion(v).to_string())
            },
            Err(e) => unreadable(format!("could not read the Kobo's database: {e}")),
        }
    }

    /// Fails unless ERC may change this Kobo. Every change goes through
    /// here: database writes when a [`crate::DeviceWriter`] opens, file
    /// changes before they start.
    pub fn check_writable(&self) -> Result<()> {
        let db = self.open_db()?;
        match write_blocker(self.version(), db.connection())? {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

/// Assesses a database connection (read-only or the writer's own).
pub(crate) fn assess(version: &VersionInfo, conn: &Connection) -> Result<Support> {
    let content = columns(conn, "content")?;
    let has = |c: &str| content.contains(&c.to_ascii_lowercase());
    Ok(Support {
        db_version: Some(db_version(conn)?),
        read_only: write_blocker(version, conn)?.map(|e| e.to_string()),
        series: has("Series") && has("SeriesNumber"),
        reading_time: has("TimeSpentReading"),
    })
}

/// Why ERC must not change this Kobo, as the error a write returns.
pub(crate) fn write_blocker(version: &VersionInfo, conn: &Connection) -> Result<Option<Error>> {
    let db = db_version(conn)?;
    if db < MIN_SUPPORTED_DB_VERSION {
        return Ok(Some(Error::UnsupportedDbVersion(db)));
    }
    if db > MAX_TESTED_DB_VERSION {
        return Ok(Some(Error::UntestedDbVersion(db)));
    }
    if version.is_tolino() {
        return Ok(Some(Error::UnsupportedDevice(format!(
            "the {} runs Tolino's version of Kobo's software, which stores data differently",
            version.model_name()
        ))));
    }
    let major = version.firmware.0.first().copied().unwrap_or(0);
    if major > MAX_WRITE_FIRMWARE_MAJOR {
        return Ok(Some(Error::UnsupportedDevice(format!(
            "firmware {} is newer than any ERC knows how to change (up to {MAX_WRITE_FIRMWARE_MAJOR}.x)",
            version.firmware
        ))));
    }
    let mut missing = Vec::new();
    for (table, needed) in REQUIRED {
        let present = columns(conn, table)?;
        missing.extend(
            needed
                .iter()
                .filter(|c| !present.contains(&c.to_ascii_lowercase()))
                .map(|c| format!("{table}.{c}")),
        );
    }
    if !missing.is_empty() {
        return Ok(Some(Error::UnsupportedDevice(format!(
            "its database lacks {}",
            missing.join(", ")
        ))));
    }
    if integer_booleans(conn)? {
        return Ok(Some(Error::UnsupportedDevice(
            "its database stores yes/no values as numbers, which ERC doesn't write".into(),
        )));
    }
    Ok(None)
}

fn db_version(conn: &Connection) -> Result<u32> {
    Ok(conn.query_row("SELECT version FROM DbVersion", [], |r| r.get(0))?)
}

/// Whether yes/no columns hold 0/1 instead of `'true'`/`'false'`. Kobo
/// firmware stores text in these (`IsDownloaded` is integer on Kobos too, so
/// it says nothing).
fn integer_booleans(conn: &Connection) -> Result<bool> {
    let content = columns(conn, "content")?;
    for column in ["FirstTimeReading", "IsEncrypted"] {
        if content.contains(&column.to_ascii_lowercase()) {
            let sql =
                format!("SELECT EXISTS(SELECT 1 FROM content WHERE typeof({column}) = 'integer')");
            if conn.query_row(&sql, [], |r| r.get::<_, bool>(0))? {
                return Ok(true);
            }
        }
    }
    for table in ["Shelf", "ShelfContent"] {
        let sql =
            format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE typeof(_IsDeleted) = 'integer')");
        if conn.query_row(&sql, [], |r| r.get::<_, bool>(0))? {
            return Ok(true);
        }
    }
    Ok(false)
}
