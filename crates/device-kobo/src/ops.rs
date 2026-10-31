//! Changes to books on the device. Database changes go through a
//! [`DeviceWriter`]; file changes happen after the database commit, so a
//! failure can at worst leave a file Nickel will simply re-import.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, params};

use crate::KoboDevice;
use crate::db::ONBOARD_PREFIX;
use crate::error::{Error, Result};
use crate::state::{BookChange, LogEntry, Record};
use crate::writer::DeviceWriter;

/// Metadata Nickel shows for a book, written straight into `content` when a
/// book is updated in place (Nickel doesn't re-read an imported book's OPF).
#[derive(Debug, Clone, Default)]
pub struct DisplayMetadata {
    pub title: String,
    pub author: Option<String>,
    pub series: Option<String>,
    pub series_number: Option<String>,
}

/// calibre's `qhash`, which Nickel uses to spread cover thumbnails over
/// `.kobo-images/<h & 0xff>/<(h >> 8) & 0xff>/`.
pub fn qhash(s: &str) -> u32 {
    let mut h: u32 = 0;
    for &b in s.as_bytes() {
        h = (h << 4).wrapping_add(u32::from(b));
        h ^= (h & 0xF000_0000) >> 23;
        h &= 0x0FFF_FFFF;
    }
    h
}

/// calibre's `imageid_from_contentid` for Kobo (non-Tolino) devices.
pub fn image_id_for(content_id: &str) -> String {
    content_id.replace(['/', ' ', ':', '.'], "_")
}

impl KoboDevice {
    fn cover_cache_dir(&self, image_id: &str) -> PathBuf {
        let h = qhash(image_id);
        self.mount()
            .join(".kobo-images")
            .join((h & 0xFF).to_string())
            .join(((h >> 8) & 0xFF).to_string())
    }

