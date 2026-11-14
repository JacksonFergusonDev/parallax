//! Where each book is open, saved off the Kobo and put back after the book
//! is sent again. Adapted from Kobo Utilities' "Store/restore current
//! bookmark" (janlarres/kobo-utilities, GPLv3, features/bookmarks.py), which
//! keeps the same `content` columns.

use std::collections::HashMap;

use rusqlite::{OptionalExtension, params};

use crate::db::{KoboDb, columns};
use crate::error::Result;
use crate::state::LogEntry;
use crate::writer::DeviceWriter;

/// A book's place as Nickel keeps it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    /// `ChapterIDBookmarked`: the chapter file and spot within it, relative
    /// to the book, so it still holds when the file is renamed.
    pub chapter: Option<String>,
    /// 0 unread, 1 reading, 2 finished.
    pub status: i64,
    pub percent: u8,
    /// `DateLastRead` as Nickel writes it.
    pub last_read: Option<String>,
    pub time_spent_secs: Option<u64>,
}

impl KoboDb {
    /// `ChapterIDBookmarked` of every book that has one, by ContentID.
    pub fn bookmarked_chapters(&self) -> Result<HashMap<String, String>> {
        if !columns(self.connection(), "content")?.contains("chapteridbookmarked") {
            return Ok(HashMap::new());
        }
        let mut stmt = self.connection().prepare(
            "SELECT ContentID, ChapterIDBookmarked FROM content
             WHERE BookID IS NULL AND coalesce(ChapterIDBookmarked, '') <> ''",
        )?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }
}

/// Puts saved places back into books Nickel shows as never opened. A book
/// that has been opened since, or marked read, keeps what it has. Returns
/// the ContentIDs that were restored.
pub fn restore_positions(
    writer: &mut DeviceWriter,
    positions: &[(String, Position)],
) -> Result<Vec<String>> {
    let restored = writer.transaction(|tx| {
        let cols = columns(tx, "content")?;
        let has = |c: &str| cols.contains(&c.to_ascii_lowercase());
        let mut done = Vec::new();
        for (content_id, position) in positions {
            let untouched: Option<bool> = tx
                .query_row(
                    &format!(
                        "SELECT ReadStatus = 0 AND coalesce(___PercentRead, 0) = 0 AND {chapter}
                         FROM content WHERE ContentID = ?1 AND BookID IS NULL",
                        chapter = if has("ChapterIDBookmarked") {
                            "coalesce(ChapterIDBookmarked, '') = ''"
                        } else {
                            "1"
                        }
                    ),
                    [content_id],
                    |r| r.get(0),
                )
                .optional()?;
            if untouched != Some(true) {
                continue;
            }
            tx.execute(
                "UPDATE content SET ReadStatus = ?2, ___PercentRead = ?3
                 WHERE ContentID = ?1 AND BookID IS NULL",
                params![content_id, position.status, position.percent],
            )?;
            let optional: [(&str, rusqlite::types::Value); 4] = [
                ("ChapterIDBookmarked", position.chapter.clone().into()),
                ("DateLastRead", position.last_read.clone().into()),
                (
                    "TimeSpentReading",
                    position
                        .time_spent_secs
                        .and_then(|s| i64::try_from(s).ok())
                        .into(),
                ),
                (
                    "FirstTimeReading",
                    if position.status == 0 { "true" } else { "false" }
                        .to_owned()
                        .into(),
                ),
            ];
            for (column, value) in optional {
                if has(column) {
                    tx.execute(
                        &format!(
                            "UPDATE content SET {column} = ?2 WHERE ContentID = ?1 AND BookID IS NULL"
                        ),
                        params![content_id, value],
                    )?;
                }
            }
            done.push(content_id.clone());
        }
        Ok(done)
    })?;
    if !restored.is_empty() {
        writer.log(
            LogEntry::new("restore positions")
                .ids(restored.iter().cloned())
                .detail("put back where books were open"),
        );
    }
    Ok(restored)
}
