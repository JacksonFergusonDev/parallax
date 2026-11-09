//! Sending books to a Kobo: convert each EPUB to KEPUB, then copy it to the
//! device. Each book succeeds or fails on its own so one bad file doesn't stop
//! the rest.

use std::path::{Path, PathBuf};

use erc_comic::prepare::{Prepared, safe_kobo_component};
use erc_convert::Converter;
use erc_device_kobo::{
    BOOKS_DIR, DisplayMetadata, KoboDevice, device_file_name, replace_book_with_progress,
};
use erc_library::{Book, Format, Library};
use serde::Serialize;

use crate::comic_send::{ComicTarget, check_space, is_prepared_copy};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "stage")]
pub enum Progress {
    Preparing {
        index: usize,
        total: usize,
        file: String,
    },
    Converting {
        index: usize,
        total: usize,
        file: String,
    },
    ProcessingComic {
        index: usize,
        total: usize,
        file: String,
        page: usize,
        pages: usize,
    },
    Copying {
        index: usize,
        total: usize,
        file: String,
        bytes: u64,
        total_bytes: u64,
    },
    Done {
        index: usize,
        total: usize,
        file: String,
        device_path: String,
    },
    Failed {
        index: usize,
        total: usize,
        file: String,
        error: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct SendOutcome {
    /// What was sent: a file path, or a library book's title.
    pub source: String,
    pub book_id: Option<i64>,
    pub result: Result<SentInfo, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentInfo {
    pub device_path: String,
    pub content_id: String,
    pub bytes: u64,
    /// Comic pages left out because they aren't readable images.
    pub skipped_pages: Vec<String>,
}

pub const COMIC_SPREADS: &str = "comic_spreads";
pub const COMIC_CROP: &str = "comic_crop";
pub const COMIC_AUTOCONTRAST: &str = "comic_autocontrast";
pub const COMIC_UPSCALE: &str = "comic_upscale";
/// Dithering of greyscale pages; on unless set to "off".
pub const COMIC_DITHER: &str = "comic_dither";
/// Colour pages on greyscale screens: colour separation strength, a
/// percentage of the recommended strength, which is the default.
pub const COMIC_COLOUR_SEPARATION: &str = "comic_colour_separation";
/// Colour pages on greyscale screens: contrast stretch; on unless "off".
pub const COMIC_COLOUR_CONTRAST: &str = "comic_colour_contrast";
/// Colour pages on greyscale screens: mid-tone darkening on mostly light
/// pages, a percentage of the recommended amount, which is the default.
pub const COMIC_MIDTONE_DARKENING: &str = "comic_midtone_darkening";
/// The percentages strength settings may hold; no value is 100.
pub const COMIC_STRENGTHS: [&str; 6] = ["0", "50", "75", "125", "150", "200"];
/// Every comic page setting, for resetting them to recommended together.
pub const COMIC_PAGE_SETTINGS: [&str; 8] = [
    COMIC_SPREADS,
    COMIC_CROP,
    COMIC_AUTOCONTRAST,
    COMIC_UPSCALE,
    COMIC_DITHER,
    COMIC_COLOUR_SEPARATION,
    COMIC_COLOUR_CONTRAST,
    COMIC_MIDTONE_DARKENING,
];

const KEPUB_EXT: &str = ".kepub.epub";

/// Where a loose file goes on the device: `KEpub/<file stem>.kepub.epub`.
pub fn device_path_for(source: &Path) -> String {
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    let lower = name.to_lowercase();
    let stem = [
        KEPUB_EXT, ".kepub", ".epub", ".azw3", ".azw", ".mobi", ".prc", ".kfx",
    ]
    .iter()
    .find(|ext| lower.ends_with(*ext))
    .map_or(&name[..], |ext| &name[..name.len() - ext.len()]);
    format!("{BOOKS_DIR}/{}{KEPUB_EXT}", device_file_name(stem))
}

/// Where a library book goes on the device: `KEpub/<Title> - <Author>.kepub.epub`.
pub fn device_path_for_book(book: &Book) -> String {
    if book.format == Format::Cbz {
        let name = match (&book.series, &book.series_index) {
            (Some(series), Some(index)) => {
                let volume = index
                    .parse::<u32>()
                    .map_or_else(|_| index.clone(), |n| format!("{n:02}"));
                format!("{series} v{volume} - {}", book.title)
            }
            (Some(series), None) if series != &book.title => format!("{series} - {}", book.title),
            _ => book.title.clone(),
        };
        let name = device_file_name(&safe_kobo_component(&name));
        return format!("{BOOKS_DIR}/{name}{KEPUB_EXT}");
    }
    let name = match book.authors.first() {
        Some(author) => format!("{} - {author}", book.title),
        None => book.title.clone(),
    };
    format!("{BOOKS_DIR}/{}{KEPUB_EXT}", device_file_name(&name))
}

/// Converts (unless already a KEPUB) and copies each of `sources` to `device`.
pub fn send_books(
    device: &KoboDevice,
    converter: &dyn Converter,
    sources: &[PathBuf],
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
        let source = &sources[i];
        let outcome = |result| SendOutcome {
            source: source.display().to_string(),
            book_id: None,
            result,
        };
        let dest = device_path_for(source);
        let is_kepub = source.to_string_lossy().to_lowercase().ends_with(KEPUB_EXT);
        outcome(deliver(
            device, converter, source, !is_kepub, &dest, None, stage,
        ))
    })
}

/// Exports each library book with its edited metadata, converts it, copies it
/// to `device`, and records the copy in the library.
pub fn send_library_books(
    library: &Library,
    device: &KoboDevice,
    converter: &dyn Converter,
    ids: &[i64],
    on_progress: impl FnMut(Progress),
) -> Vec<SendOutcome> {
    let books: Vec<Result<Book, String>> = ids
        .iter()
        .map(|&id| library.book(id).map_err(|e| e.to_string()))
        .collect();
    let labels: Vec<String> = books
        .iter()
        .zip(ids)
        .map(|(b, id)| {
            b.as_ref()
                .map_or_else(|_| format!("Book {id}"), |b| b.title.clone())
        })
        .collect();
    let serial = device.version().serial.clone();
    // Copies already on this device, which may have a different file name if
    // the book was renamed since it was sent.
    let existing = library.device_copies(&serial).unwrap_or_default();
    let comic_target = std::cell::OnceCell::new();
    run_batch(&labels, on_progress, |i, stage| {
        let id = ids[i];
        let result = books[i].clone().and_then(|book| {
            if let Some(copy) = existing
                .iter()
                .find(|c| c.book_id == id && device.mount().join(&c.device_path).is_file())
            {
                return Err(format!("already on the device as {}", copy.device_path));
            }
            let dest = device_path_for_book(&book);
            stage(&|index, total, file| Progress::Preparing { index, total, file });
            if device.mount().join(&dest).exists() {
                return Err(format!("{dest} is already on the device"));
            }
            device.check_writable().map_err(|e| e.to_string())?;
            let upload = prepare_upload(library, device, converter, &book, &comic_target, stage)?;
            upload.check_space(device)?;
            let sent = device
                .send_file_with_progress(upload.path(), &dest, Some(id), copy_progress(stage))
                .map_err(|e| e.to_string())?;
            library
                .record_device_copy_profile(
                    id,
                    &serial,
                    &sent.path,
                    &sent.content_id,
                    upload.fingerprint(),
                )
                .map_err(|e| e.to_string())?;
            let sent = SentInfo {
                device_path: sent.path,
                content_id: sent.content_id,
                bytes: sent.bytes,
                skipped_pages: upload.skipped_pages(),
            };
            Ok(sent)
        });
        SendOutcome {
            source: labels[i].clone(),
            book_id: Some(id),
            result,
        }
    })
}

/// Re-sends library books that are already on the device, replacing each
/// copy in place (same path, so reading progress is kept) and updating the
/// title, author and series Nickel shows. Runs in one backed-up write session.
pub fn update_on_device(
    library: &Library,
    device: &KoboDevice,
    converter: &dyn Converter,
    backup_root: &Path,
    ids: &[i64],
    on_progress: impl FnMut(Progress),
) -> erc_device_kobo::Result<Vec<SendOutcome>> {
    let serial = device.version().serial.clone();
    let copies = library.device_copies(&serial).unwrap_or_default();
    let labels: Vec<String> = ids
        .iter()
        .map(|&id| {
            library
                .book(id)
                .map_or_else(|_| format!("Book {id}"), |b| b.title)
        })
        .collect();
    let mut writer = device.open_writer(backup_root)?;
    let comic_target = std::cell::OnceCell::new();
    let outcomes = run_batch(&labels, on_progress, |i, stage| {
        let id = ids[i];
        let result = (|| {
            let copy = copies
                .iter()
                .find(|c| c.book_id == id && device.mount().join(&c.device_path).is_file())
                .ok_or("not on the device")?;
            let book = library.book(id).map_err(|e| e.to_string())?;
            if book.format == Format::Cbz && !is_prepared_copy(&copy.device_path) {
                return Err(format!(
                    "{} is the comic file as it was added, so it can't be updated. Remove it from the Kobo and send it again.",
                    copy.device_path
                ));
            }
            stage(&|index, total, file| Progress::Preparing { index, total, file });
            let upload = prepare_upload(library, device, converter, &book, &comic_target, stage)?;
            let bytes = upload.check_space(device)?;
            let meta = DisplayMetadata {
                title: book.title.clone(),
                author: (!book.authors.is_empty()).then(|| book.authors.join(", ")),
                series: book.series.clone(),
                series_number: book.series.as_ref().and(book.series_index.clone()),
            };
            replace_book_with_progress(
                &mut writer,
                upload.path(),
                &copy.device_path,
                &meta,
                copy_progress(stage),
            )
            .map_err(|e| e.to_string())?;
            library
                .record_device_copy_profile(
                    id,
                    &serial,
                    &copy.device_path,
                    &copy.content_id,
                    upload.fingerprint(),
                )
                .map_err(|e| e.to_string())?;
            Ok(SentInfo {
                device_path: copy.device_path.clone(),
                content_id: copy.content_id.clone(),
                bytes,
                skipped_pages: upload.skipped_pages(),
            })
        })();
        SendOutcome {
            source: labels[i].clone(),
            book_id: Some(id),
            result,
        }
    });
    writer.close()?;
    Ok(outcomes)
}

enum Upload {
    Comic(Prepared, String),
    Epub {
        _work: tempfile::TempDir,
        path: PathBuf,
    },
}

impl Upload {
    fn path(&self) -> &Path {
        match self {
            Self::Comic(prepared, _) => prepared.path(),
            Self::Epub { path, .. } => path,
        }
    }

