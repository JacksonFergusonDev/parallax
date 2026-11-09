//! Going back to how the library was before a big change.
//!
//! A snapshot is a copy of `library.sqlite` taken before a removal or bulk
//! change, in `snapshots/`. Books' files live outside the database, so removed
//! books' folders are held in `removed/` rather than deleted. Going back
//! matches the folders in `books/` to the snapshot's rows: books the snapshot
//! has are brought back from `removed/`, and books it doesn't have (added
//! since) are moved there. Settings stay as they are now; they aren't part of
//! what a change did.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{Duration, SecondsFormat, Utc};
use rusqlite::backup::{Backup, Progress};
use rusqlite::{Connection, MAIN_DB, params};
use serde::{Deserialize, Serialize};

use crate::{Error, Library, Result, book_dir, io_err, schema};

/// Snapshots kept; older ones are deleted.
pub const SNAPSHOTS_KEPT: usize = 30;
/// How long removed books' files are held.
pub const REMOVED_KEPT_DAYS: i64 = 30;

const SNAPSHOT_DIR: &str = "snapshots";
const REMOVED_DIR: &str = "removed";
const REMOVED_NOTE: &str = "removed.json";
const PREFIX: &str = "library-";
const EXTENSION: &str = ".sqlite";
const STAMP: &str = "%Y%m%d-%H%M%S%.3f";

/// A point the library can go back to, newest first in [`Library::snapshots`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub file: String,
    pub at: String,
    /// The change this was taken before: `remove`, `tidy`, `enhance`,
    /// `calibre`, `kobo-import` or `restore`.
    pub op: String,
    /// What it changed, in words: a title, or "3 books".
    pub detail: Option<String>,
    /// Books in the library at the time.
    pub books: i64,
    /// Why this snapshot can't be used; `None` when it can.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SnapshotRestore {
    /// The snapshot of how the library was just before going back.
    pub safety_snapshot: String,
    /// Books brought back from `removed/`.
    pub restored: usize,
    /// Books added since the snapshot, moved to `removed/`.
    pub set_aside: usize,
    /// Books in the snapshot whose files are gone for good.
    pub lost: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RemovedNote {
    hash: String,
    title: String,
}

impl Library {
    fn snapshot_dir(&self) -> PathBuf {
        self.root.join(SNAPSHOT_DIR)
    }