    /// Deletes Nickel's cached thumbnails for a book (`<ImageId> - N3_*.parsed`)
    /// so it regenerates them from the book. Returns how many were removed.
    pub fn clear_cover_cache(&self, image_id: &str) -> usize {
        let dir = self.cover_cache_dir(image_id);
        let prefix = format!("{image_id} - ");
        let Ok(entries) = fs::read_dir(&dir) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with(&prefix) && fs::remove_file(entry.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }

    fn path_for(&self, content_id: &str) -> Option<PathBuf> {
        content_id
            .strip_prefix(ONBOARD_PREFIX)
            .map(|rel| self.mount().join(rel))
    }
}

/// Removes a sideloaded book: its database rows (calibre KoboTouch
/// `delete_via_sql`), then its file, cached covers and now-empty folder.
/// Works for books Nickel hasn't imported yet too.
pub fn remove_book(writer: &mut DeviceWriter, content_id: &str) -> Result<()> {
    let device = writer.device();
    let path = device.path_for(content_id).ok_or_else(|| Error::Io {
        path: PathBuf::from(content_id),
        source: std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "only sideloaded books can be removed",
        ),
    })?;
    let image_id = writer.transaction(|tx| {
        let image_id: Option<String> = tx
            .query_row(
                "SELECT ImageId FROM content WHERE ContentID = ?1 AND BookID IS NULL",
                [content_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let has_table = |name: &str| -> rusqlite::Result<bool> {
            tx.query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                [name],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n > 0)
        };
        // calibre KOBO.delete_via_sql then KOBOTOUCH.delete_via_sql.
        for (table, column) in [
            ("volume_shortcovers", "volumeid"),
            ("content_keys", "volumeid"),
            ("Bookmark", "VolumeID"),
            ("ShelfContent", "ContentId"),
            ("content_settings", "ContentID"),
            ("ratings", "ContentID"),
            ("Activity", "Id"),
        ] {
            if has_table(table)? {
                tx.execute(
                    &format!("DELETE FROM {table} WHERE {column} = ?1"),
                    [content_id],
                )?;
            }
        }
        tx.execute("DELETE FROM content WHERE BookID = ?1", [content_id])?;
        tx.execute(
            "DELETE FROM content WHERE BookID IS NULL AND ContentID = ?1",
            [content_id],
        )?;
        Ok(image_id)
    })?;

    if path.exists() {
        fs::remove_file(&path).map_err(|e| Error::Io {
            path: path.clone(),
            source: e,
        })?;
    }
    crate::transfer::remove_apple_double(&path);
    device.clear_cover_cache(&image_id.unwrap_or_else(|| image_id_for(content_id)));
    remove_empty_parents(device.mount(), &path);
    writer.track(BookChange::Removed {
        content_id: content_id.to_owned(),
    });
    writer.log(LogEntry::new("remove").ids([content_id]));
    Ok(())
}

/// Replaces a book's file in place with `src` and updates what Nickel shows
/// (title, author, series) without losing reading progress, which is keyed
/// to the unchanged path. Cached covers are cleared so a new cover appears.
pub fn replace_book(
    writer: &mut DeviceWriter,
    src: &Path,
    device_path: &str,
    meta: &DisplayMetadata,
) -> Result<()> {
    let content_id = format!("{ONBOARD_PREFIX}{device_path}");
    writer.device().replace_file(src, device_path)?;
    let image_id = writer.transaction(|tx| {
        tx.execute(
            "UPDATE content SET Title = ?2, Attribution = ?3 WHERE ContentID = ?1 AND BookID IS NULL",
            params![content_id, meta.title, meta.author],
        )?;
        set_series(tx, &content_id, meta.series.as_deref(), meta.series_number.as_deref())?;
        Ok(tx
            .query_row("SELECT ImageId FROM content WHERE ContentID = ?1 AND BookID IS NULL", [&content_id], |r| r.get(0))
            .optional()?
            .flatten())
    })?;
    writer
        .device()
        .clear_cover_cache(&image_id.unwrap_or_else(|| image_id_for(&content_id)));
    if let Some(change) = BookChange::written(device_path, src, None) {
        writer.track(change);
    }
    writer.log(
        LogEntry::new("replace")
            .ids([content_id])
            .detail(&meta.title),
    );
    Ok(())
}

/// Sets (or clears) the series Nickel shows for an imported book. SeriesID
/// reuses a store book's id for the same series if there is one, else the
/// series name, as kepubify's seriesmeta does for sideloads (otherwise one
/// series shows up twice).
pub fn set_series(
    tx: &rusqlite::Transaction,
    content_id: &str,
    series: Option<&str>,
    number: Option<&str>,
) -> Result<bool> {
    let number = series.and(number);
    let changed = tx.execute(
        "UPDATE content SET Series = ?2, SeriesNumber = ?3,
             SeriesNumberFloat = CAST(?3 AS REAL),
             SeriesID = CASE WHEN ?2 IS NULL THEN NULL ELSE coalesce(
                 (SELECT c2.SeriesID FROM content c2 WHERE c2.Series = ?2 AND c2.WorkId IS NOT NULL
                  AND c2.WorkId != '' AND c2.SeriesID IS NOT NULL AND c2.SeriesID != '' LIMIT 1), ?2) END
         WHERE ContentID = ?1 AND BookID IS NULL",
        params![content_id, series, number],
    )?;
    Ok(changed > 0)
}

/// Folders macOS and Kobo use for their own bookkeeping; never touched.
const SKIP_DIRS: [&str; 6] = [
    ".kobo",
    ".kobo-images",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    "System Volume Information",
];
/// First bytes of an AppleDouble file.
const APPLE_DOUBLE_MAGIC: [u8; 4] = [0x00, 0x05, 0x16, 0x07];

/// Deletes macOS `._*` AppleDouble files from the device's library folders.
/// Only files that really are AppleDouble (by magic number) are removed.
pub fn tidy_apple_double(device: &KoboDevice) -> usize {
    fn walk(dir: &Path, removed: &mut usize) {
        let Ok(entries) = fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let path = entry.path();
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                if !SKIP_DIRS.contains(&name.as_str()) {
                    walk(&path, removed);
                }
            } else if name.starts_with("._")
                && is_apple_double(&path)
                && fs::remove_file(&path).is_ok()
            {
                *removed += 1;
            }
        }
    }
    let mut removed = 0;
    walk(device.mount(), &mut removed);
    if removed > 0 {
        device.record(Record {
            entries: vec![LogEntry::new("tidy").detail(format!("removed {removed} ._ files"))],
            books: Vec::new(),
        });
    }
    removed
}

fn is_apple_double(path: &Path) -> bool {
    use std::io::Read;
    let mut magic = [0u8; 4];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == APPLE_DOUBLE_MAGIC
}

fn remove_empty_parents(mount: &Path, path: &Path) {
    let mut dir = path.parent();
    while let Some(d) = dir {
        if d == mount || !d.starts_with(mount) || fs::remove_dir(d).is_err() {
            break;
        }
        crate::transfer::remove_apple_double(d);
        dir = d.parent();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qhash_matches_nickel_layout() {
        // Observed on the reference Kobo: .kobo-images/178/169/<this> - N3_FULL.parsed
        let h = qhash("file____mnt_onboard_KEpub_Nonfiction_Endurance_-_Alfred_Lansing_kepub_epub");
        assert_eq!((h & 0xFF, (h >> 8) & 0xFF), (178, 169));
        let h =
            qhash("file____mnt_onboard_KEpub_Nonfiction_Underland_-_Robert_Macfarlane_kepub_epub");
        assert_eq!((h & 0xFF, (h >> 8) & 0xFF), (178, 43));
    }

    #[test]
    fn image_ids_follow_calibre() {
        assert_eq!(
            image_id_for("file:///mnt/onboard/KEpub/Dune - Frank Herbert.kepub.epub"),
            "file____mnt_onboard_KEpub_Dune_-_Frank_Herbert_kepub_epub"
        );
    }
}
