//! Sending books to a Kobo: convert each EPUB to KEPUB, then copy it to the
//! device. Each book succeeds or fails on its own so one bad file doesn't stop
//! the rest.

use std::path::{Path, PathBuf};

use erc_convert::Converter;
use erc_device_kobo::{BOOKS_DIR, KoboDevice, device_file_name};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "stage")]
pub enum Progress {
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
    pub source: PathBuf,
    pub result: Result<SentInfo, String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SentInfo {
    pub device_path: String,
    pub content_id: String,
    pub bytes: u64,
}

const KEPUB_EXT: &str = ".kepub.epub";

/// Where a book goes on the device: `KEpub/<file stem>.kepub.epub`.
pub fn device_path_for(source: &Path) -> String {
    let name = source.file_name().unwrap_or_default().to_string_lossy();
    let lower = name.to_lowercase();
    let stem = [KEPUB_EXT, ".kepub", ".epub"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(&name[..], |ext| &name[..name.len() - ext.len()]);
    format!("{BOOKS_DIR}/{}{KEPUB_EXT}", device_file_name(stem))
}

/// Converts (unless already a KEPUB) and copies each of `sources` to `device`.
pub fn send_books(
    device: &KoboDevice,
    converter: &dyn Converter,
    sources: &[PathBuf],
    mut on_progress: impl FnMut(Progress),
) -> Vec<SendOutcome> {
    let total = sources.len();
    sources
        .iter()
        .enumerate()
        .map(|(i, source)| {
            let index = i + 1;
            let file = source
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let result = send_one(device, converter, source, |p| {
                on_progress(p(index, total, file.clone()))
            });
            on_progress(match &result {
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
            SendOutcome {
                source: source.clone(),
                result,
            }
        })
        .collect()
}

type Stage = fn(usize, usize, String) -> Progress;

fn send_one(
    device: &KoboDevice,
    converter: &dyn Converter,
    source: &Path,
    mut on_stage: impl FnMut(Stage),
) -> Result<SentInfo, String> {
    let dest = device_path_for(source);
    if device.mount().join(&dest).exists() {
        return Err(format!("{dest} is already on the device"));
    }
    let is_kepub = source.to_string_lossy().to_lowercase().ends_with(KEPUB_EXT);
    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let upload = if is_kepub {
        source.to_path_buf()
    } else {
        on_stage(|index, total, file| Progress::Converting { index, total, file });
        let out = workdir.path().join(format!("book{KEPUB_EXT}"));
        converter.convert(source, &out).map_err(|e| e.to_string())?;
        out
    };
    on_stage(|index, total, file| Progress::Copying { index, total, file });
    let sent = device
        .send_file(&upload, &dest)
        .map_err(|e| e.to_string())?;
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
