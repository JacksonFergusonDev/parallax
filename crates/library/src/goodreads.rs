//! Books from Goodreads exports, kept so a newer export can update them.
//!
//! The library stores each book as opaque JSON under its Goodreads Book Id;
//! `parallax-core` parses exports and decides how a newer one merges with the
//! saved books. Each saved book has two copies: `data`, the book as you see
//! it, and `base`, the book as the last export had it. Comparing the two with
//! a newer export tells which side changed a field. Changes are applied in one
//! transaction, so an import either happens completely or not at all.

use chrono::{DateTime, NaiveDate, Utc};
use rusqlite::{OptionalExtension, Transaction, params};
use serde::Serialize;

use crate::{Library, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodreadsRow {
    pub goodreads_id: String,
    pub data: String,
    pub base: String,
    /// You kept it after an export no longer had it.
    pub local_only: bool,
}

/// One change to the saved books.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoodreadsWrite {
    /// A book new to the library.
    Add {
        id: String,
        data: String,
    },
    /// Both copies at once, from merging an export.
    Merge {
        id: String,
        data: String,
        base: String,
    },
    /// An edit made here; the export's copy stays as it was.
    Edit {
        id: String,
        data: String,
    },
    /// Keep a book an export no longer has.
    KeepLocal {
        id: String,
    },
    Remove {
        id: String,
    },
}

/// An export you imported, as recorded when you imported it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoodreadsImport {
    pub imported_at: DateTime<Utc>,
    pub file_name: Option<String>,
    /// When the export file was last changed: usually when you downloaded it.
    pub file_modified: Option<DateTime<Utc>>,
    /// The latest date added or read in the export, the nearest thing to its
    /// own date that Goodreads gives.
    pub latest_activity: Option<NaiveDate>,
    pub books: usize,
    pub added: usize,
    pub updated: usize,
}

/// What to record about an import; the library adds the time.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GoodreadsImportRecord {
    pub file_name: Option<String>,
    pub file_modified: Option<DateTime<Utc>>,
    pub latest_activity: Option<NaiveDate>,
    pub books: usize,
    pub added: usize,
    pub updated: usize,
}

