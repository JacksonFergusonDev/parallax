//! Database backups on this computer, and going back to one.
//!
//! Under `<backup root>/<serial>/`:
//! ```text
//! KoboReader-<stamp>.sqlite   the database just before a change
//! KoboReader-<stamp>.json     what that change was
//! removed/<stamp>/            book files that change removed, with manifest.json
//! ```
//!
//! A database backup can't bring back a deleted file, so removals copy the
//! file here first. Going back to a backup puts back the files removed by it
//! and every later change, then the database, after saving the current state
//! as one more backup so that going back can itself be undone.

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use chrono::{Duration, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};

use crate::KoboDevice;
use crate::db::open_immutable;
use crate::error::{Error, Result};
use crate::state::{BookChange, LogEntry, Record};
use crate::writer::restore;

/// The newest backups always kept per device.
pub const BACKUPS_KEPT: usize = 10;
/// Also kept: the earliest backup of each of this many most recent days, so a
/// busy afternoon can't push out how the Kobo was before it.
pub const DAILY_BACKUPS_KEPT: usize = 7;
/// Also kept: this many of the newest backups taken before a removal.
pub const REMOVAL_BACKUPS_KEPT: usize = 5;
/// How long removed book files, and the backups kept for them, are held.
pub const REMOVED_KEPT_DAYS: i64 = 30;

const PREFIX: &str = "KoboReader-";
const EXTENSION: &str = ".sqlite";
const REMOVED_DIR: &str = "removed";
const MANIFEST: &str = "manifest.json";
const STAMP: &str = "%Y%m%d-%H%M%S%.3f";

/// What's known about a backup, newest first in [`KoboDevice::backups`].
#[derive(Debug, Clone, Serialize)]
pub struct BackupInfo {
    pub file: String,
    pub at: String,
    /// The change this backup was taken before. Empty when nothing recorded it.
    pub entries: Vec<LogEntry>,
    /// Book files that change removed, still held on this computer.
    pub removed_files: usize,
    pub db_version: Option<u32>,
    /// Why this backup can't be put back; `None` when it can.
    pub blocked: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RestoreOutcome {
    /// The backup of how the Kobo was just before going back.
    pub safety_backup: String,
    /// Book files put back on the Kobo.
    pub files_restored: usize,
}

#[derive(Debug, Serialize, Deserialize)]
struct Sidecar {
    firmware: String,
    entries: Vec<LogEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemovedFile {
    /// Relative to the mount point.
    path: String,
    /// Name in this change's `removed/<stamp>/` folder.
    file: String,
}

impl KoboDevice {
    fn backup_dir(&self, root: &Path) -> PathBuf {
        root.join(sanitize(&self.version().serial))
    }

    /// Backups of this Kobo, newest first. Reads only.
    pub fn backups(&self, root: &Path) -> Vec<BackupInfo> {
        let dir = self.backup_dir(root);
        let current = readable_version(&self.db_path());
        let log = self.erc_state().log;
        let mut names = backup_names(&dir);
        names.reverse();
        names
            .into_iter()
            .map(|file| {
                let path = dir.join(&file);
                let sidecar = read_sidecar(&path);
                let db_version = readable_version(&path);
                let firmware = sidecar.as_ref().map(|s| s.firmware.as_str());
                let blocked = self.restore_blocker(current, db_version, firmware);
                let entries = sidecar.as_ref().map_or_else(
                    // Backups from before sidecars: the Kobo's own log names them.
                    || {
                        log.iter()
                            .filter(|e| e.backup.as_deref() == Some(file.as_str()))
                            .cloned()
                            .collect()
                    },
                    |s| s.entries.clone(),
                );
                BackupInfo {
                    at: stamp_time(&file).unwrap_or_default(),
                    removed_files: read_manifest(&removed_dir(&path)).len(),
                    entries,
                    db_version,
                    blocked,
                    file,
                }
            })
            .collect()
    }

    fn restore_blocker(
        &self,
        current: Option<u32>,
        backup: Option<u32>,
        firmware: Option<&str>,
    ) -> Option<String> {
        let Some(backup) = backup else {
            return Some("this backup can't be read".to_owned());
        };
        match current {
            Some(current) if current != backup => Some(format!(
                "this backup is from database version {backup}, and the Kobo is now on {current}"
            )),
            // A damaged database can't say its version, and is what backups
            // are for, so go by the firmware the backup was taken on.
            None if firmware.is_some_and(|f| f != self.version().firmware.to_string()) => {
                Some("this backup is from different firmware".to_owned())
            }
            _ => None,
        }
    }

