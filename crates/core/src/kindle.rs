//! Library workflows for Kindles: listing what's on one, sending and
//! updating library books as AZW3, removing them, adding books from the
//! Kindle to the library, and copying reading progress and highlights back.
//!
//! A Kindle's own records (collections, read status) are out of reach, so a
//! book is identified by its path on the Kindle, which is also its
//! `content_id` in the library's device copies.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, NaiveDateTime, Utc};
use parallax_convert::Converter;
use parallax_device::{DeviceBook, ReadStatus};
use parallax_device_kindle::books::{self, KindleBook, MetaCache};
use parallax_device_kindle::transfer::file_name;
use parallax_device_kindle::{Clipping, KindleDevice, Outgoing, clippings, text};
use parallax_library::{Book, Format, ImportOutcome, Library, Reading};

use crate::device::{CatalogBook, OnDevice, RemoveOutcome};
use crate::import::{DeviceImport, DeviceImportOutcome};
use crate::send::{Progress, SendOutcome, SentInfo, Stage, copy_progress, run_batch};

/// Page numbers (APNX) for sent books; on unless "off".
pub const KINDLE_PAGE_NUMBERS: &str = "kindle_page_numbers";
/// Stop macOS's photo import service when it holds an MTP Kindle; on unless
/// "off".
pub const KINDLE_FREE_FROM_PHOTOS: &str = "kindle_free_from_photos";

/// Percent read at which a Kindle book counts as finished. A Kindle's own
/// "read" flag lives in its database, out of reach, so this is an estimate.
const FINISHED_PERCENT: u8 = 99;

fn is_store_book(book: &KindleBook) -> bool {
    book.meta.drm || book.path.contains("-asin_")
}

/// How a Kindle book looks in the device view.
pub fn device_book(book: &KindleBook) -> DeviceBook {
    let percent = book.percent_read();
    let started = book.reading.last.is_some() || book.reading.furthest.is_some();
    let read_status = match percent {
        Some(p) if p >= FINISHED_PERCENT => ReadStatus::Finished,
        Some(p) if p > 0 || started => ReadStatus::Reading,
        _ if started => ReadStatus::Reading,
        _ => ReadStatus::Unread,
    };
    DeviceBook {
        id: book.path.clone(),
        title: book.title(),
        author: (!book.meta.authors.is_empty()).then(|| book.meta.authors.join(", ")),
        path: Some(book.path.clone()),
        sideloaded: !is_store_book(book),
        series: None,
        series_number: None,
        read_status,
        percent_read: percent.unwrap_or(0),
        last_read: book
            .last_read_ms()
            .and_then(DateTime::<Utc>::from_timestamp_millis),
        reading_time_secs: book
            .reading
            .total_time_ms
            .and_then(|ms| u64::try_from(ms / 1000).ok()),
        added: book
            .meta
            .modified
            .and_then(|s| DateTime::<Utc>::from_timestamp(s, 0)),
        file_size: Some(book.meta.size),
        collections: book.collections.clone(),
    }
}

/// Every book on the Kindle, as the device view lists them.
pub fn catalog(books: &[KindleBook]) -> Vec<CatalogBook> {
    books
        .iter()
        .map(|b| CatalogBook {
            book: device_book(b),
            pending_import: false,
        })
        .collect()
}

/// Library books whose copy is on the Kindle.
pub fn books_on_device(
    library: &Library,
    device: &KindleDevice,
    books: &[KindleBook],
) -> parallax_library::Result<Vec<OnDevice>> {
    let present: std::collections::HashSet<&str> = books.iter().map(|b| b.path.as_str()).collect();
    Ok(library
        .device_copies(device.serial())?
        .into_iter()
        .filter(|c| present.contains(c.device_path.as_str()))
        .filter_map(|c| {
            let book = library.book(c.book_id).ok()?;
            Some(OnDevice {
                stale: book.modified_at > c.sent_at,
                original_comic: false,
                book_id: c.book_id,
                device_path: c.device_path,
                content_id: c.content_id,
                sent_at: c.sent_at,
            })
        })
        .collect())
}

/// What sending needs from the app.
pub struct SendOptions<'a> {
    /// EPUB to AZW3, normally `ChildProcess::to_azw3`.
    pub converter: &'a dyn Converter,
    pub page_numbers: bool,
}

fn not_for_kindle(book: &Book) -> Option<String> {
    (book.format == Format::Cbz)
        .then(|| "comics can't be sent to a Kindle yet; send them to a Kobo instead".to_owned())
}

