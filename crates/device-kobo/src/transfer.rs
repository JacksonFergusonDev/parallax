//! Copying books onto the device and ejecting it.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::KoboDevice;
use crate::db::ONBOARD_PREFIX;
use crate::error::{Error, Result};
use crate::state::{BookChange, LogEntry, Record};

/// Folder sideloaded books go into, matching calibre's KoboTouch layout.
pub const BOOKS_DIR: &str = "KEpub";

#[derive(Debug, Clone)]
pub struct SentBook {
    /// Path relative to the mount point, `/`-separated.
    pub path: String,
    /// The `ContentID` Nickel will give the book when it imports it.
    pub content_id: String,
    pub bytes: u64,
}

impl KoboDevice {
    /// Copies `src` to `dest` (relative to the mount point). Writes to a
    /// temporary name first so an interrupted copy never leaves a half-written
    /// book where Nickel would import it, then removes the `._*` AppleDouble
    /// files macOS creates on FAT32 for every new file and folder.
    pub fn send_file(&self, src: &Path, dest: &str) -> Result<SentBook> {
        self.send_file_for(src, dest, None)
    }

    /// [`KoboDevice::send_file`] for a desktop library book, so `/.erc/`
    /// records which library book the copy came from.
    pub fn send_file_for(
        &self,
        src: &Path,
        dest: &str,
        library_book: Option<i64>,
    ) -> Result<SentBook> {
        self.check_writable()?;
        if self.mount().join(dest).exists() {
            return Err(Error::AlreadyOnDevice(dest.to_owned()));
        }
        let sent = self.write_file(src, dest)?;
        self.record(Record {
            entries: vec![LogEntry::new("send").ids([sent.content_id.clone()])],
            books: BookChange::written(dest, src, library_book)
                .into_iter()
                .collect(),
        });
        Ok(sent)
    }

    /// Like [`KoboDevice::send_file`] but overwrites an existing book at
    /// `dest` in one rename, so the path (and Nickel's `ContentID`, which
    /// carries reading progress) stays the same.
    /// Only called inside a [`crate::DeviceWriter`] session, which has
    /// already checked the Kobo may be changed.
    pub(crate) fn replace_file(&self, src: &Path, dest: &str) -> Result<SentBook> {
        let target = self.mount().join(dest);
        if !target.is_file() {
            return Err(Error::Io {
                path: target,
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "book is not on the device",
                ),
            });
        }
        self.write_file(src, dest)
    }

    fn write_file(&self, src: &Path, dest: &str) -> Result<SentBook> {
        let target = self.mount().join(dest);
        let parent = target.parent().unwrap_or(self.mount());
        let mut created_dirs: Vec<&Path> = parent
            .ancestors()
            .take_while(|d| *d != self.mount() && !d.exists())
            .collect();
        fs::create_dir_all(parent).map_err(|e| io_err(parent, e))?;
        created_dirs.reverse();
        let file_name = target.file_name().unwrap_or_default().to_string_lossy();
        let partial = parent.join(format!(".{file_name}.erc-part"));

        let bytes = copy_plain(src, &partial).inspect_err(|_| {
            let _ = fs::remove_file(&partial);
        })?;
        let expected = fs::metadata(src).map_err(|e| io_err(src, e))?.len();
        if bytes != expected {
            let _ = fs::remove_file(&partial);
            return Err(Error::IncompleteCopy {
                path: target,
                expected,
                written: bytes,
            });
        }
        fs::rename(&partial, &target).map_err(|e| io_err(&target, e))?;
        remove_apple_double(&target);
        for dir in created_dirs {
            remove_apple_double(dir);
        }
        self.disable_indexing();

        Ok(SentBook {
            content_id: format!("{ONBOARD_PREFIX}{dest}"),
            path: dest.to_owned(),
            bytes,
        })
    }

    /// Unmounts the volume so Nickel imports new books. macOS only for now.
    pub fn eject(&self) -> Result<()> {
        let out = Command::new("diskutil")
            .arg("eject")
            .arg(self.mount())
            .output()
            .map_err(|e| Error::Eject(e.to_string()))?;
        if out.status.success() {
            Ok(())
        } else {
            Err(Error::Eject(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            ))
        }
    }

    /// Stops Spotlight from indexing the volume (and littering it with
    /// `.Spotlight-V100`). Best effort.
    fn disable_indexing(&self) {
        let marker = self.mount().join(".metadata_never_index");
        if !marker.exists() && File::create(&marker).is_ok() {
            remove_apple_double(&marker);
        }
    }
}

/// macOS attaches `com.apple.provenance` to every file a process creates and
/// it cannot be removed; on FAT32 that becomes a `._<name>` companion file.
/// Deleting the companion is safe and it is not recreated (verified on the
/// reference Kobo, see docs/device-notes.md).
pub(crate) fn remove_apple_double(path: &Path) {
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        let mut companion = std::ffi::OsString::from("._");
        companion.push(name);
        let _ = fs::remove_file(parent.join(companion));
    }
}

/// Streams bytes only: no xattrs, ACLs or resource forks. `fs::copy` uses
/// `fcopyfile` on macOS, which would copy extended attributes.
pub(crate) fn copy_plain(src: &Path, dest: &Path) -> Result<u64> {
    let mut reader =
        BufReader::with_capacity(1 << 20, File::open(src).map_err(|e| io_err(src, e))?);
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dest)
        .map_err(|e| io_err(dest, e))?;
    let mut writer = BufWriter::with_capacity(1 << 20, file);
    let bytes = io::copy(&mut reader, &mut writer).map_err(|e| io_err(dest, e))?;
    let file = writer
        .into_inner()
        .map_err(|e| io_err(dest, e.into_error()))?;
    file.sync_all().map_err(|e| io_err(dest, e))?;
    Ok(bytes)
}

/// A FAT32-safe file name built from `name`. calibre's KoboTouch makes similar
/// substitutions; `:` in particular is common in titles.
pub fn device_file_name(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned
        .trim()
        .trim_end_matches(['.', ' '])
        .trim_start_matches('.');
    // FAT32 allows 255 UTF-16 units per name; leave room for the extension.
    let mut out = String::new();
    for c in trimmed.chars() {
        if out.encode_utf16().count() + c.len_utf16() > 180 {
            break;
        }
        out.push(c);
    }
    if out.is_empty() {
        "book".to_owned()
    } else {
        out
    }
}

fn io_err(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: PathBuf::from(path),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitises_names_for_fat32() {
        assert_eq!(
            device_file_name("A Game of Thrones: Book 1"),
            "A Game of Thrones_ Book 1"
        );
        assert_eq!(device_file_name("  .hidden?. "), "hidden_");
        assert_eq!(device_file_name("..."), "book");
        assert!(device_file_name(&"x".repeat(400)).len() <= 180);
    }
}