    /// Puts `backup` (a file name, or a path in this Kobo's backup folder)
    /// back, with the book files removed since. The Kobo's current state is
    /// backed up first; nothing changes if that or any check fails.
    pub fn restore_backup(&self, root: &Path, backup: &Path) -> Result<RestoreOutcome> {
        let dir = self.backup_dir(root);
        let cannot = |why: &str| Error::CannotRestore(why.to_owned());
        let name = backup
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .filter(|n| is_backup(n))
            .ok_or_else(|| cannot("that isn't a Kobo database backup"))?;
        if let Some(parent) = backup.parent().filter(|p| !p.as_os_str().is_empty())
            && parent.canonicalize().ok() != dir.canonicalize().ok()
        {
            return Err(cannot("that backup is from a different Kobo"));
        }
        let path = dir.join(&name);
        if !path.is_file() {
            return Err(cannot("this Kobo has no such backup"));
        }

        let db = self.db_path();
        let sidecar = read_sidecar(&path);
        let current = readable_version(&db);
        let firmware = sidecar.as_ref().map(|s| s.firmware.as_str());
        if let Some(why) = self.restore_blocker(current, readable_version(&path), firmware) {
            return Err(Error::CannotRestore(why));
        }
        if current.is_some()
            && let Some(blocker) =
                crate::support::write_blocker(self.version(), &open_immutable(&db)?)?
        {
            return Err(blocker);
        }
        if quick_check(&path).as_deref() != Some("ok") {
            return Err(cannot("this backup is damaged"));
        }

        let safety = save_current(&db, &dir)?;
        let safety_name = file_name(&safety);
        let mut entry = LogEntry::new("restore").detail(name.clone());
        entry.backup = Some(safety_name.clone());
        self.write_sidecar(&safety, std::slice::from_ref(&entry));

        // Files first: a failure here leaves at worst an extra file Nickel
        // imports, where the other order would leave rows with no file.
        let books = self.put_back_removed(&dir, &name)?;
        let files_restored = books.len();
        restore(&db, &path)?;
        if quick_check_immutable(&db).as_deref() != Some("ok") {
            restore(&db, &safety)?;
            return Err(Error::RolledBack("the restored copy didn't verify".into()));
        }
        self.record(Record {
            entries: vec![entry],
            books,
        });
        prune(&dir);
        Ok(RestoreOutcome {
            safety_backup: safety_name,
            files_restored,
        })
    }

    /// Copies back what `backup`'s change and later ones removed, unless
    /// something is at that path now. Copies stay here, so a later change of
    /// mind can use them too. Oldest first: that's closest to the backup.
    fn put_back_removed(&self, dir: &Path, backup: &str) -> Result<Vec<BookChange>> {
        let from = stem(backup);
        let root = dir.join(REMOVED_DIR);
        let mut changes = Vec::new();
        let mut stamps: Vec<String> = fs::read_dir(&root)
            .map(|d| {
                d.flatten()
                    .map(|e| e.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        stamps.retain(|s| s.as_str() >= from);
        stamps.sort();
        for stamp in stamps {
            let held = root.join(stamp);
            for file in read_manifest(&held) {
                let safe = Path::new(&file.path)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_)));
                let src = held.join(&file.file);
                if !safe || !src.is_file() || self.mount().join(&file.path).exists() {
                    continue;
                }
                self.write_file(&src, &file.path)?;
                changes.extend(BookChange::written(&file.path, &src, None));
            }
        }
        Ok(changes)
    }

