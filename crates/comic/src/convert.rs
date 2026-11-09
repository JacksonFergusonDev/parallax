//! CBR and CB7 (or any archive libarchive reads) to CBZ. libarchive is the
//! copy built into macOS, so RAR is read without unrar, whose licence
//! conflicts with the GPL.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use compress_tools::{ArchiveContents, ArchiveIterator};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::entries::{
    Kind, MAX_COMIC_INFO_BYTES, MAX_ENTRIES, MAX_PAGE_BYTES, SkipReason, Skipped, classify,
    normalise,
};
use crate::error::{Error, Result, damaged, io};

/// What a conversion kept and left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Converted {
    pub pages: usize,
    pub comic_info: bool,
    pub skipped: Vec<Skipped>,
}

/// Regular files only; directories and links are ignored.
const S_IFMT: u64 = 0o170_000;
const S_IFREG: u64 = 0o100_000;

enum Current {
    None,
    Page(String, Vec<u8>),
    Info(Vec<u8>),
}

/// Writes every page of `src` into a new CBZ at `dest`, stored rather than
/// compressed (images don't shrink), under the same names. ComicInfo.xml
/// comes along; other files are left out and listed. `dest` appears only
/// once it is complete.
pub fn convert_to_cbz(src: &Path, dest: &Path) -> Result<Converted> {
    let file = File::open(src).map_err(io(src))?;
    let mut entries =
        ArchiveIterator::from_read(BufReader::new(file)).map_err(|e| libarchive(src, &e))?;

    let dir = dest.parent().unwrap_or(Path::new("."));
    let tmp = tempfile::NamedTempFile::new_in(dir).map_err(io(dir))?;
    let mut zip = ZipWriter::new(BufWriter::new(tmp.as_file()));
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(false);

    let mut result = Converted {
        pages: 0,
        comic_info: false,
        skipped: Vec::new(),
    };
    let mut seen = HashSet::new();
    let mut count = 0usize;
    let mut current = Current::None;
    for item in &mut entries {
        match item {
            ArchiveContents::StartOfEntry(name, stat) => {
                count += 1;
                if count > MAX_ENTRIES {
                    return Err(Error::TooManyEntries {
                        path: src.to_path_buf(),
                        limit: MAX_ENTRIES,
                    });
                }
                let name = normalise(&name);
                let regular = u64::from(stat.st_mode) & S_IFMT == S_IFREG;
                current = match classify(&name) {
                    _ if !regular => Current::None,
                    Kind::Page if seen.insert(name.to_lowercase()) => {
                        Current::Page(name, Vec::new())
                    }
                    Kind::Page => {
                        result.skipped.push(Skipped {
                            name,
                            reason: SkipReason::Duplicate,
                        });
                        Current::None
                    }
                    Kind::ComicInfo if !result.comic_info && name.matches('/').count() <= 1 => {
                        Current::Info(Vec::new())
                    }
                    Kind::ComicInfo => Current::None,
                    Kind::Skip(reason) => {
                        result.skipped.push(Skipped { name, reason });
                        Current::None
                    }
                };
            }
            ArchiveContents::DataChunk(chunk) => match &mut current {
                Current::Page(name, bytes) => {
                    bytes.extend_from_slice(&chunk);
                    if bytes.len() as u64 > MAX_PAGE_BYTES {
                        return Err(Error::EntryTooLarge {
                            path: src.to_path_buf(),
                            name: name.clone(),
                            limit: MAX_PAGE_BYTES,
                        });
                    }
                }
                Current::Info(bytes) => {
                    if (bytes.len() + chunk.len()) as u64 <= MAX_COMIC_INFO_BYTES {
                        bytes.extend_from_slice(&chunk);
                    }
                }
                Current::None => {}
            },
            ArchiveContents::EndOfEntry => match std::mem::replace(&mut current, Current::None) {
                Current::Page(name, bytes) => {
                    zip.start_file(&name, stored).map_err(damaged(dest))?;
                    zip.write_all(&bytes).map_err(io(dest))?;
                    result.pages += 1;
                }
                Current::Info(bytes) => {
                    zip.start_file("ComicInfo.xml", SimpleFileOptions::default())
                        .map_err(damaged(dest))?;
                    zip.write_all(&bytes).map_err(io(dest))?;
                    result.comic_info = true;
                }
                Current::None => {}
            },
            ArchiveContents::Err(e) => return Err(libarchive(src, &e)),
        }
    }
    entries.close().map_err(|e| libarchive(src, &e))?;
    if result.pages == 0 {
        return Err(Error::NoPages(src.to_path_buf()));
    }
    zip.finish()
        .map_err(damaged(dest))?
        .flush()
        .map_err(io(dest))?;
    tmp.as_file().sync_all().map_err(io(dest))?;
    tmp.persist(dest).map_err(|e| Error::Io {
        path: dest.to_path_buf(),
        source: e.error,
    })?;
    Ok(result)
}

fn libarchive(path: &Path, e: &compress_tools::Error) -> Error {
    let message = e.to_string();
    let lower = message.to_lowercase();
    if lower.contains("passphrase") || lower.contains("encrypt") || lower.contains("password") {
        return Error::Encrypted(path.to_path_buf());
    }
    if lower.contains("unrecognized archive format") {
        return Error::NotAnArchive(path.to_path_buf());
    }
    Error::Damaged {
        path: path.to_path_buf(),
        message,
    }
}
