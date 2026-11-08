//! Database backups on this computer, and going back to one.
//!
//! Under `<backup root>/<serial>/`:
//! ```text
//! KoboReader-<stamp>.sqlite.zst  the database just before a change
//! KoboReader-<stamp>.json        what that change was
//! removed/<stamp>/               book files that change removed, with manifest.json
//! ```
//!
//! Each backup is compressed on its own, about 19 to 1 on the reference Kobo.
//! Storing only what changed between backups would save a little more, but
//! then one damaged file could take several backups with it. Backups from
//! before compression (`.sqlite`) still work, and are compressed at the next
//! write session.
//!
//! A database backup can't bring back a deleted file, so removals copy the
//! file here first. Going back to a backup puts back the files removed by it
//! and every later change, then the database, after saving the current state
//! as one more backup so that going back can itself be undone.
//!
//! How much is kept is the user's [`BackupPolicy`], saved beside the backups
//! as `policy.json` so every write session applies it.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use chrono::{Duration, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

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

/// The whole backup folder's default size limit.
pub const DEFAULT_MAX_BYTES: u64 = 500_000_000;

const PREFIX: &str = "KoboReader-";
const EXTENSION: &str = ".sqlite";
const PACKED: &str = ".sqlite.zst";
/// Measured on a real 37.5 MB database: 1.98 MB in 0.14 s. Level 19 saves
/// another 0.2 MB but takes 6.5 s, and every write session waits for this.
const LEVEL: i32 = 9;
/// Unfinished work older than this was left by a crash.
const STALE_SECS: u64 = 3600;
const REMOVED_DIR: &str = "removed";
const MANIFEST: &str = "manifest.json";
const STAMP: &str = "%Y%m%d-%H%M%S%.3f";
const POLICY_FILE: &str = "policy.json";

/// Which backups each Kobo keeps.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Keep {
    /// Recent changes, each day's first, and removals: see the constants.
    #[default]
    History,
    /// Only how the Kobo was before the newest change.
    Latest,
}

/// How much of the Kobo's history is kept on this computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct BackupPolicy {
    pub keep: Keep,
    /// The most the whole backup folder may take, or `None` for no limit.
    /// The oldest backups go first, but each Kobo's newest one always stays,
    /// so the folder can be over the limit when that alone is bigger.
    pub max_bytes: Option<u64>,
}

impl Default for BackupPolicy {
    fn default() -> Self {
        Self {
            keep: Keep::default(),
            max_bytes: Some(DEFAULT_MAX_BYTES),
        }
    }
}

impl BackupPolicy {
    /// The policy saved under `root`, or the default.
    pub fn load(root: &Path) -> Self {
        fs::read(root.join(POLICY_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Saves the policy under `root` and applies it to every Kobo's backups.
    pub fn save(&self, root: &Path) -> Result<()> {
        fs::create_dir_all(root).map_err(|e| io_err(root, e))?;
        let path = root.join(POLICY_FILE);
        let tmp = path.with_extension("json.tmp");
        let json = serde_json::to_vec_pretty(self).map_err(|e| io_err(&path, e.into()))?;
        fs::write(&tmp, json)
            .and_then(|()| fs::rename(&tmp, &path))
            .map_err(|e| io_err(&path, e))?;
        for dir in device_dirs(root) {
            prune_dir(&dir, self);
        }
        limit_size(root, self);
        Ok(())
    }
}

/// Bytes every Kobo's backups take, removed book files included.
pub fn backups_size(root: &Path) -> u64 {
    fn walk(path: &Path) -> u64 {
        let Ok(entries) = fs::read_dir(path) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| match entry.file_type() {
                Ok(t) if t.is_dir() => walk(&entry.path()),
                Ok(t) if t.is_file() => entry.metadata().map_or(0, |m| m.len()),
                _ => 0,
            })
            .sum()
    }
    device_dirs(root).iter().map(|dir| walk(dir)).sum()
}

/// Deletes every Kobo's backups and held book files, keeping the policy.
/// Nothing can be gone back to afterwards.
pub fn delete_all_backups(root: &Path) -> Result<()> {
    for dir in device_dirs(root) {
        fs::remove_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
    }
    Ok(())
}

/// One folder per Kobo, by serial.
fn device_dirs(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .map(|e| e.path())
                .collect()
        })
        .unwrap_or_default()
}

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

