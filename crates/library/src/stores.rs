//! Small stores used by `erc-core` workflows: which books are on a device,
//! duplicate groups the user dismissed, app settings, and reading progress
//! seen on devices.

use std::collections::HashSet;

use rusqlite::{OptionalExtension, params};
use serde::Serialize;

use crate::{Library, Result, now};

/// Reading progress as a device last reported it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Reading {
    pub device_serial: String,
    /// "unread", "reading" or "finished".
    pub status: String,
    pub percent: u8,
    pub last_read: Option<String>,
    pub time_spent_secs: Option<u64>,
    /// When the app read it from the device.
    pub seen_at: String,
}

impl Library {
    /// Ids of books with a copy on any device.
    pub fn books_on_any_device(&self) -> Result<HashSet<i64>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT book_id FROM device_copies")?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }

    /// Remembers that the group `members` (a key from `erc-core`) in `scope`
    /// isn't a set of duplicates.
    pub fn dismiss_duplicates(&self, scope: &str, members: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO dismissed_duplicates (scope, members) VALUES (?1, ?2)",
            params![scope, members],
        )?;
        Ok(())
    }

    pub fn dismissed_duplicates(&self, scope: &str) -> Result<HashSet<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT members FROM dismissed_duplicates WHERE scope = ?1")?;
        let keys = stmt
            .query_map([scope], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(keys)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    /// Sets, or with `None` clears, a setting.
    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                params![key, v],
            )?,
            None => self
                .conn
                .execute("DELETE FROM settings WHERE key = ?1", [key])?,
        };
        Ok(())
    }

    /// Stores what a device says about a book's reading.
    pub fn record_reading(
        &self,
        book_id: i64,
        device_serial: &str,
        status: &str,
        percent: u8,
        last_read: Option<&str>,
        time_spent_secs: Option<u64>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO reading
                 (book_id, device_serial, status, percent, last_read, time_spent, seen_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                book_id,
                device_serial,
                status,
                percent,
                last_read,
                time_spent_secs.map(|t| t as i64),
                now()
            ],
        )?;
        Ok(())
    }

    /// The most recently read state of a book across devices.
    pub fn reading(&self, book_id: i64) -> Result<Option<Reading>> {
        Ok(self
            .conn
            .query_row(
                "SELECT device_serial, status, percent, last_read, time_spent, seen_at
                 FROM reading WHERE book_id = ?1
                 ORDER BY coalesce(last_read, '') DESC, seen_at DESC LIMIT 1",
                [book_id],
                |r| {
                    Ok(Reading {
                        device_serial: r.get(0)?,
                        status: r.get(1)?,
                        percent: r.get::<_, i64>(2)?.clamp(0, 100) as u8,
                        last_read: r.get(3)?,
                        time_spent_secs: r.get::<_, Option<i64>>(4)?.map(|t| t.max(0) as u64),
                        seen_at: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }
}
