//! What ERC has done to a Kobo, kept on the Kobo itself in `/.erc/`.
//!
//! Inspired by calibre's `driveinfo.calibre` (who manages this device) and
//! `metadata.calibre` (its view of every book), so another computer, a later
//! ERC version or a person debugging can tell which books ERC wrote and what
//! it did:
//!
//! - `device.json`: an id ERC gives the device, plus the ERC version, firmware
//!   and database version at the first and the latest write;
//! - `books.json`: every book ERC copied, with its SHA-256 and size at write
//!   time, so changes made by something else can be spotted;
//! - `log.jsonl`: an append-only list of operations, newest last, capped at
//!   [`LOG_KEPT`] entries.
//!
//! Nickel ignores dot folders. Everything here is best effort: a missing or
//! unreadable file never blocks a change (it is set aside and rebuilt), and
//! failing to record never fails the change that was made.

use std::collections::BTreeMap;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::KoboDevice;
use crate::db::ONBOARD_PREFIX;

pub const STATE_DIR: &str = ".erc";
/// Log entries kept; older ones are dropped.
pub const LOG_KEPT: usize = 2000;
const FORMAT: u32 = 1;
/// The ERC release doing the writing (all crates share the workspace version).
pub const ERC_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stamp {
    pub at: String,
    pub erc_version: String,
    pub firmware: String,
    pub db_version: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceRecord {
    pub format: u32,
    pub device_id: String,
    pub serial: String,
    pub first_write: Stamp,
    pub last_write: Stamp,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManagedBook {
    pub content_id: String,
    /// Relative to the mount point.
    pub path: String,
    /// The desktop library's id for the book, when it came from the library.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library_book: Option<i64>,
    pub sha256: String,
    pub size: u64,
    pub first_written: String,
    pub last_written: String,
    pub erc_version: String,
    pub writes: u32,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BooksFile {
    format: u32,
    books: Vec<ManagedBook>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogEntry {
    pub at: String,
    pub erc_version: String,
    /// e.g. `send`, `replace`, `remove`, `collection`, `series`, `tidy`.
    pub op: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// File name of the database backup taken before the change.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup: Option<String>,
    /// `None` when it worked, else what went wrong.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl LogEntry {
    pub fn new(op: &str) -> Self {
        Self {
            at: now(),
            erc_version: ERC_VERSION.to_owned(),
            op: op.to_owned(),
            content_ids: Vec::new(),
            detail: None,
            backup: None,
            error: None,
        }
    }

    pub fn ids(mut self, ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.content_ids = ids.into_iter().map(Into::into).collect();
        self
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }
}

/// A change to the list of managed books.
#[derive(Debug, Clone)]
pub enum BookChange {
    Written {
        path: String,
        sha256: String,
        size: u64,
        library_book: Option<i64>,
    },
    Removed {
        content_id: String,
    },
}

impl BookChange {
    /// `src` is the local file that was copied to `path`; hashed now, while
    /// it still exists.
    pub fn written(path: &str, src: &Path, library_book: Option<i64>) -> Option<Self> {
        let (sha256, size) = hash_file(src).ok()?;
        Some(Self::Written {
            path: path.to_owned(),
            sha256,
            size,
            library_book,
        })
    }
}

/// Everything recorded for one operation.
#[derive(Debug, Clone, Default)]
pub struct Record {
    pub entries: Vec<LogEntry>,
    pub books: Vec<BookChange>,
}

/// What `/.erc/` says, for display. Problems reading it are listed rather
/// than returned as errors.
#[derive(Debug, Clone, Default, Serialize)]
pub struct ErcState {
    pub device: Option<DeviceRecord>,
    pub books: Vec<ManagedBook>,
    /// Newest first.
    pub log: Vec<LogEntry>,
    pub problems: Vec<String>,
}

/// How a managed book compares with what ERC wrote.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BookCheck {
    Unchanged,
    /// Different size: replaced or edited by something other than ERC.
    Changed,
    Missing,
}

impl ErcState {
    /// Compares each managed book's file with what was recorded. Cheap: a
    /// size check, not a hash.
    pub fn check(&self, mount: &Path) -> Vec<(String, BookCheck)> {
        self.books
            .iter()
            .map(|b| {
                let check = match fs::metadata(mount.join(&b.path)) {
                    Err(_) => BookCheck::Missing,
                    Ok(m) if m.len() != b.size => BookCheck::Changed,
                    Ok(_) => BookCheck::Unchanged,
                };
                (b.content_id.clone(), check)
            })
            .collect()
    }
}

impl KoboDevice {
    fn state_dir(&self) -> PathBuf {
        self.mount().join(STATE_DIR)
    }

    /// Books Parallax wrote, without loading the operation log.
    pub fn managed_books(&self) -> Result<Vec<ManagedBook>, String> {
        read_json::<BooksFile>(&self.state_dir().join("books.json"))
            .map(|file| file.map_or_else(Vec::new, |file| file.books))
    }

    /// Reads `/.erc/` without changing anything.
    pub fn erc_state(&self) -> ErcState {
        let dir = self.state_dir();
        let mut state = ErcState::default();
        match read_json::<DeviceRecord>(&dir.join("device.json")) {
            Ok(d) => state.device = d,
            Err(e) => state.problems.push(format!("device.json: {e}")),
        }
        match self.managed_books() {
            Ok(books) => state.books = books,
            Err(e) => state.problems.push(format!("books.json: {e}")),
        }
        let (mut log, bad) = read_log(&dir.join("log.jsonl"));
        if bad > 0 {
            state
                .problems
                .push(format!("log.jsonl: {bad} unreadable lines skipped"));
        }
        log.reverse();
        state.log = log;
        state
    }

    /// Writes `record` to `/.erc/`. Best effort; see the module docs.
    pub fn record(&self, record: Record) {
        if record.entries.is_empty() && record.books.is_empty() {
            return;
        }
        let _ = self.try_record(record);
    }

    fn try_record(&self, record: Record) -> std::io::Result<()> {
        let dir = self.state_dir();
        let created = !dir.exists();
        fs::create_dir_all(&dir)?;
        if created {
            crate::transfer::remove_apple_double(&dir);
        }
        let stamp = Stamp {
            at: now(),
            erc_version: ERC_VERSION.to_owned(),
            firmware: self.version().firmware.to_string(),
            db_version: self.open_db().ok().map(|db| db.db_version()),
        };

        let device_path = dir.join("device.json");
        let device = match read_json::<DeviceRecord>(&device_path) {
            Ok(Some(d)) => DeviceRecord {
                last_write: stamp,
                ..d
            },
            other => {
                if other.is_err() {
                    set_aside(&device_path);
                }
                DeviceRecord {
                    format: FORMAT,
                    device_id: uuid::Uuid::new_v4().to_string(),
                    serial: self.version().serial.clone(),
                    first_write: stamp.clone(),
                    last_write: stamp,
                }
            }
        };
        write_atomic(&device_path, &serde_json::to_vec_pretty(&device)?)?;

        if !record.books.is_empty() {
            let books_path = dir.join("books.json");
            let mut books: BTreeMap<String, ManagedBook> =
                if let Ok(f) = read_json::<BooksFile>(&books_path) {
                    f.map(|f| f.books).unwrap_or_default()
                } else {
                    set_aside(&books_path);
                    Vec::new()
                }
                .into_iter()
                .map(|b| (b.content_id.clone(), b))
                .collect();
            for change in record.books {
                apply_change(&mut books, change);
            }
            let file = BooksFile {
                format: FORMAT,
                books: books.into_values().collect(),
            };
            write_atomic(&books_path, &serde_json::to_vec_pretty(&file)?)?;
        }

        if !record.entries.is_empty() {
            let log_path = dir.join("log.jsonl");
            let (mut log, _) = read_log(&log_path);
            log.extend(record.entries);
            let skip = log.len().saturating_sub(LOG_KEPT);
            let mut out = Vec::new();
            for entry in &log[skip..] {
                serde_json::to_writer(&mut out, entry)?;
                out.push(b'\n');
            }
            write_atomic(&log_path, &out)?;
        }
        Ok(())
    }
}

fn apply_change(books: &mut BTreeMap<String, ManagedBook>, change: BookChange) {
    match change {
        BookChange::Removed { content_id } => {
            books.remove(&content_id);
        }
        BookChange::Written {
            path,
            sha256,
            size,
            library_book,
        } => {
            let content_id = format!("{ONBOARD_PREFIX}{path}");
            let at = now();
            let previous = books.remove(&content_id);
            books.insert(
                content_id.clone(),
                ManagedBook {
                    library_book: library_book
                        .or_else(|| previous.as_ref().and_then(|p| p.library_book)),
                    first_written: previous
                        .as_ref()
                        .map_or_else(|| at.clone(), |p| p.first_written.clone()),
                    writes: previous.as_ref().map_or(0, |p| p.writes) + 1,
                    content_id,
                    path,
                    sha256,
                    size,
                    last_written: at,
                    erc_version: ERC_VERSION.to_owned(),
                },
            );
        }
    }
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// `Ok(None)` when the file doesn't exist; `Err` when it can't be parsed.
fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    match fs::read(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| e.to_string()),
    }
}

/// Entries that parse, and how many lines didn't.
fn read_log(path: &Path) -> (Vec<LogEntry>, usize) {
    let Ok(text) = fs::read_to_string(path) else {
        return (Vec::new(), 0);
    };
    let mut bad = 0;
    let entries = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|l| serde_json::from_str(l).inspect_err(|_| bad += 1).ok())
        .collect();
    (entries, bad)
}

/// Keeps an unreadable file for debugging instead of overwriting it.
fn set_aside(path: &Path) {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(
        ".unreadable-{}",
        Utc::now().format("%Y%m%d-%H%M%S")
    ));
    let aside = PathBuf::from(name);
    if fs::rename(path, &aside).is_ok() {
        crate::transfer::remove_apple_double(&aside);
    }
}

/// Writes to a temporary name, then renames, so a yanked cable leaves the old
/// file or the new one, never half of one.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = PathBuf::from(tmp);
    let _ = fs::remove_file(&tmp);
    {
        use std::io::Write;
        let mut file = fs::File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&tmp, path)?;
    crate::transfer::remove_apple_double(&tmp);
    crate::transfer::remove_apple_double(path);
    Ok(())
}

fn hash_file(path: &Path) -> std::io::Result<(String, u64)> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((hex, size))
}
