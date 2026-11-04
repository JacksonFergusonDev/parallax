//! Reading history Nickel keeps beyond each book's current state.
//!
//! The `Event` table counts things per book, and its rows outlive the book:
//! they stay after a book is removed. Kobo doesn't document the event types;
//! type 5 is recorded each time a book is finished (its count goes up on a
//! re-read and `LastOccurrence` is the latest finish), which is what ERC uses
//! for "books finished". The others (opens, page turns…) aren't used.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::db::{KoboDb, columns, parse_kobo_date};
use crate::error::Result;

/// Nickel's event type for finishing a book.
const FINISHED_EVENT: i64 = 5;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FinishedEvent {
    /// The ContentID it was recorded for; the book may be gone since.
    pub content_id: String,
    /// The latest time it was finished.
    pub at: Option<DateTime<Utc>>,
    /// How many times it was finished.
    pub times: u32,
}

impl KoboDb {
    /// Every book Nickel recorded as finished, including books removed since.
    /// Empty on schemas without the `Event` table.
    pub fn finished_events(&self) -> Result<Vec<FinishedEvent>> {
        let conn = self.connection();
        if columns(conn, "Event")?.is_empty() {
            return Ok(Vec::new());
        }
        let mut stmt = conn.prepare(
            "SELECT ContentID, LastOccurrence, EventCount FROM Event
             WHERE EventType = ?1 AND ContentID IS NOT NULL AND ContentID != ''",
        )?;
        let events = stmt
            .query_map([FINISHED_EVENT], |r| {
                Ok(FinishedEvent {
                    content_id: r.get(0)?,
                    at: parse_kobo_date(r.get(1)?),
                    times: r.get::<_, Option<i64>>(2)?.unwrap_or(1).max(1) as u32,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(events)
    }

    /// Nickel's estimate of reading time left, in seconds, by ContentID.
    pub fn time_left(&self) -> Result<HashMap<String, u64>> {
        let conn = self.connection();
        if !columns(conn, "content")?.contains("restofbookestimate") {
            return Ok(HashMap::new());
        }
        let mut stmt = conn.prepare(
            "SELECT ContentID, RestOfBookEstimate FROM content
             WHERE BookID IS NULL AND RestOfBookEstimate > 0",
        )?;
        let left = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(left)
    }

    /// Title and author of every book row, downloaded or not, by ContentID.
    pub fn titles(&self) -> Result<HashMap<String, (String, Option<String>)>> {
        let mut stmt = self.connection().prepare(
            "SELECT ContentID, Title, Attribution FROM content
             WHERE BookID IS NULL AND Title IS NOT NULL AND Title != ''",
        )?;
        let titles = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    (
                        r.get(1)?,
                        r.get::<_, Option<String>>(2)?
                            .filter(|a| !a.trim().is_empty()),
                    ),
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(titles)
    }
}
