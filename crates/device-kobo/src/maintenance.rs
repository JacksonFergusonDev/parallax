//! Housekeeping ported from Kobo Utilities (janlarres/kobo-utilities,
//! GPLv3): reading status, leftover cover images, duplicate collections, and
//! books the database and the files disagree about.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::Utc;
use erc_device::ReadStatus;
use rusqlite::{Transaction, params};
use serde::Serialize;

use crate::KoboDevice;
use crate::db::{ONBOARD_PREFIX, columns};
use crate::error::Result;
use crate::state::{LogEntry, Record};
use crate::writer::DeviceWriter;

/// Sets books' reading status (features/metadata.py `do_update_metadata`).
/// `reset_position` also clears where the book was open and its percentage,
/// as Nickel's own "Mark as unread" does; `reset_time` clears the reading
/// time Nickel shows. Returns how many books changed.
pub fn set_read_status(
    writer: &mut DeviceWriter,
    content_ids: &[String],
    status: ReadStatus,
    reset_position: bool,
    reset_time: bool,
) -> Result<usize> {
    let code = match status {
        ReadStatus::Unread => 0,
        ReadStatus::Reading => 1,
        ReadStatus::Finished => 2,
    };
    let changed = writer.transaction(|tx| {
        // Older schemas lack some of these; reset what this one has.
        let cols = columns(tx, "content")?;
        let present = |wanted: &[(&'static str, &'static str)]| -> Vec<String> {
            wanted
                .iter()
                .filter(|(c, _)| cols.contains(&c.to_ascii_lowercase()))
                .map(|(c, v)| format!("{c} = {v}"))
                .collect()
        };
        let position = present(&[
            ("DateLastRead", "NULL"),
            ("ChapterIDBookmarked", "NULL"),
            ("___PercentRead", "0"),
            (
                "FirstTimeReading",
                if code < 2 { "'true'" } else { "'false'" },
            ),
        ]);
        let time = present(&[("TimeSpentReading", "0"), ("RestOfBookEstimate", "0")]);
        let mut n = 0;
        for id in content_ids {
            n += tx.execute(
                "UPDATE content SET ReadStatus = ?2 WHERE ContentID = ?1 AND BookID IS NULL",
                params![id, code],
            )?;
            for (wanted, set) in [(reset_position, &position), (reset_time, &time)] {
                if wanted && !set.is_empty() {
                    tx.execute(
                        &format!(
                            "UPDATE content SET {} WHERE ContentID = ?1 AND BookID IS NULL",
                            set.join(", ")
                        ),
                        [id],
                    )?;
                }
            }
        }
        Ok(n)
    })?;
    let label = match status {
        ReadStatus::Unread => "unread",
        ReadStatus::Reading => "reading",
        ReadStatus::Finished => "finished",
    };
    let mut detail = format!("marked {label}");
    if reset_position {
        detail.push_str(", position reset");
    }
    if reset_time {
        detail.push_str(", reading time reset");
    }
    writer.log(
        LogEntry::new("reading status")
            .ids(content_ids.iter().cloned())
            .detail(detail),
    );
    Ok(changed)
}

/// Collections that exist more than once under the same name.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DuplicateCollection {
    pub name: String,
    pub copies: usize,
}