    pub(crate) fn write_sidecar(&self, backup: &Path, entries: &[LogEntry]) {
        let sidecar = Sidecar {
            firmware: self.version().firmware.to_string(),
            entries: entries.to_vec(),
        };
        if let Ok(json) = serde_json::to_vec_pretty(&sidecar) {
            let _ = fs::write(backup.with_extension("json"), json);
        }
    }
}

/// Copies a book file about to be removed next to `backup`, the backup of the
/// change removing it. `path` is the file, `rel` its path on the device.
pub(crate) fn hold_removed(backup: &Path, path: &Path, rel: &str) -> Result<()> {
    let dir = removed_dir(backup);
    fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
    let mut held = read_manifest(&dir);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    let file = format!("{}-{name}", held.len());
    let dest = dir.join(&file);
    let _ = fs::remove_file(&dest);
    crate::transfer::copy_plain(path, &dest)?;
    held.push(RemovedFile {
        path: rel.to_owned(),
        file,
    });
    let json = serde_json::to_vec_pretty(&held).map_err(|e| io_err(&dir, e.into()))?;
    let manifest = dir.join(MANIFEST);
    fs::write(&manifest, json).map_err(|e| io_err(&manifest, e))
}

/// Backs the open database up into `dir`, verified.
pub(crate) fn take_backup(conn: &Connection, dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let path = new_backup_path(dir);
    {
        let mut dest = Connection::open(&path)?;
        let backup = Backup::new(conn, &mut dest)?;
        backup.run_to_completion(256, std::time::Duration::ZERO, None)?;
    }
    let check: String =
        Connection::open(&path)?.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
    if check != "ok" {
        let _ = fs::remove_file(&path);
        return Err(Error::CorruptBefore(format!(
            "backup failed verification: {check}"
        )));
    }
    Ok(path)
}

/// Saves the database as it is, even if SQLite can't read it: a damaged
/// database is the usual reason to go back, and is still worth keeping.
fn save_current(db: &Path, dir: &Path) -> Result<PathBuf> {
    let online = Connection::open_with_flags(
        db,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(Error::from)
    .and_then(|conn| take_backup(&conn, dir));
    if let Ok(path) = online {
        return Ok(path);
    }
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let path = new_backup_path(dir);
    crate::transfer::copy_plain(db, &path)?;
    Ok(path)
}

fn new_backup_path(dir: &Path) -> PathBuf {
    loop {
        let stamp = Utc::now().format(STAMP);
        let path = dir.join(format!("{PREFIX}{stamp}{EXTENSION}"));
        if !path.exists() {
            return path;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Applies the retention rules (see the constants) and forgets removed book
/// files past [`REMOVED_KEPT_DAYS`].
pub(crate) fn prune(dir: &Path) {
    let names = backup_names(dir);
    let cutoff = format!(
        "{PREFIX}{}",
        (Utc::now() - Duration::days(REMOVED_KEPT_DAYS)).format(STAMP)
    );
    let mut keep: HashSet<&str> = names
        .iter()
        .rev()
        .take(BACKUPS_KEPT)
        .map(String::as_str)
        .collect();
    let mut days: BTreeMap<&str, &str> = BTreeMap::new();
    for name in &names {
        // Timestamped names sort chronologically, so the first seen is earliest.
        days.entry(day(name)).or_insert(name);
    }
    keep.extend(days.values().rev().take(DAILY_BACKUPS_KEPT));
    keep.extend(
        names
            .iter()
            .rev()
            .filter(|n| **n >= cutoff && was_removal(&dir.join(n)))
            .take(REMOVAL_BACKUPS_KEPT)
            .map(String::as_str),
    );
    for name in names.iter().filter(|n| !keep.contains(n.as_str())) {
        let path = dir.join(name);
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(path.with_extension("json"));
    }

    let Ok(held) = fs::read_dir(dir.join(REMOVED_DIR)) else {
        return;
    };
    for entry in held.flatten() {
        if entry.file_name().to_string_lossy().as_ref() < stem(&cutoff) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

fn was_removal(backup: &Path) -> bool {
    read_sidecar(backup).is_some_and(|s| s.entries.iter().any(|e| e.op == "remove"))
}

fn is_backup(name: &str) -> bool {
    name.starts_with(PREFIX) && name.ends_with(EXTENSION) && !name.contains(['/', '\\'])
}

/// Backup file names in `dir`, oldest first.
fn backup_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|d| {
            d.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| is_backup(n))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn stem(name: &str) -> &str {
    name.strip_suffix(EXTENSION).unwrap_or(name)
}

/// `20261104` from `KoboReader-20261104-210442.223.sqlite`.
fn day(name: &str) -> &str {
    let stamp = name.strip_prefix(PREFIX).unwrap_or(name);
    stamp.split('-').next().unwrap_or(stamp)
}

fn stamp_time(name: &str) -> Option<String> {
    let stamp = stem(name).strip_prefix(PREFIX)?;
    let time = NaiveDateTime::parse_from_str(stamp, STAMP).ok()?;
    Some(
        Utc.from_utc_datetime(&time)
            .to_rfc3339_opts(SecondsFormat::Millis, true),
    )
}

fn removed_dir(backup: &Path) -> PathBuf {
    let name = backup.file_name().unwrap_or_default().to_string_lossy();
    backup
        .parent()
        .unwrap_or(backup)
        .join(REMOVED_DIR)
        .join(stem(&name))
}

fn read_sidecar(backup: &Path) -> Option<Sidecar> {
    let bytes = fs::read(backup.with_extension("json")).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn read_manifest(dir: &Path) -> Vec<RemovedFile> {
    fs::read(dir.join(MANIFEST))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn readable_version(db: &Path) -> Option<u32> {
    open_immutable(db)
        .ok()?
        .query_row("SELECT version FROM DbVersion", [], |r| r.get(0))
        .ok()
}

fn quick_check(path: &Path) -> Option<String> {
    Connection::open(path)
        .ok()?
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .ok()
}

/// As [`quick_check`], leaving no side files: for the database on the device.
fn quick_check_immutable(path: &Path) -> Option<String> {
    open_immutable(path)
        .ok()?
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .ok()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

pub(crate) fn sanitize(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}
