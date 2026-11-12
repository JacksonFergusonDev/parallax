//! Library-aware changes to books already on a device.

use std::{collections::BTreeMap, path::Path};

use parallax_device::{DeviceBook, ReadStatus};
use parallax_device_kobo::{
    AnnotationKind, BOOKS_DIR, DuplicateCollection, KoboDb, KoboDevice, Mismatch, ONBOARD_PREFIX,
    duplicate_collections, files_not_in_database, merge_duplicate_collections, missing_files,
    orphaned_images, remove_book_with_progress, set_read_status,
};
use parallax_library::{Format, Library};
use serde::Serialize;

use crate::comic_send::{ComicTarget, is_original_comic};

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
    // A send remains on the Kobo even if its library book has since been removed.
    let mut candidates: BTreeMap<_, _> = device
        .managed_books()
        .unwrap_or_default()
        .into_iter()
        .filter(|book| {
            Path::new(&book.path).parent() == Some(Path::new(BOOKS_DIR))
                && book.path.ends_with(".kepub.epub")
                && book.content_id == format!("{ONBOARD_PREFIX}{}", book.path)
        })
        .map(|book| (book.content_id, (book.path, book.last_written, None)))
        .collect();
    for copy in library.device_copies(&device.version().serial)? {
        candidates.insert(
            copy.content_id,
            (copy.device_path, copy.sent_at, Some(copy.book_id)),
        );
    }
    if candidates.is_empty() {
        return Ok(books);
    }
    let imported = db.sideloaded_ids()?;
    let memberships = db.memberships()?;
    for (content_id, (path, sent_at, book_id)) in candidates {
        if imported.contains(&content_id) {
            continue;
        }
        let Ok(file) = device.mount().join(&path).metadata() else {
            continue;
        };
        if !file.is_file() {
            continue;
        }
        let (title, author, series, series_number) = if let Some(id) = book_id {
            let local = library.book(id)?;
            (
                local.title,
                (!local.authors.is_empty()).then(|| local.authors.join(", ")),
                local.series,
                local.series_index,
            )
        } else {
            let name = Path::new(&path)
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            (
                name.strip_suffix(".kepub.epub").unwrap_or(&name).to_owned(),
                None,
                None,
                None,
            )
        };
        books.push(CatalogBook {
            pending_import: true,
            book: DeviceBook {
                collections: memberships.get(&content_id).cloned().unwrap_or_default(),
                id: content_id,
                title,
                author,
                path: Some(path),
                sideloaded: true,
                series,
                series_number,
                read_status: ReadStatus::Unread,
                percent_read: 0,
                last_read: None,
                reading_time_secs: None,
                added: parallax_device_kobo::parse_kobo_date(Some(sent_at)),
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
    /// A comic on the Kobo as its original CBZ (added from the Kobo), not
    /// a copy Parallax prepared. Updating it changes only its details.
    pub original_comic: bool,
}

pub fn books_on_device(
    library: &Library,
    device: &KoboDevice,
) -> parallax_library::Result<Vec<OnDevice>> {
    let copies = library.device_copies(&device.version().serial)?;
    let target = std::cell::OnceCell::new();
    Ok(copies
        .into_iter()
        .filter(|c| device.mount().join(&c.device_path).is_file())
        .filter_map(|c| {
            let book = library.book(c.book_id).ok()?;
            // A comic on the Kobo as its original CBZ only takes edited
            // details, so comic page settings don't make it out of date.
            let original_comic = is_original_comic(&book, &c.device_path);
            Some(OnDevice {
                stale: book.modified_at > c.sent_at
                    || (book.format == Format::Cbz
                        && !original_comic
                        && target
                            .get_or_init(|| ComicTarget::new(library, device))
                            .as_ref()
                            .is_ok_and(|target| {
                                c.comic_profile.as_deref() != Some(&target.fingerprint(&book))
                            })),
                original_comic,
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
) -> parallax_device_kobo::Result<Vec<RemoveOutcome>> {
    remove_from_device_with_progress(library, device, backup_root, content_ids, |_, _, _| {})
}

/// Reports completed removals, batch size and the fraction of the current book saved.
pub fn remove_from_device_with_progress(
    library: Option<&Library>,
    device: &KoboDevice,
    backup_root: &Path,
    content_ids: &[String],
    mut progress: impl FnMut(usize, usize, f64),
) -> parallax_device_kobo::Result<Vec<RemoveOutcome>> {
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
pub fn sync_reading(library: &Library, device: &KoboDevice) -> parallax_device_kobo::Result<usize> {
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

pub fn health(device: &KoboDevice) -> parallax_device_kobo::Result<Health> {
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
) -> parallax_device_kobo::Result<usize> {
    if duplicate_collections(device.open_db()?.connection())?.is_empty() {
        return Ok(0);
    }
    let mut writer = device.open_writer(backup_root)?;
    let removed = writer.transaction(merge_duplicate_collections);
    if let Ok(n) = removed {
        writer.log(
            parallax_device_kobo::LogEntry::new("collection")
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
) -> parallax_device_kobo::Result<usize> {
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
) -> parallax_device_kobo::Result<usize> {
    let serial = device.version().serial.clone();
    let copies = library.device_copies(&serial).unwrap_or_default();
    if copies.is_empty() {
        return Ok(0);
    }
    let all = device.open_db()?.annotations(None)?;
    let mut changed = 0;
    for copy in &copies {
        let items: Vec<parallax_library::Annotation> = all
            .iter()
            .filter(|a| a.book == copy.content_id)
            .map(|a| parallax_library::Annotation {
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
