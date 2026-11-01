//! Bringing books that are already on a Kobo into the desktop library.
//!
//! Only reads the device. Each sideloaded EPUB is cleaned of Kobo markup,
//! added to the library (or matched to a book already there), given the
//! series Nickel shows if the EPUB has none, and recorded as a device copy so
//! it shows as "On Kobo" without being sent again. Store books are skipped:
//! they're DRM-protected and Kobo keeps them in its cloud.

use std::path::Path;

use erc_device::DeviceBook;
use erc_device_kobo::KoboDevice;
use erc_epub::{Epub, has_drm, strip_kobo_markup};
use erc_library::{ImportOutcome, Library};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeviceImport {
    /// Added as a new library book.
    Added {
        book_id: i64,
    },
    /// The library already had this book; the device copy is now linked.
    Linked {
        book_id: i64,
    },
    Skipped {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceImportOutcome {
    pub content_id: String,
    pub title: String,
    pub result: Result<DeviceImport, String>,
}

/// Imports `content_ids` (all books when `None`) from `device` into
/// `library`. `on_progress(index, total, title)` is called before each book.
pub fn import_from_device(
    library: &mut Library,
    device: &KoboDevice,
    content_ids: Option<&[String]>,
    mut on_progress: impl FnMut(usize, usize, &str),
) -> erc_device_kobo::Result<Vec<DeviceImportOutcome>> {
    let books: Vec<DeviceBook> = device
        .open_db()?
        .books()?
        .into_iter()
        .filter(|b| content_ids.is_none_or(|ids| ids.contains(&b.id)))
        .collect();
    let serial = device.version().serial.clone();
    let erc = device.erc_state();
    let total = books.len();
    Ok(books
        .into_iter()
        .enumerate()
        .map(|(i, book)| {
            on_progress(i + 1, total, &book.title);
            let from_erc = erc
                .books
                .iter()
                .find(|b| b.content_id == book.id)
                .and_then(|b| b.library_book);
            let result = import_one(library, device, &serial, &book, from_erc);
            DeviceImportOutcome {
                content_id: book.id,
                title: book.title,
                result,
            }
        })
        .collect())
}

fn skip(reason: &str) -> Result<DeviceImport, String> {
    Ok(DeviceImport::Skipped {
        reason: reason.to_owned(),
    })
}

fn import_one(
    library: &mut Library,
    device: &KoboDevice,
    serial: &str,
    book: &DeviceBook,
    from_erc: Option<i64>,
) -> Result<DeviceImport, String> {
    let copies = library.device_copies(serial).map_err(|e| e.to_string())?;
    if copies.iter().any(|c| c.content_id == book.id) {
        return skip("already in your library");
    }
    let Some(path) = book.path.as_deref().filter(|_| book.sideloaded) else {
        return skip("bought from the Kobo store (DRM-protected; it stays in your Kobo account)");
    };
    if !path.to_lowercase().ends_with(".epub") {
        let ext = path.rsplit('.').next().unwrap_or_default();
        return skip(&format!("not an EPUB (.{ext})"));
    }
    let link = |library: &Library, book_id: i64| {
        library
            .record_device_copy(book_id, serial, path, &book.id)
            .map_err(|e| e.to_string())
    };
    // Sent from this library on another computer, or before its record here
    // was lost: ERC's own note on the device names the library book.
    if let Some(id) = from_erc.filter(|&id| library.book(id).is_ok()) {
        link(library, id)?;
        return Ok(DeviceImport::Linked { book_id: id });
    }

    let source = device.mount().join(path);
    if has_drm(&source).map_err(|e| e.to_string())? {
        return skip("DRM-protected");
    }
    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = clean_copy(&source, path, workdir.path())?;
    let meta = Epub::open(&file).map_err(|e| e.to_string())?.metadata();
    if let Some(id) = library.find_similar_to(&meta).map_err(|e| e.to_string())? {
        link(library, id)?;
        return Ok(DeviceImport::Linked { book_id: id });
    }

    let (book_id, added) = match library.import(&file).map_err(|e| e.to_string())? {
        ImportOutcome::Added { book: added, .. } => {
            // Nickel ignores series in sideloaded EPUBs, so a series shown on
            // the Kobo came from somewhere the EPUB doesn't record (calibre,
            // or ERC's reconcile); keep it.
            if added.series.is_none()
                && let Some(series) = &book.series
            {
                let mut edit = added.edit();
                edit.series = Some(series.clone());
                edit.series_index = book.series_number.clone();
                library.update(added.id, &edit).map_err(|e| e.to_string())?;
            }
            (added.id, true)
        }
        ImportOutcome::AlreadyInLibrary { book: existing } => (existing.id, false),
    };
    link(library, book_id)?;
    Ok(if added {
        DeviceImport::Added { book_id }
    } else {
        DeviceImport::Linked { book_id }
    })
}

/// A plain EPUB named after the device file (`Dune.kepub.epub` →
/// `Dune.epub`) in `dir`, with Kobo markup removed.
fn clean_copy(source: &Path, device_path: &str, dir: &Path) -> Result<std::path::PathBuf, String> {
    let name = device_path.rsplit('/').next().unwrap_or(device_path);
    let lower = name.to_lowercase();
    let stem = [".kepub.epub", ".epub"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(name, |ext| &name[..name.len() - ext.len()]);
    let dest = dir.join(format!("{stem}.epub"));
    strip_kobo_markup(source, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}
