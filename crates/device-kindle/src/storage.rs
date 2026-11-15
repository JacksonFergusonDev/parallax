//! The Kindle's storage, reached either as a mounted drive or over MTP.
//!
//! Every path is relative to the storage root and uses `/`, as calibre's MTP
//! driver does, so the rest of this crate never learns which one it has.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::{Error, Result};

/// One file or folder in a listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    /// Seconds since the Unix epoch, when the device reports it.
    pub modified: Option<i64>,
}

/// Called with bytes written so far and the total.
pub type Progress<'a> = &'a mut dyn FnMut(u64, u64);

/// Files on the device. Implementations must make [`Storage::write`] leave
/// either the old file or the whole new one, never a partial file under the
/// final name.
pub trait Storage: Send + Sync {
    /// The folder's entries, or an empty list when it doesn't exist.
    fn list(&self, dir: &str) -> Result<Vec<Entry>>;
    /// The file's size, or `None` when it doesn't exist.
    fn size(&self, path: &str) -> Result<Option<u64>>;
    fn read(&self, path: &str) -> Result<Vec<u8>>;
    /// Up to `len` bytes from the start of the file.
    fn read_prefix(&self, path: &str, len: u64) -> Result<Vec<u8>>;
    /// Writes the file, creating its folders and replacing any file there.
    fn write(&self, path: &str, data: &[u8], progress: Progress) -> Result<()>;
    /// Removes a file, or a folder and everything in it. Missing is fine.
    fn remove(&self, path: &str) -> Result<()>;
    fn create_dir_all(&self, path: &str) -> Result<()>;
    /// Free bytes, when known.
    fn free_space(&self) -> Option<u64>;

    fn exists(&self, path: &str) -> Result<bool> {
        Ok(self.size(path)?.is_some() || self.is_dir(path)?)
    }

    fn is_dir(&self, path: &str) -> Result<bool> {
        let (parent, name) = split(path);
        Ok(self
            .list(parent)?
            .iter()
            .any(|e| e.is_dir && e.name == name))
    }
}

/// `"documents/a.azw3"` → `("documents", "a.azw3")`; `"a"` → `("", "a")`.
pub fn split(path: &str) -> (&str, &str) {
    let path = path.trim_matches('/');
    path.rsplit_once('/').unwrap_or(("", path))
}

/// Joins path components with `/`, skipping empty ones.
pub fn join(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|p| p.trim_matches('/'))
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join("/")
}

/// A Kindle mounted as a drive.
#[derive(Debug, Clone)]
pub struct DriveStorage {
    root: PathBuf,
}

impl DriveStorage {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    fn path(&self, rel: &str) -> PathBuf {
        let mut path = self.root.clone();
        for part in rel.split('/').filter(|p| !p.is_empty() && *p != ".") {
            // Paths come from this crate, never from the device's own files,
            // but refuse to climb out of the device all the same.
            if part != ".." {
                path.push(part);
            }
        }
        path
    }
}

fn io(path: &Path) -> impl FnOnce(std::io::Error) -> Error + '_ {
    move |source| Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

impl Storage for DriveStorage {
    fn list(&self, dir: &str) -> Result<Vec<Entry>> {
        let path = self.path(dir);
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io(&path)(e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // macOS litters FAT drives with AppleDouble files.
            if name.starts_with("._") {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            out.push(Entry {
                name,
                is_dir: meta.is_dir(),
                size: meta.len(),
                modified: meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| i64::try_from(d.as_secs()).ok()),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    fn size(&self, path: &str) -> Result<Option<u64>> {
        let path = self.path(path);
        match fs::metadata(&path) {
            Ok(meta) if meta.is_file() => Ok(Some(meta.len())),
            Ok(_) => Ok(None),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(io(&path)(e)),
        }
    }

    fn is_dir(&self, path: &str) -> Result<bool> {
        Ok(self.path(path).is_dir())
    }

    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let path = self.path(path);
        fs::read(&path).map_err(io(&path))
    }

    fn read_prefix(&self, path: &str, len: u64) -> Result<Vec<u8>> {
        let path = self.path(path);
        let mut out = Vec::new();
        File::open(&path)
            .and_then(|f| f.take(len).read_to_end(&mut out))
            .map_err(io(&path))?;
        Ok(out)
    }

    fn write(&self, rel: &str, data: &[u8], progress: Progress) -> Result<()> {
        let path = self.path(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(io(parent))?;
        }
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let tmp = path.with_file_name(format!(".{name}.parallax-part"));
        let total = data.len() as u64;
        let result = (|| {
            let mut file = File::create(&tmp)?;
            let mut written = 0u64;
            for chunk in data.chunks(1 << 20) {
                file.write_all(chunk)?;
                written += chunk.len() as u64;
                progress(written, total);
            }
            file.sync_all()?;
            fs::rename(&tmp, &path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        result.map_err(io(&path))
    }

    fn remove(&self, rel: &str) -> Result<()> {
        let path = self.path(rel);
        let result = if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        match result {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(&path)(e)),
            _ => Ok(()),
        }
    }

    fn create_dir_all(&self, rel: &str) -> Result<()> {
        let path = self.path(rel);
        fs::create_dir_all(&path).map_err(io(&path))
    }

    fn free_space(&self) -> Option<u64> {
        free_space(&self.root)
    }
}

#[cfg(unix)]
fn free_space(path: &Path) -> Option<u64> {
    // `df -k` is everywhere on macOS and Linux and saves an unsafe statvfs.
    let out = std::process::Command::new("df")
        .arg("-k")
        .arg(path)
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    let line = text.lines().nth(1)?;
    let available: u64 = line.split_whitespace().nth(3)?.parse().ok()?;
    Some(available * 1024)
}

#[cfg(not(unix))]
fn free_space(_: &Path) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_join() {
        assert_eq!(split("documents/a.azw3"), ("documents", "a.azw3"));
        assert_eq!(split("/a/"), ("", "a"));
        assert_eq!(
            join(&["documents", "", "/b.sdr/", "b.apnx"]),
            "documents/b.sdr/b.apnx"
        );
    }

    #[test]
    fn drive_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let s = DriveStorage::new(dir.path());
        assert!(s.list("documents").unwrap().is_empty());
        let mut seen = 0;
        s.write("documents/x/a.txt", b"hello", &mut |w, _| seen = w)
            .unwrap();
        assert_eq!(seen, 5);
        assert_eq!(s.read("documents/x/a.txt").unwrap(), b"hello");
        assert_eq!(s.read_prefix("documents/x/a.txt", 2).unwrap(), b"he");
        assert_eq!(s.size("documents/x/a.txt").unwrap(), Some(5));
        assert!(s.is_dir("documents/x").unwrap());
        assert!(s.exists("documents/x").unwrap());
        let names: Vec<_> = s
            .list("documents/x")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["a.txt"]);
        s.remove("documents/x").unwrap();
        assert!(!s.exists("documents/x").unwrap());
        s.remove("documents/missing").unwrap();
        // Can't escape the root.
        assert_eq!(s.path("../../etc"), dir.path().join("etc"));
    }
}
