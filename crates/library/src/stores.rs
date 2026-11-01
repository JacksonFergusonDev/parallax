//! Small stores used by `erc-core` workflows: which books are on a device,
//! duplicate groups the user dismissed, and app settings.

use std::collections::HashSet;

use rusqlite::{OptionalExtension, params};

use crate::{Library, Result};

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
}