/// Exports a library book with its current details, converts it to AZW3 and
/// writes it to the Kindle at `file`.
fn deliver(
    library: &Library,
    device: &KindleDevice,
    options: &SendOptions,
    book: &Book,
    file: &str,
    stage: &mut dyn FnMut(Stage<'_>),
) -> Result<SentInfo, String> {
    stage(&|index, total, file| Progress::Preparing { index, total, file });
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let epub = work.path().join("book.epub");
    let azw3 = work.path().join("book.azw3");
    library.export(book.id, &epub).map_err(|e| e.to_string())?;
    stage(&|index, total, file| Progress::Converting { index, total, file });
    options
        .converter
        .convert(&epub, &azw3)
        .map_err(|e| e.to_string())?;
    let data = std::fs::read(&azw3).map_err(|e| e.to_string())?;
    let cover = library.cover_source(book.id).ok().flatten();
    let mut progress = copy_progress(stage);
    let sent = device
        .send(
            &Outgoing {
                file_name: file,
                data: &data,
                cover: cover.as_deref(),
                page_numbers: options.page_numbers,
            },
            &mut progress,
        )
        .map_err(|e| e.to_string())?;
    Ok(SentInfo {
        content_id: sent.path.clone(),
        device_path: sent.path,
        bytes: data.len() as u64,
        skipped_pages: Vec::new(),
        details_only: false,
    })
}

fn labels(library: &Library, ids: &[i64]) -> (Vec<Result<Book, String>>, Vec<String>) {
    let books: Vec<Result<Book, String>> = ids
        .iter()
        .map(|&id| library.book(id).map_err(|e| e.to_string()))
        .collect();
    let labels = books
        .iter()
        .zip(ids)
        .map(|(b, id)| {
            b.as_ref()
                .map_or_else(|_| format!("Book {id}"), |b| b.title.clone())
        })
        .collect();
    (books, labels)
}

/// Sends library books to the Kindle as AZW3 and records each copy.
pub fn send_library_books(
    library: &Library,
    device: &KindleDevice,
    options: &SendOptions,
    ids: &[i64],
    on_progress: impl FnMut(Progress),
) -> Vec<SendOutcome> {
    let (books, labels) = labels(library, ids);
    let serial = device.serial().to_owned();
    let existing = library.device_copies(&serial).unwrap_or_default();
    run_batch(&labels, on_progress, |i, stage| {
        let id = ids[i];
        let result = books[i].clone().and_then(|book| {
            if let Some(reason) = not_for_kindle(&book) {
                return Err(reason);
            }
            let storage = device.storage();
            if let Some(copy) = existing
                .iter()
                .find(|c| c.book_id == id && storage.size(&c.device_path).ok().flatten().is_some())
            {
                return Err(format!("already on the Kindle as {}", copy.device_path));
            }
            let name = file_name(&book.title, &book.authors, "azw3");
            let path = format!("{}/{name}", parallax_device_kindle::DOCUMENTS_DIR);
            if storage.size(&path).map_err(|e| e.to_string())?.is_some() {
                return Err(format!("{path} is already on the Kindle"));
            }
            let sent = deliver(library, device, options, &book, &name, stage)?;
            library
                .record_device_copy(id, &serial, &sent.device_path, &sent.content_id)
                .map_err(|e| e.to_string())?;
            Ok(sent)
        });
        SendOutcome {
            source: labels[i].clone(),
            book_id: Some(id),
            result,
        }
    })
}

/// Sends library books again over their copies on the Kindle, keeping each
/// file name so the Kindle keeps its place and highlights.
pub fn update_on_device(
    library: &Library,
    device: &KindleDevice,
    options: &SendOptions,
    ids: &[i64],
    on_progress: impl FnMut(Progress),
) -> Vec<SendOutcome> {
    let (books, labels) = labels(library, ids);
    let serial = device.serial().to_owned();
    let copies = library.device_copies(&serial).unwrap_or_default();
    run_batch(&labels, on_progress, |i, stage| {
        let id = ids[i];
        let result = books[i].clone().and_then(|book| {
            if let Some(reason) = not_for_kindle(&book) {
                return Err(reason);
            }
            let copy = copies
                .iter()
                .find(|c| c.book_id == id)
                .ok_or("this book isn't on the Kindle")?;
            let (_, name) = parallax_device_kindle::storage::split(&copy.device_path);
            if books::extension(name) != "azw3" {
                return Err(format!(
                    "{} isn't an AZW3 Parallax sent; remove it and send the book again",
                    copy.device_path
                ));
            }
            let sent = deliver(library, device, options, &book, name, stage)?;
            library
                .record_device_copy(id, &serial, &sent.device_path, &sent.content_id)
                .map_err(|e| e.to_string())?;
            Ok(sent)
        });
        SendOutcome {
            source: labels[i].clone(),
            book_id: Some(id),
            result,
        }
    })
}

/// Removes books (by path) from the Kindle and forgets their library copies.
pub fn remove_from_device(
    library: Option<&Library>,
    device: &KindleDevice,
    paths: &[String],
    mut progress: impl FnMut(usize, usize),
) -> Vec<RemoveOutcome> {
    let copies = library
        .and_then(|l| l.device_copies(device.serial()).ok())
        .unwrap_or_default();
    let total = paths.len();
    progress(0, total);
    paths
        .iter()
        .enumerate()
        .map(|(i, path)| {
            let book_id = copies
                .iter()
                .find(|c| &c.device_path == path)
                .map(|c| c.book_id);
            let result = device.remove(path).map_err(|e| e.to_string());
            if result.is_ok()
                && let (Some(library), Some(id)) = (library, book_id)
            {
                let _ = library.remove_device_copy(id, device.serial());
            }
            progress(i + 1, total);
            RemoveOutcome {
                content_id: path.clone(),
                book_id,
                result,
            }
        })
        .collect()
}

fn skip(reason: &str) -> DeviceImport {
    DeviceImport::Skipped {
        reason: reason.to_owned(),
    }
}

/// Adds books from the Kindle (all when `paths` is `None`) to the library.
/// `converter` turns Amazon formats into EPUB. Only reads the Kindle.
pub fn import_from_device(
    library: &mut Library,
    device: &KindleDevice,
    books: &[KindleBook],
    paths: Option<&[String]>,
    converter: &dyn Converter,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> Vec<DeviceImportOutcome> {
    let chosen: Vec<&KindleBook> = books
        .iter()
        .filter(|b| paths.is_none_or(|p| p.contains(&b.path)))
        .collect();
    let total = chosen.len();
    if total > 0 {
        let _ = library.snapshot("kindle-import", Some(&format!("{total} books")));
    }
    chosen
        .into_iter()
        .enumerate()
        .map(|(i, book)| {
            let title = book.title();
            on_progress(i + 1, total, &title);
            DeviceImportOutcome {
                content_id: book.path.clone(),
                result: import_one(library, device, book, converter),
                title,
            }
        })
        .collect()
}

fn import_one(
    library: &mut Library,
    device: &KindleDevice,
    book: &KindleBook,
    converter: &dyn Converter,
) -> Result<DeviceImport, String> {
    let serial = device.serial();
    let copies = library.device_copies(serial).map_err(|e| e.to_string())?;
    if copies.iter().any(|c| c.content_id == book.path) {
        return Ok(skip("already in your library"));
    }
    if is_store_book(book) {
        return Ok(skip(
            "bought from the Kindle Store (DRM-protected; it stays in your Amazon account)",
        ));
    }
    let ext = book.extension();
    if !parallax_convert::amazon::EXTENSIONS.contains(&ext.as_str()) {
        return Ok(skip(&format!(
            "not a Kindle book Parallax can add (.{ext})"
        )));
    }
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let (_, name) = parallax_device_kindle::storage::split(&book.path);
    let local = work.path().join(name);
    let data = device.read(&book.path).map_err(|e| e.to_string())?;
    std::fs::write(&local, data).map_err(|e| e.to_string())?;
    let outcome =
        crate::ingest::import_file(library, converter, &local).map_err(|e| e.to_string())?;
    let (book_id, added) = match outcome {
        ImportOutcome::Added { book: added, .. } => (added.id, true),
        ImportOutcome::AlreadyInLibrary { book: existing } => (existing.id, false),
    };
    library
        .record_device_copy(book_id, serial, &book.path, &book.path)
        .map_err(|e| e.to_string())?;
    Ok(if added {
        DeviceImport::Added { book_id }
    } else {
        DeviceImport::Linked { book_id }
    })
}

/// Copies reading progress from the Kindle into the library for books sent
/// from it. Returns how many changed.
pub fn sync_reading(library: &Library, device: &KindleDevice, books: &[KindleBook]) -> usize {
    let serial = device.serial().to_owned();
    let copies = library.device_copies(&serial).unwrap_or_default();
    let by_path: HashMap<&str, &KindleBook> = books.iter().map(|b| (b.path.as_str(), b)).collect();
    let mut changed = 0;
    for copy in &copies {
        let Some(book) = by_path.get(copy.device_path.as_str()) else {
            continue;
        };
        let shown = device_book(book);
        let status = match shown.read_status {
            ReadStatus::Unread => "unread",
            ReadStatus::Reading => "reading",
            ReadStatus::Finished => "finished",
        };
        let last_read = shown.last_read.map(|d| d.to_rfc3339());
        let unchanged = library
            .reading(copy.book_id)
            .ok()
            .flatten()
            .is_some_and(|r| {
                r.device_serial == serial
                    && r.status == status
                    && r.percent == shown.percent_read
                    && r.last_read == last_read
                    && r.time_spent_secs == shown.reading_time_secs
            });
        if !unchanged
            && library
                .record_reading(
                    copy.book_id,
                    &Reading {
                        device_serial: serial.clone(),
                        status: status.to_owned(),
                        percent: shown.percent_read,
                        last_read,
                        time_spent_secs: shown.reading_time_secs,
                        chapter: None,
                        seen_at: String::new(),
                    },
                )
                .is_ok()
        {
            changed += 1;
        }
    }
    changed
}

/// "Monday, 3 November 2025 21:04:10" (and the US order) as RFC 3339.
fn clipping_date(added: &str) -> Option<String> {
    let formats = [
        "%A, %d %B %Y %H:%M:%S",
        "%A, %B %d, %Y %I:%M:%S %p",
        "%A, %B %d, %Y %H:%M:%S",
        "%A, %d %B %Y %I:%M:%S %p",
    ];
    formats
        .iter()
        .find_map(|f| NaiveDateTime::parse_from_str(added.trim(), f).ok())
        .map(|d| d.and_utc().to_rfc3339())
}

fn same_title(a: &str, b: &str) -> bool {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    !a.trim().is_empty() && norm(a) == norm(b)
}

fn annotation(serial: &str, clip: &Clipping) -> parallax_library::Annotation {
    let text = (!clip.text.is_empty()).then(|| clip.text.clone());
    let (text, note) = if clip.kind == "note" {
        (None, text)
    } else {
        (text, None)
    };
    parallax_library::Annotation {
        id: clippings::id(clip),
        device_serial: serial.to_owned(),
        kind: clip.kind.to_owned(),
        text,
        note,
        chapter: clip.page.as_ref().map(|p| format!("Page {p}")),
        position: f64::from(clip.location.unwrap_or(0)),
        created: clip.added.as_deref().and_then(clipping_date),
        modified: None,
        on_device: true,
    }
}

/// Highlights, notes and bookmarks the book's KRDS sidecar holds that
/// `My Clippings.txt` doesn't, such as ones made before the file was
/// cleared, or in a language whose clippings can't be told apart. KRDS
/// gives positions only, so a highlight's text is read from the book on the
/// Kindle, once: later syncs reuse the text the library already has.
fn krds_annotations(
    device: &KindleDevice,
    book: &KindleBook,
    serial: &str,
    clips: &[&Clipping],
    known: &[parallax_library::Annotation],
) -> Vec<parallax_library::Annotation> {
    let mut book_text: Option<Option<Vec<u8>>> = None;
    let mut out = Vec::new();
    for a in &book.reading.annotations {
        if !matches!(a.kind.as_str(), "highlight" | "note" | "bookmark") {
            continue;
        }
        // KFX positions aren't byte offsets; only MOBI and KF8 ones are.
        let (Ok(start), Ok(end)) = (a.start.trim().parse::<u64>(), a.end.trim().parse::<u64>())
        else {
            continue;
        };
        let location = text::location(start);
        let in_clippings = clips
            .iter()
            .any(|c| c.kind == a.kind && c.location.is_some_and(|l| l.abs_diff(location) <= 1));
        if in_clippings {
            continue;
        }
        let id = a.id();
        let text = if a.kind == "highlight" {
            known
                .iter()
                .find(|k| k.id == id && k.device_serial == serial)
                .and_then(|k| k.text.clone())
                .or_else(|| {
                    book_text
                        .get_or_insert_with(|| {
                            device.read(&book.path).ok().and_then(|b| text::extract(&b))
                        })
                        .as_deref()
                        .and_then(|t| text::passage(t, start, end))
                })
        } else {
            None
        };
        out.push(parallax_library::Annotation {
            id,
            device_serial: serial.to_owned(),
            kind: a.kind.clone(),
            text,
            note: a.note.clone(),
            chapter: None,
            position: f64::from(location),
            created: a
                .created_ms
                .and_then(DateTime::<Utc>::from_timestamp_millis)
                .map(|d| d.to_rfc3339()),
            modified: None,
            on_device: true,
        });
    }
    out
}

/// Copies highlights, notes and bookmarks into the library for books sent
/// from it: from `My Clippings.txt`, matching entries by title, then any
/// more the book's KRDS sidecar holds. Returns how many were new or changed.
pub fn sync_annotations(
    library: &mut Library,
    device: &KindleDevice,
    books: &[KindleBook],
) -> parallax_device_kindle::Result<usize> {
    let serial = device.serial().to_owned();
    let copies = library.device_copies(&serial).unwrap_or_default();
    if copies.is_empty() {
        return Ok(0);
    }
    let clips = device.clippings()?;
    let mut changed = 0;
    for copy in &copies {
        let Some(book) = books.iter().find(|b| b.path == copy.device_path) else {
            continue;
        };
        let titles = [
            Some(book.title()),
            library.book(copy.book_id).ok().map(|b| b.title),
        ];
        let book_clips: Vec<&Clipping> = clips
            .iter()
            .filter(|c| titles.iter().flatten().any(|t| same_title(t, &c.title)))
            .collect();
        let mut items: Vec<_> = book_clips.iter().map(|c| annotation(&serial, c)).collect();
        let known = library.annotations(copy.book_id).unwrap_or_default();
        items.extend(krds_annotations(device, book, &serial, &book_clips, &known));
        let mut seen = std::collections::HashSet::new();
        let items: Vec<_> = items
            .into_iter()
            .filter(|a| seen.insert(a.id.clone()))
            .collect();
        changed += library
            .sync_annotations(copy.book_id, &serial, &items)
            .unwrap_or(0);
    }
    Ok(changed)
}

/// What [`reconcile`] did when the Kindle connected.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Reconciled {
    pub thumbnails: usize,
    pub reading: usize,
    pub annotations: usize,
}

/// On connect: puts back covers Amazon replaced, then copies reading
/// progress and highlights into the library.
pub fn reconcile(
    library: &mut Library,
    device: &KindleDevice,
    cache: &mut MetaCache,
) -> parallax_device_kindle::Result<Reconciled> {
    let thumbnails = device.restore_thumbnails()?;
    let books = device.books(cache)?;
    Ok(Reconciled {
        thumbnails,
        reading: sync_reading(library, device, &books),
        annotations: sync_annotations(library, device, &books)?,
    })
}

/// A book file on the computer to send as it is (already AZW3, MOBI, PDF…).
pub fn send_files(
    device: &KindleDevice,
    sources: &[std::path::PathBuf],
    page_numbers: bool,
    on_progress: impl FnMut(Progress),
) -> Vec<SendOutcome> {
    let labels: Vec<String> = sources
        .iter()
        .map(|s| {
            s.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    run_batch(&labels, on_progress, |i, stage| {
        let source: &Path = &sources[i];
        let result = (|| {
            let ext = source
                .extension()
                .map_or_default(|e| e.to_string_lossy().to_ascii_lowercase());
            if !books::BOOK_EXTENSIONS.contains(&ext.as_str()) {
                return Err(format!(
                    "a Kindle can't open .{ext} files; add it to your library and send it from there"
                ));
            }
            let data = std::fs::read(source).map_err(|e| e.to_string())?;
            let stem = source.file_stem().unwrap_or_default().to_string_lossy();
            let name = file_name(&stem, &[], &ext);
            let mut progress = copy_progress(stage);
            let sent = device
                .send(
                    &Outgoing {
                        file_name: &name,
                        data: &data,
                        cover: None,
                        page_numbers,
                    },
                    &mut progress,
                )
                .map_err(|e| e.to_string())?;
            Ok(SentInfo {
                content_id: sent.path.clone(),
                device_path: sent.path,
                bytes: data.len() as u64,
                skipped_pages: Vec::new(),
                details_only: false,
            })
        })();
        SendOutcome {
            source: source.display().to_string(),
            book_id: None,
            result,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_and_titles() {
        assert_eq!(
            clipping_date("Monday, 3 November 2025 21:04:10").as_deref(),
            Some("2025-11-03T21:04:10+00:00")
        );
        assert_eq!(
            clipping_date("Monday, November 3, 2025 9:04:10 PM").as_deref(),
            Some("2025-11-03T21:04:10+00:00")
        );
        assert_eq!(clipping_date("lundi 3 novembre 2025"), None);
        assert!(same_title("The Enchiridion", "the enchiridion"));
        assert!(same_title("Meditations: Book 1", "Meditations — Book 1"));
        assert!(!same_title("", ""));
    }
}
