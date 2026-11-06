//! Small stores used by `erc-core` workflows: which books are on a device,
//! duplicate groups the user dismissed, app settings, reading progress,
//! reading statistics and annotations seen on devices, and calibre imports.

use std::collections::{HashMap, HashSet};

use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::{Library, Result, now};

/// A highlight, note or bookmark copied from a device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    /// The device's id for it (Kobo `BookmarkID`).
    pub id: String,
    pub device_serial: String,
    /// "highlight", "note", "bookmark" or "markup".
    pub kind: String,
    pub text: Option<String>,
    pub note: Option<String>,
    pub chapter: Option<String>,
    /// Reading order within the book.
    pub position: f64,
    pub created: Option<String>,
    pub modified: Option<String>,
    /// False once it's been deleted on the device.
    pub on_device: bool,
}

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
                time_spent_secs,
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
                        percent: u8::try_from(r.get::<_, i64>(2)?.clamp(0, 100)).unwrap_or(0),
                        last_read: r.get(3)?,
                        time_spent_secs: r
                            .get::<_, Option<i64>>(4)?
                            .map(|t| u64::try_from(t).unwrap_or(0)),
                        seen_at: r.get(5)?,
                    })
                },
            )
            .optional()?)
    }

    /// Replaces what `device_serial` says about `book_id`'s annotations:
    /// upserts `items` and marks the book's others there as removed from
    /// the device (they're kept). Returns how many were new or changed.
    pub fn sync_annotations(
        &mut self,
        book_id: i64,
        device_serial: &str,
        items: &[Annotation],
    ) -> Result<usize> {
        let tx = self.conn.transaction()?;
        let mut changed = 0;
        for a in items {
            let current: Option<(Option<String>, Option<String>, bool)> = tx
                .query_row(
                    "SELECT text, note, on_device FROM annotations
                     WHERE device_serial = ?1 AND bookmark_id = ?2",
                    params![device_serial, a.id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            if current != Some((a.text.clone(), a.note.clone(), true)) {
                changed += 1;
            }
            tx.execute(
                "INSERT OR REPLACE INTO annotations
                     (device_serial, bookmark_id, book_id, kind, text, note, chapter, position,
                      created, modified, on_device)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, 1)",
                params![
                    device_serial,
                    a.id,
                    book_id,
                    a.kind,
                    a.text,
                    a.note,
                    a.chapter,
                    a.position,
                    a.created,
                    a.modified
                ],
            )?;
        }
        let seen: HashSet<&str> = items.iter().map(|a| a.id.as_str()).collect();
        let existing: Vec<String> = {
            let mut stmt = tx.prepare(
                "SELECT bookmark_id FROM annotations
                 WHERE book_id = ?1 AND device_serial = ?2 AND on_device = 1",
            )?;
            stmt.query_map(params![book_id, device_serial], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in existing.iter().filter(|id| !seen.contains(id.as_str())) {
            changed += tx.execute(
                "UPDATE annotations SET on_device = 0 WHERE device_serial = ?1 AND bookmark_id = ?2",
                params![device_serial, id],
            )?;
        }
        tx.commit()?;
        Ok(changed)
    }

    /// A book's annotations from every device, in reading order.
    pub fn annotations(&self, book_id: i64) -> Result<Vec<Annotation>> {
        let mut stmt = self.conn.prepare(
            "SELECT bookmark_id, device_serial, kind, text, note, chapter, position, created,
                    modified, on_device
             FROM annotations WHERE book_id = ?1
             ORDER BY position, coalesce(created, '')",
        )?;
        let rows = stmt
            .query_map([book_id], |r| {
                Ok(Annotation {
                    id: r.get(0)?,
                    device_serial: r.get(1)?,
                    kind: r.get(2)?,
                    text: r.get(3)?,
                    note: r.get(4)?,
                    chapter: r.get(5)?,
                    position: r.get(6)?,
                    created: r.get(7)?,
                    modified: r.get(8)?,
                    on_device: r.get(9)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// A book's highlights and notes (bookmarks not counted).
    pub fn annotation_count(&self, book_id: i64) -> Result<usize> {
        Ok(self.conn.query_row(
            "SELECT count(*) FROM annotations WHERE book_id = ?1 AND kind IN ('highlight', 'note')",
            [book_id],
            |r| r.get(0),
        )?)
    }

    /// Highlights and notes per book (bookmarks not counted).
    pub fn annotation_counts(&self) -> Result<HashMap<i64, usize>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, count(*) FROM annotations
             WHERE kind IN ('highlight', 'note') GROUP BY book_id",
        )?;
        let counts = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(counts)
    }

    /// The library book imported from the calibre book with this uuid.
    pub fn calibre_link(&self, calibre_uuid: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT book_id FROM calibre_links WHERE calibre_uuid = ?1",
                [calibre_uuid],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn record_calibre_link(&self, calibre_uuid: &str, book_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO calibre_links (calibre_uuid, book_id) VALUES (?1, ?2)",
            params![calibre_uuid, book_id],
        )?;
        Ok(())
    }

    /// Stores the latest reading statistics for a device (any JSON value).
    pub fn save_reading_stats(&self, device_serial: &str, stats: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO reading_stats (device_serial, taken_at, stats)
             VALUES (?1, ?2, ?3)",
            params![device_serial, now(), stats],
        )?;
        Ok(())
    }

    /// The most recently saved reading statistics: (device serial, when, JSON).
    pub fn latest_reading_stats(&self) -> Result<Option<(String, String, String)>> {
        Ok(self
            .conn
            .query_row(
                "SELECT device_serial, taken_at, stats FROM reading_stats
                 ORDER BY taken_at DESC LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?)
    }
}