    /// Saves the database before a change. `op` and `detail` describe the
    /// change; see [`Snapshot`].
    pub fn snapshot(&self, op: &str, detail: Option<&str>) -> Result<PathBuf> {
        let dir = self.snapshot_dir();
        fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
        let path = loop {
            let name = format!("{PREFIX}{}{EXTENSION}", Utc::now().format(STAMP));
            let path = dir.join(name);
            if !path.exists() {
                break path;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        {
            let mut dest = Connection::open(&path)?;
            let backup = Backup::new(&self.conn, &mut dest)?;
            backup.run_to_completion(256, std::time::Duration::ZERO, None)?;
        }
        let note = Snapshot {
            file: String::new(),
            at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
            op: op.to_owned(),
            detail: detail.map(str::to_owned),
            books: self
                .conn
                .query_row("SELECT count(*) FROM books", [], |r| r.get(0))?,
            blocked: None,
        };
        let sidecar = path.with_extension("json");
        let json = serde_json::to_vec_pretty(&note).map_err(|e| io_err(&sidecar, e.into()))?;
        fs::write(&sidecar, json).map_err(|e| io_err(&sidecar, e))?;
        self.prune_snapshots();
        Ok(path)
    }

    /// Snapshots, newest first.
    pub fn snapshots(&self) -> Vec<Snapshot> {
        let dir = self.snapshot_dir();
        let mut names = snapshot_names(&dir);
        names.reverse();
        names
            .into_iter()
            .filter_map(|file| {
                let path = dir.join(&file);
                let bytes = fs::read(path.with_extension("json")).ok()?;
                let mut note: Snapshot = serde_json::from_slice(&bytes).ok()?;
                note.blocked = restore_blocker(&path);
                note.file = file;
                Some(note)
            })
            .collect()
    }

    /// Takes the library back to `file`, a name from [`Library::snapshots`].
    /// The library as it is now is saved as a snapshot first, so this can be
    /// undone too.
    pub fn restore_snapshot(&mut self, file: &str) -> Result<SnapshotRestore> {
        let path = self.snapshot_dir().join(file);
        if !is_snapshot(file) || !path.is_file() {
            return Err(Error::CannotRestore("there's no such snapshot".to_owned()));
        }
        if let Some(why) = restore_blocker(&path) {
            return Err(Error::CannotRestore(why));
        }

        let before = self.book_notes()?;
        let settings = self.all_settings()?;
        let safety = self.snapshot("restore", Some(file))?;

        self.conn.restore(MAIN_DB, &path, None::<fn(Progress)>)?;
        schema::migrate(&self.conn)?;
        self.conn.execute("DELETE FROM settings", [])?;
        for (key, value) in settings {
            self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                params![key, value],
            )?;
        }

        let mut outcome = self.match_files_to_rows(&before)?;
        outcome.safety_snapshot = safety
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        Ok(outcome)
    }

    /// Makes `books/` agree with the database after going back.
    fn match_files_to_rows(&self, before: &HashMap<i64, RemovedNote>) -> Result<SnapshotRestore> {
        let mut outcome = SnapshotRestore::default();
        let rows: HashMap<i64, (String, String, u64)> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id, file_hash, title, file_size FROM books")?;
            stmt.query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))?
                .collect::<rusqlite::Result<_>>()?
        };

        // Book ids get reused, so a folder only belongs to a row of its size.
        let books = self.root.join("books");
        for entry in fs::read_dir(&books)
            .map_err(|e| io_err(&books, e))?
            .flatten()
        {
            let Some(id) = entry.file_name().to_str().and_then(|n| n.parse().ok()) else {
                continue;
            };
            let size = fs::metadata(crate::format::stored_file(&entry.path())).map(|m| m.len());
            if rows
                .get(&id)
                .is_some_and(|r| size.as_ref().ok() == Some(&r.2))
            {
                continue;
            }
            let (hash, title) = before.get(&id).map_or((String::new(), String::new()), |n| {
                (n.hash.clone(), n.title.clone())
            });
            self.hold_book_dir(id, &hash, &title)?;
            if !rows.contains_key(&id) {
                outcome.set_aside += 1;
            }
        }

        let held = self.held_books();
        for (id, (hash, title, _)) in &rows {
            let dir = book_dir(&self.root, *id);
            if crate::format::has_stored_file(&dir) {
                continue;
            }
            if let Some(from) = held.get(hash) {
                fs::rename(from, &dir).map_err(|e| io_err(&dir, e))?;
                let _ = fs::remove_file(dir.join(REMOVED_NOTE));
                outcome.restored += 1;
            } else {
                self.conn.execute("DELETE FROM books WHERE id = ?1", [id])?;
                outcome.lost.push(title.clone());
            }
        }

        // Custom covers aren't versioned: a row can't claim one that's gone.
        let custom: Vec<i64> = {
            let mut stmt = self
                .conn
                .prepare("SELECT id FROM books WHERE custom_cover = 1")?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<rusqlite::Result<_>>()?
        };
        for id in custom {
            if !self.custom_cover_file(id).is_file() {
                self.conn
                    .execute("UPDATE books SET custom_cover = 0 WHERE id = ?1", [id])?;
            }
        }
        outcome.lost.sort();
        Ok(outcome)
    }

    /// Moves a book's folder to `removed/`. A book with no folder has nothing
    /// to hold.
    pub(crate) fn hold_book_dir(&self, id: i64, hash: &str, title: &str) -> Result<()> {
        let dir = book_dir(&self.root, id);
        if !dir.exists() {
            return Ok(());
        }
        let removed = self.root.join(REMOVED_DIR);
        fs::create_dir_all(&removed).map_err(|e| io_err(&removed, e))?;
        let dest = loop {
            let dest = removed.join(format!("{}-{id}", Utc::now().format(STAMP)));
            if !dest.exists() {
                break dest;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        };
        fs::rename(&dir, &dest).map_err(|e| io_err(&dest, e))?;
        let note = RemovedNote {
            hash: hash.to_owned(),
            title: title.to_owned(),
        };
        let path = dest.join(REMOVED_NOTE);
        let json = serde_json::to_vec_pretty(&note).map_err(|e| io_err(&path, e.into()))?;
        fs::write(&path, json).map_err(|e| io_err(&path, e))
    }

    /// Held folders by file hash, the newest where a book was removed twice.
    fn held_books(&self) -> HashMap<String, PathBuf> {
        let mut dirs: Vec<PathBuf> = fs::read_dir(self.root.join(REMOVED_DIR))
            .map(|d| d.flatten().map(|e| e.path()).collect())
            .unwrap_or_default();
        dirs.sort();
        dirs.into_iter()
            .filter_map(|dir| {
                let bytes = fs::read(dir.join(REMOVED_NOTE)).ok()?;
                let note: RemovedNote = serde_json::from_slice(&bytes).ok()?;
                Some((note.hash, dir))
            })
            .collect()
    }

    fn book_notes(&self) -> Result<HashMap<i64, RemovedNote>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, file_hash, title FROM books")?;
        let notes = stmt
            .query_map([], |r| {
                Ok((
                    r.get(0)?,
                    RemovedNote {
                        hash: r.get(1)?,
                        title: r.get(2)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(notes)
    }

    fn all_settings(&self) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM settings")?;
        let settings = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<_>>()?;
        Ok(settings)
    }

    /// Best effort: failing to tidy old copies never fails a change.
    fn prune_snapshots(&self) {
        let dir = self.snapshot_dir();
        let names = snapshot_names(&dir);
        let excess = names.len().saturating_sub(SNAPSHOTS_KEPT);
        for name in &names[..excess] {
            let path = dir.join(name);
            let _ = fs::remove_file(&path);
            let _ = fs::remove_file(path.with_extension("json"));
        }
        let cutoff = (Utc::now() - Duration::days(REMOVED_KEPT_DAYS))
            .format(STAMP)
            .to_string();
        let Ok(held) = fs::read_dir(self.root.join(REMOVED_DIR)) else {
            return;
        };
        for entry in held.flatten() {
            if entry.file_name().to_string_lossy().as_ref() < cutoff.as_str() {
                let _ = fs::remove_dir_all(entry.path());
            }
        }
    }
}

fn is_snapshot(name: &str) -> bool {
    name.starts_with(PREFIX) && name.ends_with(EXTENSION) && !name.contains(['/', '\\'])
}

/// Snapshot file names, oldest first.
fn snapshot_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_snapshot(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn restore_blocker(path: &Path) -> Option<String> {
    let check = || -> rusqlite::Result<(String, usize)> {
        let conn = Connection::open(path)?;
        let check = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        let version = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        Ok((check, version))
    };
    match check() {
        Ok((check, _)) if check != "ok" => Some("this snapshot is damaged".to_owned()),
        Ok((_, version)) if version > schema::latest() => {
            Some("this snapshot is from a newer version of the app".to_owned())
        }
        Ok(_) => None,
        Err(_) => Some("this snapshot can't be read".to_owned()),
    }
}
