//! Library-aware changes to books already on a device.

use std::path::Path;

use erc_device::{DeviceBook, ReadStatus};
use erc_device_kobo::{
    AnnotationKind, DuplicateCollection, KoboDb, KoboDevice, Mismatch, duplicate_collections,
    files_not_in_database, merge_duplicate_collections, missing_files, orphaned_images,
    remove_book_with_progress, set_read_status,
};
use erc_library::{Format, Library};
use serde::Serialize;

use crate::comic_send::{ComicTarget, is_prepared_copy};

#[derive(Debug, Serialize)]
pub struct CatalogBook {
    #[serde(flatten)]
    pub book: DeviceBook,
    pub pending_import: bool,
}

/// Books actually on the Kobo, including known files Nickel has yet to import.
/// Database rows without a file are left to the maintenance view.
pub fn device_catalog(
    library: &Library,
    device: &KoboDevice,
    db: &KoboDb,
) -> Result<Vec<CatalogBook>, Box<dyn std::error::Error>> {
    let mut books: Vec<_> = db
        .books()?
        .into_iter()
        .filter(|b| {
            b.path
                .as_ref()
                .is_none_or(|path| device.mount().join(path).is_file())
        })
        .map(|book| CatalogBook {
            book,
            pending_import: false,
        })
        .collect();
    let copies = library.device_copies(&device.version().serial)?;
    if copies.is_empty() {
        return Ok(books);
    }
    let imported = db.sideloaded_ids()?;
    let memberships = db.memberships()?;
    for copy in copies {
        if imported.contains(&copy.content_id) {
            continue;
        }
        let Ok(file) = device.mount().join(&copy.device_path).metadata() else {
            continue;
        };
        if !file.is_file() {
            continue;
        }
        let local = library.book(copy.book_id)?;
        books.push(CatalogBook {
            pending_import: true,
            book: DeviceBook {
                collections: memberships
                    .get(&copy.content_id)
                    .cloned()
                    .unwrap_or_default(),
                id: copy.content_id,
                title: local.title,
                author: (!local.authors.is_empty()).then(|| local.authors.join(", ")),
                path: Some(copy.device_path),
                sideloaded: true,
                series: local.series,
                series_number: local.series_index,
                read_status: ReadStatus::Unread,
                percent_read: 0,
                last_read: None,
                reading_time_secs: None,
                added: erc_device_kobo::parse_kobo_date(Some(copy.sent_at)),
                file_size: Some(file.len()),
            },
        });
    }
    Ok(books)
}

/// A library book whose copy is on the connected device.
#[derive(Debug, Clone, Serialize)]
pub struct OnDevice {
    pub book_id: i64,
    pub device_path: String,
    pub content_id: String,
    pub sent_at: String,
    /// The book was edited (metadata or cover) after it was sent.
    pub stale: bool,
}

