//! The only way this app writes to `KoboReader.sqlite`.
//!
//! Every write session:
//! 1. refuses if Nickel left `-wal`/`-shm`/`-journal` files (not cleanly
//!    handed over) or the schema is outside the range calibre has validated;
//! 2. runs `PRAGMA quick_check` and backs the database up with SQLite's online
//!    backup API, keeping the newest [`BACKUPS_KEPT`] per device;
//! 3. applies changes in immediate transactions;
//! 4. on close runs `PRAGMA integrity_check`, restoring the backup if it fails,
//!    then checkpoints the WAL and closes so no side files are left beside the
//!    database (the bundled SQLite deletes them on close, unlike Apple's).

use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use rusqlite::backup::Backup;
use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};

use crate::KoboDevice;
use crate::db::{MAX_TESTED_DB_VERSION, MIN_SUPPORTED_DB_VERSION};
use crate::error::{Error, Result};
use crate::state::{BookChange, LogEntry, Record};

/// Backups kept per device; older ones are deleted.
pub const BACKUPS_KEPT: usize = 10;

const SIDE_FILES: [&str; 3] = ["-wal", "-shm", "-journal"];

pub struct DeviceWriter<'a> {
    device: &'a KoboDevice,
    conn: Option<Connection>,
    backup: PathBuf,
    /// Written to `/.erc/` when the session ends.
    record: Record,
}

impl KoboDevice {
    /// Opens a write session, backing the database up under
    /// `backup_root/<serial>/` first.
    pub fn open_writer(&self, backup_root: &Path) -> Result<DeviceWriter<'_>> {
        let db_path = self.db_path();
        for suffix in SIDE_FILES {
            let side = side_file(&db_path, suffix);
            if side.exists() {
                return Err(Error::DatabaseBusy(
                    side.file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned(),
                ));
            }
        }
        let conn = Connection::open_with_flags(
            &db_path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        conn.busy_timeout(std::time::Duration::from_secs(2))?;
        let version: u32 = conn.query_row("SELECT version FROM DbVersion", [], |r| r.get(0))?;
        if version < MIN_SUPPORTED_DB_VERSION {
            return Err(Error::UnsupportedDbVersion(version));
        }
        if version > MAX_TESTED_DB_VERSION {
            return Err(Error::UntestedDbVersion(version));
        }
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(Error::CorruptBefore(check));
        }
        let backup = backup_to(&conn, &backup_root.join(sanitize(&self.version().serial)))?;
        Ok(DeviceWriter {
            device: self,
            conn: Some(conn),
            backup,
            record: Record::default(),
        })
    }
}

impl<'a> DeviceWriter<'a> {
    pub fn device(&self) -> &'a KoboDevice {
        self.device
    }

    /// Where this session's pre-change backup is.
    pub fn backup_path(&self) -> &Path {
        &self.backup
    }

    /// Adds an operation to this session's entry in `/.erc/log.jsonl`.
    pub fn log(&mut self, entry: LogEntry) {
        self.record.entries.push(entry);
    }

    /// Notes a book file ERC wrote or removed, for `/.erc/books.json`.
    pub fn track(&mut self, change: BookChange) {
        self.record.books.push(change);
    }

    /// Runs `f` in an immediate transaction, committing only if it succeeds.
    pub fn transaction<T>(&mut self, f: impl FnOnce(&Transaction) -> Result<T>) -> Result<T> {
        let conn = self.conn.as_mut().expect("connection open until close()");
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let value = f(&tx)?;
        tx.commit()?;
        Ok(value)
    }

    /// Verifies integrity, then checkpoints and closes. If the check fails the
    /// pre-change backup is restored and [`Error::RolledBack`] returned.
    pub fn close(mut self) -> Result<()> {
        self.finish()
    }

    fn finish(&mut self) -> Result<()> {
        let Some(conn) = self.conn.take() else {
            return Ok(());
        };
        let result = self.verify_and_close(conn);
        let mut record = std::mem::take(&mut self.record);
        let backup = self
            .backup
            .file_name()
            .map(|n| n.to_string_lossy().into_owned());
        for entry in &mut record.entries {
            entry.backup.clone_from(&backup);
            if let Err(e) = &result {
                entry.error = Some(e.to_string());
            }
        }
        self.device.record(record);
        result
    }

    fn verify_and_close(&self, conn: Connection) -> Result<()> {
        let check: String = conn.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if check != "ok" {
            drop(conn);
            restore(&self.device.db_path(), &self.backup)?;
            return Err(Error::RolledBack(check));
        }
        conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_| Ok(()))?;
        conn.close().map_err(|(_, e)| Error::Sqlite(e))?;
        // macOS tags files it writes with an xattr that FAT32 stores as
        // `._<name>`; drop those for the database and its side files.
        let db_path = self.device.db_path();
        crate::transfer::remove_apple_double(&db_path);
        for suffix in SIDE_FILES {
            crate::transfer::remove_apple_double(&side_file(&db_path, suffix));
        }
        Ok(())
    }
}

impl Drop for DeviceWriter<'_> {
    /// Sessions should end with [`DeviceWriter::close`]; this is the fallback
    /// that still verifies and cleans up.
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Copies a backup over the device database. Only call with no connection
/// open to it.
pub fn restore(db_path: &Path, backup: &Path) -> Result<()> {
    for suffix in SIDE_FILES {
        let _ = fs::remove_file(side_file(db_path, suffix));
    }
    let tmp = db_path.with_extension("sqlite.erc-restore");
    let _ = fs::remove_file(&tmp);
    crate::transfer::copy_plain(backup, &tmp)?;
    fs::rename(&tmp, db_path).map_err(|e| io_err(db_path, e))?;
    crate::transfer::remove_apple_double(db_path);
    Ok(())
}

fn backup_to(conn: &Connection, dir: &Path) -> Result<PathBuf> {
    fs::create_dir_all(dir).map_err(|e| io_err(dir, e))?;
    let stamp = Utc::now().format("%Y%m%d-%H%M%S%.3f");
    let path = dir.join(format!("KoboReader-{stamp}.sqlite"));
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
    prune(dir);
    Ok(path)
}

fn prune(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    let mut backups: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            let name = p.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with("KoboReader-") && name.ends_with(".sqlite")
        })
        .collect();
    // Timestamped names sort chronologically.
    backups.sort();
    let excess = backups.len().saturating_sub(BACKUPS_KEPT);
    for old in &backups[..excess] {
        let _ = fs::remove_file(old);
    }
}

fn side_file(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn sanitize(s: &str) -> String {
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
