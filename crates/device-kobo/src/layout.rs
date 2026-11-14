//! The font and layout Nickel opens a book with, kept per book in
//! `content_settings`. Adapted from Kobo Utilities' "Set reader font
//! settings" (janlarres/kobo-utilities, GPLv3, features/fonts.py), which
//! writes the same columns.

use std::collections::HashSet;

use chrono::Utc;
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};

use crate::db::{KoboDb, columns};
use crate::error::Result;
use crate::state::LogEntry;
use crate::writer::DeviceWriter;

/// Line spacings Nickel's slider offers.
pub const LINE_HEIGHTS: [&str; 15] = [
    "1.0", "1.05", "1.07", "1.1", "1.2", "1.35", "1.5", "1.7", "1.8", "2.0", "2.2", "2.4", "2.6",
    "2.8", "3.0",
];
/// The font sizes and margins Parallax will write, within Nickel's range.
pub const FONT_SIZES: std::ops::RangeInclusive<i64> = 10..=60;
pub const MARGINS: std::ops::RangeInclusive<i64> = 0..=16;

/// How text is aligned: as the book sets it, or as Nickel's left or
/// justified option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Alignment {
    Left,
    Justify,
}

/// A book's reading layout. `None` leaves that part as Nickel's default.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Layout {
    pub font_family: Option<String>,
    pub font_size: Option<i64>,
    pub line_height: Option<f64>,
    /// Left and right margins together, as Nickel's slider sets them.
    pub margins: Option<i64>,
    pub alignment: Option<Alignment>,
}

impl Layout {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    /// Column assignments for what this layout sets.
    fn assignments(&self) -> Vec<(&'static str, rusqlite::types::Value)> {
        let mut set: Vec<(&'static str, rusqlite::types::Value)> = Vec::new();
        if let Some(font) = &self.font_family {
            set.push(("ReadingFontFamily", font.clone().into()));
        }
        if let Some(size) = self.font_size {
            set.push((
                "ReadingFontSize",
                size.clamp(*FONT_SIZES.start(), *FONT_SIZES.end()).into(),
            ));
        }
        if let Some(height) = self.line_height {
            set.push(("ReadingLineHeight", height.into()));
        }
        if let Some(margin) = self.margins {
            let margin = margin.clamp(*MARGINS.start(), *MARGINS.end());
            set.push(("ReadingLeftMargin", margin.into()));
            set.push(("ReadingRightMargin", margin.into()));
        }
        if let Some(alignment) = self.alignment {
            let value = match alignment {
                Alignment::Left => "Left",
                Alignment::Justify => "Justify",
            };
            set.push(("ReadingAlignment", value.to_owned().into()));
        }
        set
    }
}

impl KoboDb {
    /// Books that have their own reading settings: opened and adjusted on
    /// the Kobo, or set by Parallax.
    pub fn books_with_layout(&self) -> Result<HashSet<String>> {
        let mut stmt = self.connection().prepare(
            "SELECT ContentID FROM content_settings WHERE ContentType = 6 AND (
                 ReadingFontFamily IS NOT NULL OR ReadingFontSize IS NOT NULL
                 OR ReadingLineHeight IS NOT NULL OR ReadingLeftMargin IS NOT NULL
                 OR ReadingAlignment IS NOT NULL)",
        )?;
        let ids = stmt
            .query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(ids)
    }
}

/// Sets `layout` on each book Nickel has imported. Returns how many books
/// changed; books not imported yet are skipped.
pub fn apply_layout(
    writer: &mut DeviceWriter,
    content_ids: &[String],
    layout: &Layout,
) -> Result<usize> {
    let assignments = layout.assignments();
    if assignments.is_empty() {
        return Ok(0);
    }
    let changed = writer.transaction(|tx| {
        let cols = columns(tx, "content_settings")?;
        let assignments: Vec<_> = assignments
            .iter()
            .filter(|(c, _)| cols.contains(&c.to_ascii_lowercase()))
            .collect();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let mut n = 0;
        for id in content_ids {
            let imported: Option<i64> = tx
                .query_row(
                    "SELECT 1 FROM content WHERE ContentID = ?1 AND BookID IS NULL",
                    [id],
                    |r| r.get(0),
                )
                .optional()?;
            if imported.is_none() {
                continue;
            }
            tx.execute(
                "INSERT OR IGNORE INTO content_settings (ContentID, ContentType, DateModified)
                 VALUES (?1, 6, ?2)",
                params![id, now],
            )?;
            for (column, value) in &assignments {
                tx.execute(
                    &format!(
                        "UPDATE content_settings SET {column} = ?2, DateModified = ?3
                         WHERE ContentID = ?1 AND ContentType = 6"
                    ),
                    params![id, value, now],
                )?;
            }
            n += 1;
        }
        Ok(n)
    })?;
    if changed > 0 {
        writer.log(
            LogEntry::new("reading layout")
                .ids(content_ids.iter().cloned())
                .detail("set font and layout"),
        );
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_layout_sets_nothing() {
        assert!(Layout::default().assignments().is_empty());
    }

    #[test]
    fn margins_set_both_sides_within_range() {
        let layout = Layout {
            margins: Some(40),
            font_size: Some(4),
            alignment: Some(Alignment::Justify),
            ..Layout::default()
        };
        let set = layout.assignments();
        let get = |c: &str| set.iter().find(|(k, _)| *k == c).map(|(_, v)| v.clone());
        assert_eq!(get("ReadingLeftMargin"), Some(16_i64.into()));
        assert_eq!(get("ReadingRightMargin"), Some(16_i64.into()));
        assert_eq!(get("ReadingFontSize"), Some(10_i64.into()));
        assert_eq!(get("ReadingAlignment"), Some("Justify".to_owned().into()));
        assert_eq!(get("ReadingFontFamily"), None);
    }
}