#[derive(Debug, Default, Serialize, Deserialize)]
struct Sidecar {
    #[serde(default)]
    firmware: Option<String>,
    /// Here so listing backups doesn't have to decompress them.
    #[serde(default)]
    db_version: Option<u32>,
    entries: Vec<LogEntry>,
}

/// A backup as a plain database file, unpacked for as long as this lives.
pub(crate) struct Plain {
    path: PathBuf,
    temporary: bool,
}

impl Plain {
    pub(crate) fn open(backup: &Path) -> Result<Self> {
        let name = file_name(backup);
        if !name.ends_with(PACKED) {
            return Ok(Self {
                path: backup.to_path_buf(),
                temporary: false,
            });
        }
        let pid = std::process::id();
        let dir = backup.parent().unwrap_or(backup);
        let plain = Self {
            path: dir.join(format!("{}.{pid}.unpacked", stem(&name))),
            temporary: true,
        };
        let source = File::open(backup).map_err(|e| io_err(backup, e))?;
        let dest = File::create(&plain.path).map_err(|e| io_err(&plain.path, e))?;
        zstd::stream::copy_decode(source, dest).map_err(|e| io_err(backup, e))?;
        Ok(plain)
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Plain {
    fn drop(&mut self) {
        if self.temporary {
            let _ = fs::remove_file(&self.path);
        }
    }
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
                let sidecar = read_sidecar(&path).unwrap_or_default();
                let db_version = sidecar.db_version.or_else(|| {
                    Plain::open(&path)
                        .ok()
                        .and_then(|p| readable_version(p.path()))
                });
                let firmware = sidecar.firmware.as_deref();
                let blocked = self.restore_blocker(current, db_version, firmware);
                let mut entries = sidecar.entries;
                if entries.is_empty() {
                    // Backups from before sidecars: the Kobo's own log names them.
                    entries = log
                        .iter()
                        .filter(|e| e.backup.as_deref().is_some_and(|b| stem(b) == stem(&file)))
                        .cloned()
                        .collect();
                }
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
        let path = locate(&dir, &name).ok_or_else(|| cannot("this Kobo has no such backup"))?;
        let plain = Plain::open(&path).map_err(|_| cannot("this backup is damaged"))?;

        let db = self.db_path();
        let sidecar = read_sidecar(&path);
        let current = readable_version(&db);
        let firmware = sidecar.as_ref().and_then(|s| s.firmware.as_deref());
        if let Some(why) = self.restore_blocker(current, readable_version(plain.path()), firmware) {
            return Err(Error::CannotRestore(why));
        }
        if current.is_some()
            && let Some(blocker) =
                crate::support::write_blocker(self.version(), &open_immutable(&db)?)?
        {
            return Err(blocker);
        }
        if quick_check(plain.path()).as_deref() != Some("ok") {
            return Err(cannot("this backup is damaged"));
        }

        let safety = save_current(&db, &dir)?;
        let safety_name = file_name(&safety);
        let mut entry = LogEntry::new("restore").detail(name.clone());
        entry.backup = Some(safety_name.clone());
        self.write_sidecar(&safety, std::slice::from_ref(&entry), current);

        // Files first: a failure here leaves at worst an extra file Nickel
        // imports, where the other order would leave rows with no file.
        let books = self.put_back_removed(&dir, &name)?;
        let files_restored = books.len();
        restore(&db, plain.path())?;
        if quick_check(&db).as_deref() != Some("ok") {
            restore(&db, &safety)?;
            return Err(Error::RolledBack("the restored copy didn't verify".into()));
        }
        self.record(Record {
            entries: vec![entry],
            books,
        });
        prune(root, &dir);
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

    pub(crate) fn write_sidecar(
        &self,
        backup: &Path,
        entries: &[LogEntry],
        db_version: Option<u32>,
    ) {
        save_sidecar(
            backup,
            &Sidecar {
                firmware: Some(self.version().firmware.to_string()),
                db_version,
                entries: entries.to_vec(),
            },
        );
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

/// Backs the open database up into `dir`, verified, and compresses it.
pub(crate) fn take_backup(conn: &Connection, dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let path = new_backup_path(dir);
    let staging = staging_path(&path);
    let result = (|| {
        {
            let mut dest = Connection::open(&staging)?;
            let backup = Backup::new(conn, &mut dest)?;
            backup.run_to_completion(256, std::time::Duration::ZERO, None)?;
        }
        let check: String =
            Connection::open(&staging)?.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(Error::CorruptBefore(format!(
                "backup failed verification: {check}"
            )));
        }
        pack(&staging, &path)
    })();
    let _ = fs::remove_file(&staging);
    result.map(|()| path)
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
    let staging = staging_path(&path);
    let result = crate::transfer::copy_plain(db, &staging).and_then(|_| pack(&staging, &path));
    let _ = fs::remove_file(&staging);
    result.map(|()| path)
}

/// Compresses `plain` to `dest`, which only appears once reading it back
/// gives exactly the same bytes.
fn pack(plain: &Path, dest: &Path) -> Result<()> {
    let tmp = dest.with_extension("zst.tmp");
    let result = (|| -> io::Result<()> {
        let mut encoder = zstd::Encoder::new(File::create(&tmp)?, LEVEL)?;
        io::copy(&mut File::open(plain)?, &mut encoder)?;
        encoder.finish()?.sync_all()?;
        let unpacked = zstd::Decoder::new(File::open(&tmp)?)?;
        if digest(unpacked)? != digest(File::open(plain)?)? {
            return Err(io::Error::other("compressed copy doesn't match"));
        }
        fs::rename(&tmp, dest)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result.map_err(|e| io_err(dest, e))
}

fn digest(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1 << 20];
    loop {
        match reader.read(&mut buffer)? {
            0 => return Ok(hash.finalize().to_vec()),
            n => hash.update(&buffer[..n]),
        }
    }
}

fn new_backup_path(dir: &Path) -> PathBuf {
    loop {
        let stamp = Utc::now().format(STAMP);
        let path = dir.join(format!("{PREFIX}{stamp}{PACKED}"));
        if !path.exists() {
            return path;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

/// Where a backup is before it's compressed.
fn staging_path(path: &Path) -> PathBuf {
    path.with_file_name(format!("{}.sqlite.tmp", stem(&file_name(path))))
}

/// The file for a backup name, which may have been compressed since listing.
fn locate(dir: &Path, name: &str) -> Option<PathBuf> {
    [PACKED, EXTENSION]
        .iter()
        .map(|extension| dir.join(format!("{}{extension}", stem(name))))
        .find(|path| path.is_file())
}

/// Compresses backups from before compression. A failure leaves the backup
/// as it was, to try again next time.
fn compress_existing(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for path in entries.flatten().map(|e| e.path()) {
        let name = file_name(&path);
        if !is_backup(&name) || name.ends_with(PACKED) {
            continue;
        }
        let packed = dir.join(format!("{}{PACKED}", stem(&name)));
        if packed.is_file() {
            // Stopped after packing it, which only finishes once verified.
            let _ = fs::remove_file(&path);
            continue;
        }
        let mut sidecar = read_sidecar(&path).unwrap_or_default();
        sidecar.db_version = sidecar.db_version.or_else(|| readable_version(&path));
        if pack(&path, &packed).is_ok() {
            save_sidecar(&packed, &sidecar);
            let _ = fs::remove_file(&path);
        }
    }
}

/// Forgets temporary files a crash left behind.
fn remove_stale(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let old = entry
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > STALE_SECS);
        let path = entry.path();
        let temporary = path
            .extension()
            .is_some_and(|e| e == "tmp" || e == "unpacked");
        if old && temporary {
            let _ = fs::remove_file(path);
        }
    }
}

/// Applies the saved [`BackupPolicy`] after a backup of the Kobo in `dir`.
pub(crate) fn prune(root: &Path, dir: &Path) {
    let policy = BackupPolicy::load(root);
    prune_dir(dir, &policy);
    limit_size(root, &policy);
}

/// Applies `policy.keep` to one Kobo's backups, and forgets removed book
/// files past [`REMOVED_KEPT_DAYS`] or older than every backup left.
fn prune_dir(dir: &Path, policy: &BackupPolicy) {
    compress_existing(dir);
    remove_stale(dir);
    let names = backup_names(dir);
    let cutoff = format!(
        "{PREFIX}{}",
        (Utc::now() - Duration::days(REMOVED_KEPT_DAYS)).format(STAMP)
    );
    let keep: HashSet<&str> = match policy.keep {
        Keep::Latest => names.last().map(String::as_str).into_iter().collect(),
        Keep::History => {
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
            keep
        }
    };
    for name in names.iter().filter(|n| !keep.contains(n.as_str())) {
        delete_backup(&dir.join(name));
    }
    forget_held(dir, stem(&cutoff));
}

/// Removes one backup and its record of what changed.
fn delete_backup(path: &Path) {
    let _ = fs::remove_file(sidecar_path(path));
    let _ = fs::remove_file(path);
}

/// Forgets removed book files from before `cutoff`, and those no backup left
/// can bring back: going back to a backup puts back what it and later
/// changes removed, so files older than the oldest backup have no use.
fn forget_held(dir: &Path, cutoff: &str) {
    let names = backup_names(dir);
    let oldest = names.first().map(|n| stem(n));
    let Ok(held) = fs::read_dir(dir.join(REMOVED_DIR)) else {
        return;
    };
    for entry in held.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.as_str() < cutoff || oldest.is_none_or(|oldest| name.as_str() < oldest) {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}

/// Deletes the oldest backups, across every Kobo, until the folder fits
/// `policy.max_bytes`. Each Kobo's newest backup stays regardless.
fn limit_size(root: &Path, policy: &BackupPolicy) {
    let Some(max) = policy.max_bytes else {
        return;
    };
    let cutoff = format!(
        "{PREFIX}{}",
        (Utc::now() - Duration::days(REMOVED_KEPT_DAYS)).format(STAMP)
    );
    while backups_size(root) > max {
        let oldest = device_dirs(root)
            .into_iter()
            .filter_map(|dir| {
                let names = backup_names(&dir);
                // The newest is never a candidate.
                let first = names.split_last()?.1.first()?.clone();
                Some((first, dir))
            })
            .min_by(|a, b| a.0.cmp(&b.0));
        let Some((name, dir)) = oldest else {
            return;
        };
        let path = dir.join(&name);
        delete_backup(&path);
        if path.exists() {
            // Can't be deleted; trying again would never end.
            return;
        }
        forget_held(&dir, stem(&cutoff));
    }
}

fn was_removal(backup: &Path) -> bool {
    read_sidecar(backup).is_some_and(|s| s.entries.iter().any(|e| e.op == "remove"))
}

fn is_backup(name: &str) -> bool {
    name.starts_with(PREFIX)
        && (name.ends_with(EXTENSION) || name.ends_with(PACKED))
        && !name.contains(['/', '\\'])
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
    // Both forms exist if compression stopped just before tidying up.
    names.dedup_by(|a, b| stem(a) == stem(b));
    names
}

fn stem(name: &str) -> &str {
    name.strip_suffix(PACKED)
        .or_else(|| name.strip_suffix(EXTENSION))
        .unwrap_or(name)
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

fn sidecar_path(backup: &Path) -> PathBuf {
    backup.with_file_name(format!("{}.json", stem(&file_name(backup))))
}

fn read_sidecar(backup: &Path) -> Option<Sidecar> {
    let bytes = fs::read(sidecar_path(backup)).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_sidecar(backup: &Path, sidecar: &Sidecar) {
    if let Ok(json) = serde_json::to_vec_pretty(sidecar) {
        let _ = fs::write(sidecar_path(backup), json);
    }
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

/// Leaves no side files, so it's safe for the database on the device.
fn quick_check(path: &Path) -> Option<String> {
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