pub fn duplicate_collections(tx: &rusqlite::Connection) -> Result<Vec<DuplicateCollection>> {
    let mut stmt = tx.prepare(
        "SELECT Name, COUNT(*) FROM Shelf WHERE _IsDeleted = 'false'
         GROUP BY Name HAVING COUNT(*) > 1 ORDER BY Name COLLATE NOCASE",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(DuplicateCollection {
                name: r.get(0)?,
                copies: r.get::<_, i64>(1)? as usize,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

/// Keeps the oldest copy of each duplicated collection and removes the rest
/// (features/duplicateshelves.py): copies Kobo's cloud knows about are marked
/// deleted so the deletion syncs; local-only copies are deleted outright.
/// Memberships refer to collections by name, so no book leaves a collection.
/// Returns how many copies were removed.
pub fn merge_duplicate_collections(tx: &Transaction) -> Result<usize> {
    let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut removed = 0;
    for dup in duplicate_collections(tx)? {
        let keep: i64 = tx.query_row(
            "SELECT rowid FROM Shelf WHERE Name = ?1 AND _IsDeleted = 'false'
             ORDER BY CreationDate, rowid LIMIT 1",
            [&dup.name],
            |r| r.get(0),
        )?;
        removed += tx.execute(
            "UPDATE Shelf SET _IsDeleted = 'true', LastModified = ?3
             WHERE Name = ?1 AND rowid <> ?2 AND _IsDeleted = 'false' AND _IsSynced = 'true'",
            params![dup.name, keep, now],
        )?;
        removed += tx.execute(
            "DELETE FROM Shelf
             WHERE Name = ?1 AND rowid <> ?2 AND _IsDeleted = 'false' AND _IsSynced = 'false'",
            params![dup.name, keep],
        )?;
    }
    Ok(removed)
}

/// Where Nickel caches covers (firmware 4.x layout).
const IMAGES_DIR: &str = ".kobo-images";
/// Parts of a cached cover's name that follow its ImageId.
const IMAGE_MARKERS: [&str; 2] = [" - N3_", " - AndroidBookLoadTablet_Aspect"];

/// Cached cover images whose book is no longer in the database
/// (features/cleanimages.py). Nickel recreates any cover it needs, so these
/// are safe to delete. Empty when the database lists no images at all, which
/// would only happen if reading it went wrong.
pub fn orphaned_images(device: &KoboDevice) -> Result<Vec<PathBuf>> {
    let db = device.open_db()?;
    let known: HashSet<String> = {
        let mut stmt = db.connection().prepare(
            "SELECT DISTINCT ImageId FROM content WHERE ContentType IN (6, 901) AND ImageId IS NOT NULL",
        )?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    if known.is_empty() {
        return Ok(Vec::new());
    }
    let mut orphans = Vec::new();
    walk_files(&device.mount().join(IMAGES_DIR), &mut |path| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let id = IMAGE_MARKERS
            .iter()
            .find_map(|m| name.split_once(m).map(|(id, _)| id));
        if let Some(id) = id
            && !known.contains(id)
        {
            orphans.push(path.to_path_buf());
        }
    });
    orphans.sort();
    Ok(orphans)
}

/// Deletes orphaned images found by [`orphaned_images`] (checked again, so a
/// stale list can't delete a cover still in use) and the folders they leave
/// empty. Returns (files removed, bytes freed).
pub fn remove_orphaned_images(device: &KoboDevice) -> Result<(usize, u64)> {
    device.check_writable()?;
    let images = device.mount().join(IMAGES_DIR);
    let (mut files, mut bytes) = (0, 0);
    for path in orphaned_images(device)? {
        let size = fs::metadata(&path).map_or(0, |m| m.len());
        if fs::remove_file(&path).is_ok() {
            crate::transfer::remove_apple_double(&path);
            files += 1;
            bytes += size;
            let mut dir = path.parent();
            while let Some(d) = dir.filter(|d| *d != images && d.starts_with(&images)) {
                if fs::remove_dir(d).is_err() {
                    break;
                }
                crate::transfer::remove_apple_double(d);
                dir = d.parent();
            }
        }
    }
    if files > 0 {
        device.record(Record {
            entries: vec![LogEntry::new("clean images").detail(format!(
                "removed {files} cached covers of books no longer on the Kobo ({} KB)",
                bytes / 1024
            ))],
            books: Vec::new(),
        });
    }
    Ok((files, bytes))
}

/// A database entry whose file is gone, or a file the database doesn't list.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Mismatch {
    /// Relative to the mount point.
    pub path: String,
    pub content_id: String,
    /// From the database, for entries; the file name for files.
    pub title: String,
}

/// File types Nickel imports.
const BOOK_EXTENSIONS: [&str; 9] = [
    "epub", "kepub", "pdf", "cbz", "cbr", "mobi", "txt", "html", "rtf",
];

/// Sideloaded entries whose file no longer exists ("ghosts" that show as
/// books that won't open).
pub fn missing_files(device: &KoboDevice) -> Result<Vec<Mismatch>> {
    let db = device.open_db()?;
    let mut stmt = db.connection().prepare(
        "SELECT ContentID, coalesce(Title, '') FROM content
         WHERE ContentType = 6 AND BookID IS NULL AND ContentID LIKE 'file:///mnt/onboard/%'",
    )?;
    let rows: Vec<(String, String)> = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows
        .into_iter()
        .filter_map(|(content_id, title)| {
            let path = content_id.strip_prefix(ONBOARD_PREFIX)?.to_owned();
            (!device.mount().join(&path).exists()).then_some(Mismatch {
                path,
                content_id,
                title,
            })
        })
        .collect())
}

/// Book files the database doesn't list (features/booksnotindb.py): either
/// sent since the last eject, so Nickel hasn't imported them yet, or files
/// Nickel couldn't import.
pub fn files_not_in_database(device: &KoboDevice) -> Result<Vec<Mismatch>> {
    let db = device.open_db()?;
    let known: HashSet<String> = {
        let mut stmt = db
            .connection()
            .prepare("SELECT ContentID FROM content WHERE BookID IS NULL")?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?
    };
    let mount = device.mount();
    let mut out = Vec::new();
    walk_books(mount, &mut |path| {
        let rel = path
            .strip_prefix(mount)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let content_id = format!("{ONBOARD_PREFIX}{rel}");
        if !known.contains(&content_id) {
            out.push(Mismatch {
                title: path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
                path: rel,
                content_id,
            });
        }
    });
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Every file under `dir`.
fn walk_files(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_files(&path, f),
            Ok(t) if t.is_file() => f(&path),
            _ => {}
        }
    }
}

/// Book files outside hidden folders, as Nickel scans them.
fn walk_books(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_books(&path, f),
            Ok(t) if t.is_file() => {
                let ext = name.rsplit('.').next().unwrap_or_default().to_lowercase();
                if BOOK_EXTENSIONS.contains(&ext.as_str()) {
                    f(&path);
                }
            }
            _ => {}
        }
    }
}
