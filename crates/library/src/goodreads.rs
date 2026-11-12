//! Books from Goodreads exports, kept so a newer export can update them.
//!
//! The library stores each book as opaque JSON under its Goodreads Book Id;
//! `parallax-core` parses exports and decides how a newer one merges with the
//! saved books. Each saved book has two copies: `data`, the book as you see
//! it, and `base`, the book as the last export had it. Comparing the two with
//! a newer export tells which side changed a field. Changes are applied in one
//! transaction, so an import either happens completely or not at all.

use chrono::Utc;
use rusqlite::params;

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
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction()?;
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
        tx.commit()?;
        Ok(())
    }
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
