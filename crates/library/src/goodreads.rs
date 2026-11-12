//! Books from Goodreads exports, kept so a newer export can update them.
//!
//! The library stores each book as opaque JSON under its Goodreads Book Id
//! (the parsing lives in `parallax-core`). A sync is one transaction: it
//! adds new books, rewrites ones that changed, leaves unchanged ones alone,
//! and only flags books a newer export no longer has. It never deletes, so a
//! partial or wrong export can't lose anything, and importing the same file
//! twice changes nothing.

use std::collections::HashMap;

use chrono::Utc;
use rusqlite::params;
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::{Library, Result};

/// What a sync did. Each book in the export is in exactly one of `added`,
/// `updated`, `unchanged` or `restored`; `missing` counts saved books the
/// export no longer has.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GoodreadsSync {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Were flagged missing by an earlier export and are back.
    pub restored: usize,
    /// Newly flagged missing: saved before, absent from this export.
    pub missing: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoodreadsRow {
    pub goodreads_id: String,
    pub data: String,
    pub missing: bool,
}

fn fingerprint(data: &str) -> String {
    Sha256::digest(data.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

impl Library {
    /// Applies an export: `(goodreads_id, json)` for every book in it. If an
    /// id appears twice the last one wins.
    pub fn goodreads_sync(&mut self, books: &[(String, String)]) -> Result<GoodreadsSync> {
        let latest: HashMap<&str, &str> = books
            .iter()
            .map(|(id, data)| (id.as_str(), data.as_str()))
            .collect();
        let now = Utc::now().to_rfc3339();
        let tx = self.conn.transaction()?;
        let mut sync = GoodreadsSync::default();
        let saved: HashMap<String, (String, bool)> = tx
            .prepare("SELECT goodreads_id, fingerprint, missing FROM goodreads_books")?
            .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?))))?
            .collect::<std::result::Result<_, _>>()?;
        for (&id, &data) in &latest {
            let print = fingerprint(data);
            match saved.get(id) {
                None => {
                    tx.execute(
                        "INSERT INTO goodreads_books
                         (goodreads_id, data, fingerprint, first_imported_at, updated_at)
                         VALUES (?1, ?2, ?3, ?4, ?4)",
                        params![id, data, print, now],
                    )?;
                    sync.added += 1;
                }
                Some((old, was_missing)) => {
                    let changed = *old != print;
                    if changed || *was_missing {
                        tx.execute(
                            "UPDATE goodreads_books SET data = ?2, fingerprint = ?3,
                             updated_at = CASE WHEN ?4 THEN ?5 ELSE updated_at END, missing = 0
                             WHERE goodreads_id = ?1",
                            params![id, data, print, changed, now],
                        )?;
                    }
                    match (*was_missing, changed) {
                        (true, _) => sync.restored += 1,
                        (false, true) => sync.updated += 1,
                        (false, false) => sync.unchanged += 1,
                    }
                }
            }
        }
        for (id, (_, was_missing)) in &saved {
            if !was_missing && !latest.contains_key(id.as_str()) {
                tx.execute(
                    "UPDATE goodreads_books SET missing = 1 WHERE goodreads_id = ?1",
                    [id],
                )?;
                sync.missing += 1;
            }
        }
        tx.commit()?;
        Ok(sync)
    }

    /// Every saved book, including those flagged missing, by id.
    pub fn goodreads_saved(&self) -> Result<Vec<GoodreadsRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT goodreads_id, data, missing FROM goodreads_books ORDER BY CAST(goodreads_id AS INTEGER), goodreads_id",
        )?;
        let rows = stmt
            .query_map([], |r| {
                Ok(GoodreadsRow {
                    goodreads_id: r.get(0)?,
                    data: r.get(1)?,
                    missing: r.get(2)?,
                })
            })?
            .collect::<std::result::Result<_, _>>()?;
        Ok(rows)
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

    fn rows(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(i, d)| ((*i).into(), (*d).into()))
            .collect()
    }

    #[test]
    fn importing_the_same_export_twice_changes_nothing() {
        let (_dir, mut lib) = lib();
        let export = rows(&[("1", "a"), ("2", "b")]);
        let first = lib.goodreads_sync(&export).unwrap();
        assert_eq!((first.added, first.unchanged), (2, 0));
        let before = lib.goodreads_saved().unwrap();
        let second = lib.goodreads_sync(&export).unwrap();
        assert_eq!(
            second,
            GoodreadsSync {
                unchanged: 2,
                ..GoodreadsSync::default()
            }
        );
        assert_eq!(lib.goodreads_saved().unwrap(), before);
    }

    #[test]
    fn a_newer_export_adds_updates_and_flags_without_duplicating() {
        let (_dir, mut lib) = lib();
        lib.goodreads_sync(&rows(&[("1", "a"), ("2", "b"), ("3", "c")]))
            .unwrap();
        let sync = lib
            .goodreads_sync(&rows(&[("1", "a"), ("2", "b2"), ("4", "d")]))
            .unwrap();
        assert_eq!(
            sync,
            GoodreadsSync {
                added: 1,
                updated: 1,
                unchanged: 1,
                restored: 0,
                missing: 1
            }
        );
        let saved = lib.goodreads_saved().unwrap();
        let state: Vec<_> = saved
            .iter()
            .map(|r| (r.goodreads_id.as_str(), r.data.as_str(), r.missing))
            .collect();
        assert_eq!(
            state,
            [
                ("1", "a", false),
                ("2", "b2", false),
                ("3", "c", true),
                ("4", "d", false)
            ]
        );
    }

    #[test]
    fn a_flagged_book_comes_back_when_it_returns_and_isnt_flagged_twice() {
        let (_dir, mut lib) = lib();
        lib.goodreads_sync(&rows(&[("1", "a"), ("2", "b")]))
            .unwrap();
        lib.goodreads_sync(&rows(&[("1", "a")])).unwrap();
        let again = lib.goodreads_sync(&rows(&[("1", "a")])).unwrap();
        assert_eq!(again.missing, 0, "already flagged");
        let back = lib
            .goodreads_sync(&rows(&[("1", "a"), ("2", "b")]))
            .unwrap();
        assert_eq!((back.restored, back.unchanged, back.missing), (1, 1, 0));
        assert!(lib.goodreads_saved().unwrap().iter().all(|r| !r.missing));
    }

    #[test]
    fn duplicate_ids_in_one_export_are_one_book() {
        let (_dir, mut lib) = lib();
        let sync = lib
            .goodreads_sync(&rows(&[("1", "old"), ("1", "new")]))
            .unwrap();
        assert_eq!(sync.added, 1);
        assert_eq!(lib.goodreads_saved().unwrap()[0].data, "new");
    }

    #[test]
    fn first_import_time_survives_updates() {
        let (_dir, mut lib) = lib();
        lib.goodreads_sync(&rows(&[("1", "a")])).unwrap();
        let first: String = lib
            .conn
            .query_row("SELECT first_imported_at FROM goodreads_books", [], |r| {
                r.get(0)
            })
            .unwrap();
        lib.goodreads_sync(&rows(&[("1", "b")])).unwrap();
        let (kept, data): (String, String) = lib
            .conn
            .query_row(
                "SELECT first_imported_at, data FROM goodreads_books",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!((kept, data.as_str()), (first, "b"));
    }
}