impl Library {
    /// Every saved book, by id.
    pub fn goodreads_saved(&self) -> Result<Vec<GoodreadsRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT goodreads_id, data, base, local_only FROM goodreads_books
             ORDER BY CAST(goodreads_id AS INTEGER), goodreads_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(GoodreadsRow {
                    goodreads_id: r.get(0)?,
                    data: r.get(1)?,
                    base: r.get(2)?,
                    local_only: r.get(3)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
    }

    /// Applies the changes together or not at all.
    pub fn goodreads_write(&mut self, writes: &[GoodreadsWrite]) -> Result<()> {
        let tx = self.conn.transaction()?;
        apply(&tx, writes)?;
        tx.commit()?;
        Ok(())
    }

    /// Applies an export's changes and records the import, together or not
    /// at all.
    pub fn goodreads_import(
        &mut self,
        writes: &[GoodreadsWrite],
        record: &GoodreadsImportRecord,
    ) -> Result<()> {
        let tx = self.conn.transaction()?;
        apply(&tx, writes)?;
        tx.execute(
            "INSERT INTO goodreads_imports
             (imported_at, file_name, file_modified, latest_activity, books, added, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                Utc::now().to_rfc3339(),
                record.file_name,
                record.file_modified.map(|t| t.to_rfc3339()),
                record.latest_activity.map(|d| d.to_string()),
                record.books,
                record.added,
                record.updated,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// The most recent import and how many there have been.
    pub fn goodreads_last_import(&self) -> Result<Option<(GoodreadsImport, usize)>> {
        let count: usize =
            self.conn
                .query_row("SELECT COUNT(*) FROM goodreads_imports", [], |r| r.get(0))?;
        let last = self
            .conn
            .query_row(
                "SELECT imported_at, file_name, file_modified, latest_activity, books, added, updated
                 FROM goodreads_imports ORDER BY id DESC LIMIT 1",
                [],
                |r| {
                    let time = |s: Option<String>| {
                        s.and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                            .map(|t| t.with_timezone(&Utc))
                    };
                    Ok(GoodreadsImport {
                        imported_at: time(r.get(0)?).unwrap_or_default(),
                        file_name: r.get(1)?,
                        file_modified: time(r.get(2)?),
                        latest_activity: r
                            .get::<_, Option<String>>(3)?
                            .and_then(|d| d.parse().ok()),
                        books: r.get(4)?,
                        added: r.get(5)?,
                        updated: r.get(6)?,
                    })
                },
            )
            .optional()?;
        Ok(last.map(|l| (l, count)))
    }
}

fn apply(tx: &Transaction<'_>, writes: &[GoodreadsWrite]) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    for write in writes {
        match write {
            GoodreadsWrite::Add { id, data } => {
                tx.execute(
                    "INSERT INTO goodreads_books
                         (goodreads_id, data, base, first_imported_at, updated_at)
                         VALUES (?1, ?2, ?2, ?3, ?3)",
                    params![id, data, now],
                )?;
            }
            GoodreadsWrite::Merge { id, data, base } => {
                tx.execute(
                    "UPDATE goodreads_books
                         SET data = ?2, base = ?3, local_only = 0, updated_at = ?4
                         WHERE goodreads_id = ?1",
                    params![id, data, base, now],
                )?;
            }
            GoodreadsWrite::Edit { id, data } => {
                tx.execute(
                    "UPDATE goodreads_books SET data = ?2, updated_at = ?3
                         WHERE goodreads_id = ?1",
                    params![id, data, now],
                )?;
            }
            GoodreadsWrite::KeepLocal { id } => {
                tx.execute(
                    "UPDATE goodreads_books SET local_only = 1 WHERE goodreads_id = ?1",
                    [id],
                )?;
            }
            GoodreadsWrite::Remove { id } => {
                tx.execute("DELETE FROM goodreads_books WHERE goodreads_id = ?1", [id])?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let lib = Library::open(dir.path()).unwrap();
        (dir, lib)
    }

    fn add(id: &str, data: &str) -> GoodreadsWrite {
        GoodreadsWrite::Add {
            id: id.into(),
            data: data.into(),
        }
    }

    #[test]
    fn a_new_book_starts_with_data_and_base_the_same() {
        let (_dir, mut lib) = lib();
        lib.goodreads_write(&[add("1", "a")]).unwrap();
        let [row] = lib.goodreads_saved().unwrap().try_into().unwrap();
        assert_eq!(
            (row.data.as_str(), row.base.as_str(), row.local_only),
            ("a", "a", false)
        );
    }

    #[test]
    fn an_edit_changes_only_what_you_see_and_a_merge_changes_both() {
        let (_dir, mut lib) = lib();
        lib.goodreads_write(&[add("1", "a")]).unwrap();
        lib.goodreads_write(&[GoodreadsWrite::Edit {
            id: "1".into(),
            data: "mine".into(),
        }])
        .unwrap();
        let row = &lib.goodreads_saved().unwrap()[0];
        assert_eq!((row.data.as_str(), row.base.as_str()), ("mine", "a"));
        lib.goodreads_write(&[GoodreadsWrite::Merge {
            id: "1".into(),
            data: "m".into(),
            base: "t".into(),
        }])
        .unwrap();
        let row = &lib.goodreads_saved().unwrap()[0];
        assert_eq!((row.data.as_str(), row.base.as_str()), ("m", "t"));
    }

    #[test]
    fn keeping_and_removing() {
        let (_dir, mut lib) = lib();
        lib.goodreads_write(&[add("1", "a"), add("2", "b")])
            .unwrap();
        lib.goodreads_write(&[
            GoodreadsWrite::KeepLocal { id: "1".into() },
            GoodreadsWrite::Remove { id: "2".into() },
        ])
        .unwrap();
        let saved = lib.goodreads_saved().unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].local_only);
        lib.goodreads_write(&[GoodreadsWrite::Merge {
            id: "1".into(),
            data: "a".into(),
            base: "a".into(),
        }])
        .unwrap();
        assert!(
            !lib.goodreads_saved().unwrap()[0].local_only,
            "back in an export"
        );
    }

    #[test]
    fn an_import_is_recorded_with_its_changes() {
        let (_dir, mut lib) = lib();
        assert_eq!(lib.goodreads_last_import().unwrap(), None);
        let record = GoodreadsImportRecord {
            file_name: Some("goodreads_library_export.csv".into()),
            file_modified: Some("2026-11-03T10:00:00Z".parse().unwrap()),
            latest_activity: NaiveDate::from_ymd_opt(2026, 11, 2),
            books: 1,
            added: 1,
            updated: 0,
        };
        lib.goodreads_import(&[add("1", "a")], &record).unwrap();
        lib.goodreads_import(
            &[],
            &GoodreadsImportRecord {
                books: 1,
                ..record.clone()
            },
        )
        .unwrap();
        let (last, count) = lib.goodreads_last_import().unwrap().unwrap();
        assert_eq!(count, 2);
        assert_eq!(
            (
                last.file_name,
                last.file_modified,
                last.latest_activity,
                last.books
            ),
            (
                record.file_name,
                record.file_modified,
                record.latest_activity,
                1
            )
        );
        assert_eq!(lib.goodreads_saved().unwrap().len(), 1);
    }

    #[test]
    fn books_saved_before_import_records_get_one() {
        let (dir, mut lib) = lib();
        let book =
            |added: &str, read: &str| format!(r#"{{"date_added":"{added}","date_read":{read}}}"#);
        lib.goodreads_write(&[
            add("1", &book("2024-04-02", "null")),
            add("2", &book("2023-01-05", r#""2025-06-30""#)),
        ])
        .unwrap();
        lib.conn
            .execute_batch("DROP TABLE goodreads_imports; PRAGMA user_version = 17;")
            .unwrap();
        drop(lib);
        let lib = Library::open(dir.path()).unwrap();
        let (last, count) = lib.goodreads_last_import().unwrap().unwrap();
        assert_eq!(count, 1);
        assert_eq!(
            (last.books, last.file_name, last.latest_activity),
            (2, None, NaiveDate::from_ymd_opt(2025, 6, 30))
        );
    }

    #[test]
    fn a_failed_import_isnt_recorded() {
        let (_dir, mut lib) = lib();
        lib.goodreads_write(&[add("1", "a")]).unwrap();
        let failed = lib.goodreads_import(&[add("1", "again")], &GoodreadsImportRecord::default());
        assert!(failed.is_err());
        assert_eq!(lib.goodreads_last_import().unwrap(), None);
    }

    #[test]
    fn a_failing_change_undoes_the_whole_batch() {
        let (_dir, mut lib) = lib();
        lib.goodreads_write(&[add("1", "a")]).unwrap();
        let result = lib.goodreads_write(&[add("2", "b"), add("1", "again")]);
        assert!(result.is_err(), "duplicate id");
        assert_eq!(
            lib.goodreads_saved().unwrap().len(),
            1,
            "2 wasn't left behind"
        );
    }
}