pub fn books_on_device(
    library: &Library,
    device: &KoboDevice,
) -> erc_library::Result<Vec<OnDevice>> {
    let copies = library.device_copies(&device.version().serial)?;
    let target = std::cell::OnceCell::new();
    Ok(copies
        .into_iter()
        .filter(|c| device.mount().join(&c.device_path).is_file())
        .filter_map(|c| {
            let book = library.book(c.book_id).ok()?;
            // A comic copied to the Kobo as it is (added from the Kobo) can't
            // be updated in place, so it is never offered as out of date.
            let raw_comic = book.format == Format::Cbz && !is_prepared_copy(&c.device_path);
            Some(OnDevice {
                stale: !raw_comic
                    && (book.modified_at > c.sent_at
                        || (book.format == Format::Cbz
                            && target
                                .get_or_init(|| ComicTarget::new(library, device))
                                .as_ref()
                                .is_ok_and(|target| {
                                    c.comic_profile.as_deref() != Some(&target.fingerprint(&book))
                                }))),
                book_id: c.book_id,
                device_path: c.device_path,
                content_id: c.content_id,
                sent_at: c.sent_at,
            })
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveOutcome {
    pub content_id: String,
    pub book_id: Option<i64>,
    pub result: Result<(), String>,
}

/// Removes books from the device in one backed-up write session. The library
/// keeps the last known paths so a backup can make restored copies visible
/// again. Returns individual outcomes once the write session closes successfully.
pub fn remove_from_device(
    library: Option<&Library>,
    device: &KoboDevice,
    backup_root: &Path,
    content_ids: &[String],
) -> erc_device_kobo::Result<Vec<RemoveOutcome>> {
    remove_from_device_with_progress(library, device, backup_root, content_ids, |_, _, _| {})
}

/// Reports completed removals, batch size and the fraction of the current book saved.
pub fn remove_from_device_with_progress(
    library: Option<&Library>,
    device: &KoboDevice,
    backup_root: &Path,
    content_ids: &[String],
    mut progress: impl FnMut(usize, usize, f64),
) -> erc_device_kobo::Result<Vec<RemoveOutcome>> {
    let serial = device.version().serial.clone();
    let copies = library
        .and_then(|l| l.device_copies(&serial).ok())
        .unwrap_or_default();
    let mut writer = device.open_writer(backup_root)?;
    let total = content_ids.len();
    progress(0, total, 0.0);
    let outcomes = content_ids
        .iter()
        .enumerate()
        .map(|(index, content_id)| {
            let result = remove_book_with_progress(&mut writer, content_id, |bytes, file_size| {
                if file_size > 0 {
                    // Reserve completion for the removal after the copy is saved.
                    let fraction = (bytes as f64 / file_size as f64).min(0.99);
                    progress(index, total, fraction);
                }
            })
            .map_err(|e| e.to_string());
            progress(index + 1, total, 0.0);
            RemoveOutcome {
                content_id: content_id.clone(),
                book_id: copies
                    .iter()
                    .find(|c| &c.content_id == content_id)
                    .map(|c| c.book_id),
                result,
            }
        })
        .collect();
    writer.close()?;
    Ok(outcomes)
}

/// Copies the reading progress Nickel shows for library books on `device`
/// into the library, so it's visible with the Kobo unplugged. Only reads the
/// device. Returns how many books were updated.
pub fn sync_reading(library: &Library, device: &KoboDevice) -> erc_device_kobo::Result<usize> {
    let serial = device.version().serial.clone();
    let copies = library.device_copies(&serial).unwrap_or_default();
    if copies.is_empty() {
        return Ok(0);
    }
    let mut n = 0;
    for book in device.open_db()?.books()? {
        let Some(copy) = copies.iter().find(|c| c.content_id == book.id) else {
            continue;
        };
        let status = match book.read_status {
            ReadStatus::Unread => "unread",
            ReadStatus::Reading => "reading",
            ReadStatus::Finished => "finished",
        };
        let last_read = book.last_read.map(|d| d.to_rfc3339());
        let unchanged = library
            .reading(copy.book_id)
            .ok()
            .flatten()
            .is_some_and(|r| {
                r.device_serial == serial
                    && r.status == status
                    && r.percent == book.percent_read
                    && r.last_read == last_read
                    && r.time_spent_secs == book.reading_time_secs
            });
        if !unchanged
            && library
                .record_reading(
                    copy.book_id,
                    &serial,
                    status,
                    book.percent_read,
                    last_read.as_deref(),
                    book.reading_time_secs,
                )
                .is_ok()
        {
            n += 1;
        }
    }
    Ok(n)
}

/// What "Check this Kobo" found. Only reads the device.
#[derive(Debug, Clone, Serialize)]
pub struct Health {
    /// Entries whose file is gone: they show as books that won't open.
    pub missing_files: Vec<Mismatch>,
    /// Files not in the database: sent since the last eject, or files Nickel
    /// couldn't import.
    pub not_imported: Vec<Mismatch>,
    pub duplicate_collections: Vec<DuplicateCollection>,
    /// Cached covers of books no longer on the Kobo.
    pub orphaned_images: usize,
    pub orphaned_image_bytes: u64,
}

pub fn health(device: &KoboDevice) -> erc_device_kobo::Result<Health> {
    let images = orphaned_images(device)?;
    Ok(Health {
        missing_files: missing_files(device)?,
        not_imported: files_not_in_database(device)?,
        duplicate_collections: duplicate_collections(device.open_db()?.connection())?,
        orphaned_image_bytes: images
            .iter()
            .filter_map(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .sum(),
        orphaned_images: images.len(),
    })
}

/// Merges same-named collections in one backed-up write session. Returns
/// how many copies were removed.
pub fn merge_collections(
    device: &KoboDevice,
    backup_root: &Path,
) -> erc_device_kobo::Result<usize> {
    if duplicate_collections(device.open_db()?.connection())?.is_empty() {
        return Ok(0);
    }
    let mut writer = device.open_writer(backup_root)?;
    let removed = writer.transaction(merge_duplicate_collections);
    if let Ok(n) = removed {
        writer.log(
            erc_device_kobo::LogEntry::new("collection")
                .detail(format!("merged duplicate collections ({n} copies removed)")),
        );
    }
    writer.close()?;
    removed
}

/// Changes reading status in one backed-up write session, then refreshes the
/// library's copy of the progress.
pub fn change_reading_status(
    library: Option<&Library>,
    device: &KoboDevice,
    backup_root: &Path,
    content_ids: &[String],
    status: ReadStatus,
    reset_position: bool,
    reset_time: bool,
) -> erc_device_kobo::Result<usize> {
    let mut writer = device.open_writer(backup_root)?;
    let changed = set_read_status(&mut writer, content_ids, status, reset_position, reset_time);
    writer.close()?;
    if let Some(library) = library {
        sync_reading(library, device)?;
    }
    changed
}

/// Copies highlights, notes and bookmarks for library books on `device` into
/// the library, and backs up Adobe `.annot` files to
/// `<library>/annotation-files/<serial>/`. Only reads the device. Returns how
/// many annotations were new, changed or removed on the device.
pub fn sync_annotations(
    library: &mut Library,
    device: &KoboDevice,
) -> erc_device_kobo::Result<usize> {
    let serial = device.version().serial.clone();
    let copies = library.device_copies(&serial).unwrap_or_default();
    if copies.is_empty() {
        return Ok(0);
    }
    let all = device.open_db()?.annotations(None)?;
    let mut changed = 0;
    for copy in &copies {
        let items: Vec<erc_library::Annotation> = all
            .iter()
            .filter(|a| a.book == copy.content_id)
            .map(|a| erc_library::Annotation {
                id: a.id.clone(),
                device_serial: serial.clone(),
                kind: match a.kind {
                    AnnotationKind::Highlight => "highlight",
                    AnnotationKind::Note => "note",
                    AnnotationKind::Bookmark => "bookmark",
                    AnnotationKind::Markup => "markup",
                }
                .to_owned(),
                text: a.text.clone(),
                note: a.note.clone(),
                chapter: a.chapter.clone(),
                position: a.position,
                created: a.created.clone(),
                modified: a.modified.clone(),
                on_device: true,
            })
            .collect();
        changed += library
            .sync_annotations(copy.book_id, &serial, &items)
            .unwrap_or(0);
    }
    let backup = library
        .root()
        .join("annotation-files")
        .join(serial.replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
    for (rel, path) in device.annotation_files() {
        let dest = backup.join(&rel);
        let same = std::fs::read(&dest).ok() == std::fs::read(&path).ok();
        if !same {
            if let Some(parent) = dest.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::copy(&path, &dest);
        }
    }
    Ok(changed)
}
