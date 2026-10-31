//! Sending books to a Kobo: convert each EPUB to KEPUB, then copy it to the
//! device. Each book succeeds or fails on its own so one bad file doesn't stop
//! the rest.

use std::path::{Path, PathBuf};

use erc_convert::Converter;
use erc_device_kobo::{BOOKS_DIR, KoboDevice, device_file_name};
use erc_library::{Book, Library};
use serde::Serialize;

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
    Copying {
        index: usize,
        total: usize,
        file: String,
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
}

const KEPUB_EXT: &str = ".kepub.epub";

/// Where a loose file goes on the device: `KEpub/<file stem>.kepub.epub`.
pub fn device_path_for(source: &Path) -> String {
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    let lower = name.to_lowercase();
    let stem = [KEPUB_EXT, ".kepub", ".epub"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(&name[..], |ext| &name[..name.len() - ext.len()]);
    format!("{BOOKS_DIR}/{}{KEPUB_EXT}", device_file_name(stem))
}

/// Where a library book goes on the device: `KEpub/<Title> - <Author>.kepub.epub`.
pub fn device_path_for_book(book: &Book) -> String {
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
        outcome(deliver(device, converter, source, !is_kepub, &dest, stage))
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
            stage(|index, total, file| Progress::Preparing { index, total, file });
            let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
            let exported = workdir.path().join("book.epub");
            library.export(id, &exported).map_err(|e| e.to_string())?;
            let sent = deliver(device, converter, &exported, true, &dest, stage)?;
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

type Stage = fn(usize, usize, String) -> Progress;

/// Runs `send` for each label, wrapping it with stage and result events.
fn run_batch(
    labels: &[String],
    mut on_progress: impl FnMut(Progress),
    mut send: impl FnMut(usize, &mut dyn FnMut(Stage)) -> SendOutcome,
) -> Vec<SendOutcome> {
    let total = labels.len();
    (0..total)
        .map(|i| {
            let (index, file) = (i + 1, labels[i].clone());
            let outcome = send(i, &mut |stage: Stage| {
                on_progress(stage(index, total, file.clone()))
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
    stage: &mut dyn FnMut(Stage),
) -> Result<SentInfo, String> {
    if device.mount().join(dest).exists() {
        return Err(format!("{dest} is already on the device"));
    }
    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let upload = if convert {
        stage(|index, total, file| Progress::Converting { index, total, file });
        let out = workdir.path().join(format!("book{KEPUB_EXT}"));
        converter.convert(epub, &out).map_err(|e| e.to_string())?;
        out
    } else {
        epub.to_path_buf()
    };
    stage(|index, total, file| Progress::Copying { index, total, file });
    let sent = device.send_file(&upload, dest).map_err(|e| e.to_string())?;
    Ok(SentInfo {
        device_path: sent.path,
        content_id: sent.content_id,
        bytes: sent.bytes,
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
    }
}
