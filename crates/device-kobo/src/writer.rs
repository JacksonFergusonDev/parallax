//! The only way this app writes to `KoboReader.sqlite`.
//!
//! Every write session:
//! 1. refuses if Nickel left `-wal`/`-shm`/`-journal` files (not cleanly
//!    handed over) or ERC doesn't support changing this Kobo
//!    ([`crate::support`]);
//! 2. runs `PRAGMA quick_check` and backs the database up with SQLite's online
//!    backup API ([`crate::backups`] has what's kept and how to go back);
//! 3. applies changes in immediate transactions;
//! 4. on close runs `PRAGMA integrity_check`, restoring the backup if it fails,
//!    then checkpoints the WAL and closes so no side files are left beside the
//!    database (the bundled SQLite deletes them on close, unlike Apple's).

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, Transaction, TransactionBehavior};

use crate::KoboDevice;
use crate::backups::{Plain, hold_removed, prune, sanitize, take_backup};
use crate::error::{Error, Result};
use crate::state::{BookChange, LogEntry, Record};

const SIDE_FILES: [&str; 3] = ["-wal", "-shm", "-journal"];

pub struct DeviceWriter<'a> {
    device: &'a KoboDevice,
    conn: Option<Connection>,
    backup: PathBuf,
    db_version: Option<u32>,
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
        if let Some(blocker) = crate::support::write_blocker(self.version(), &conn)? {
            return Err(blocker);
        }
        let check: String = conn.query_row("PRAGMA quick_check", [], |r| r.get(0))?;
        if check != "ok" {
            return Err(Error::CorruptBefore(check));
        }
        let dir = backup_root.join(sanitize(&self.version().serial));
        let backup = take_backup(&conn, &dir)?;
        let db_version = conn
            .query_row("SELECT version FROM DbVersion", [], |r| r.get(0))
            .ok();
        self.write_sidecar(&backup, &[], db_version);
        prune(&dir);
        Ok(DeviceWriter {
            device: self,
            conn: Some(conn),
            backup,
            db_version,
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

    /// Keeps a copy of a book file on this computer before it's removed, so
    /// going back to this session's backup can bring it back too.
    pub(crate) fn hold_removed(&self, path: &Path, device_path: &str) -> Result<()> {
        hold_removed(&self.backup, path, device_path)
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
        self.device
            .write_sidecar(&self.backup, &record.entries, self.db_version);
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
/// open to it. The database and its side files stay as they were until the
/// whole backup is on the device.
pub fn restore(db_path: &Path, backup: &Path) -> Result<()> {
    let plain = Plain::open(backup)?;
    let backup = plain.path();
    let tmp = db_path.with_extension("sqlite.erc-restore");
    let _ = fs::remove_file(&tmp);
    let bytes = crate::transfer::copy_plain(backup, &tmp).inspect_err(|_| {
        let _ = fs::remove_file(&tmp);
    })?;
    let expected = fs::metadata(backup).map_err(|e| io_err(backup, e))?.len();
    if bytes != expected {
        let _ = fs::remove_file(&tmp);
        return Err(Error::IncompleteCopy {
            path: db_path.to_path_buf(),
            expected,
            written: bytes,
        });
    }
    for suffix in SIDE_FILES {
        let _ = fs::remove_file(side_file(db_path, suffix));
    }
    fs::rename(&tmp, db_path).map_err(|e| io_err(db_path, e))?;
    crate::transfer::remove_apple_double(db_path);
    Ok(())
}

fn side_file(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}