    fn skipped_pages(&self) -> Vec<String> {
        match self {
            Self::Comic(prepared, _) => prepared.skipped.clone(),
            Self::Epub { .. } => Vec::new(),
        }
    }

    fn fingerprint(&self) -> Option<&str> {
        match self {
            Self::Comic(_, hash) => Some(hash),
            Self::Epub { .. } => None,
        }
    }

    fn check_space(&self, device: &KoboDevice) -> Result<u64, String> {
        match self {
            Self::Comic(prepared, _) => check_space(device, prepared.path()),
            Self::Epub { path, .. } => Ok(std::fs::metadata(path).map_or(0, |m| m.len())),
        }
    }
}

fn prepare_upload(
    library: &Library,
    device: &KoboDevice,
    converter: &dyn Converter,
    book: &Book,
    target: &std::cell::OnceCell<erc_library::Result<ComicTarget>>,
    stage: &mut dyn FnMut(Stage<'_>),
) -> Result<Upload, String> {
    if book.format == Format::Cbz {
        let target = target
            .get_or_init(|| ComicTarget::new(library, device))
            .as_ref()
            .map_err(ToString::to_string)?;
        let prepared = target.prepare(library, book, converter, |page, pages| {
            stage(&|index, total, file| Progress::ProcessingComic {
                index,
                total,
                file,
                page,
                pages,
            });
        })?;
        return Ok(Upload::Comic(prepared, target.fingerprint(book)));
    }
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let exported = work.path().join("book.epub");
    let path = work.path().join(format!("book{KEPUB_EXT}"));
    library
        .export(book.id, &exported)
        .map_err(|e| e.to_string())?;
    stage(&|index, total, file| Progress::Converting { index, total, file });
    converter
        .convert(&exported, &path)
        .map_err(|e| e.to_string())?;
    Ok(Upload::Epub { _work: work, path })
}

type Stage<'a> = &'a dyn Fn(usize, usize, String) -> Progress;

fn copy_progress(stage: &mut dyn FnMut(Stage<'_>)) -> impl FnMut(u64, u64) + '_ {
    move |bytes, total_bytes| {
        stage(&|index, total, file| Progress::Copying {
            index,
            total,
            file,
            bytes,
            total_bytes,
        });
    }
}

/// Runs `send` for each label, wrapping it with stage and result events.
fn run_batch(
    labels: &[String],
    mut on_progress: impl FnMut(Progress),
    mut send: impl FnMut(usize, &mut dyn FnMut(Stage<'_>)) -> SendOutcome,
) -> Vec<SendOutcome> {
    let total = labels.len();
    (0..total)
        .map(|i| {
            let (index, file) = (i + 1, labels[i].clone());
            let outcome = send(i, &mut |stage: Stage<'_>| {
                on_progress(stage(index, total, file.clone()));
            });
            on_progress(match &outcome.result {
                Ok(info) => Progress::Done {
                    index,
                    total,
                    file,
                    device_path: info.device_path.clone(),
                },
                Err(error) => Progress::Failed {
                    index,
                    total,
                    file,
                    error: error.clone(),
                },
            });
            outcome
        })
        .collect()
}

fn deliver(
    device: &KoboDevice,
    converter: &dyn Converter,
    epub: &Path,
    convert: bool,
    dest: &str,
    library_book: Option<i64>,
    stage: &mut dyn FnMut(Stage<'_>),
) -> Result<SentInfo, String> {
    if device.mount().join(dest).exists() {
        return Err(format!("{dest} is already on the device"));
    }
    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let upload = if convert {
        stage(&|index, total, file| Progress::Converting { index, total, file });
        let out = workdir.path().join(format!("book{KEPUB_EXT}"));
        converter.convert(epub, &out).map_err(|e| e.to_string())?;
        out
    } else {
        epub.to_path_buf()
    };
    let sent = device
        .send_file_with_progress(&upload, dest, library_book, copy_progress(stage))
        .map_err(|e| e.to_string())?;
    Ok(SentInfo {
        device_path: sent.path,
        content_id: sent.content_id,
        bytes: sent.bytes,
        skipped_pages: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_paths() {
        assert_eq!(
            device_path_for(Path::new("/x/Dune.epub")),
            "KEpub/Dune.kepub.epub"
        );
        assert_eq!(
            device_path_for(Path::new("/x/Dune.kepub.epub")),
            "KEpub/Dune.kepub.epub"
        );
        assert_eq!(
            device_path_for(Path::new("/x/A: B.EPUB")),
            "KEpub/A_ B.kepub.epub"
        );
        assert_eq!(
            device_path_for(Path::new("/x/Dune.azw3")),
            "KEpub/Dune.kepub.epub"
        );
    }
}
