//! Reading a book's files one at a time, for a reader to display.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;
use zip::ZipArchive;

use crate::{Error, Result};

/// The most one file in a book may unpack to, so a zip bomb can't exhaust
/// memory. Real entries are far smaller: the largest in a 95 MB book is 12 MB.
const MAX_ENTRY: u64 = 256 * 1024 * 1024;

/// A file inside a book.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    pub name: String,
    pub size: u64,
}

/// An open book, which stays open so each file is a seek rather than a reread
/// of the central directory.
pub struct BookFile {
    path: PathBuf,
    zip: ZipArchive<BufReader<File>>,
}

impl BookFile {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(|source| Error::Io {
            path: path.to_owned(),
            source,
        })?;
        Ok(Self {
            path: path.to_owned(),
            zip: ZipArchive::new(BufReader::new(file))?,
        })
    }

    /// Every file in the book, leaving out folders.
    pub fn entries(&mut self) -> Result<Vec<Entry>> {
        let mut entries = Vec::with_capacity(self.zip.len());
        for i in 0..self.zip.len() {
            let entry = self.zip.by_index(i)?;
            if !entry.is_dir() {
                entries.push(Entry {
                    name: entry.name()?.into_owned(),
                    size: entry.size(),
                });
            }
        }
        Ok(entries)
    }

    /// The contents of the file called `name`. Names only ever look inside the
    /// zip, so a book can't use one to reach the file system.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>> {
        let entry = self.zip.by_name(name)?;
        let mut data = Vec::new();
        entry
            .take(MAX_ENTRY + 1)
            .read_to_end(&mut data)
            .map_err(|source| Error::Io {
                path: self.path.clone(),
                source,
            })?;
        if data.len() as u64 > MAX_ENTRY {
            return Err(Error::Invalid(format!("{name} is too large to read")));
        }
        Ok(data)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use zip::ZipWriter;
    use zip::write::SimpleFileOptions;

    use super::*;

    fn book(dir: &Path) -> PathBuf {
        let path = dir.join("book.epub");
        let mut zip = ZipWriter::new(File::create(&path).unwrap());
        let options = SimpleFileOptions::default();
        zip.add_directory("OEBPS/", options).unwrap();
        zip.start_file("OEBPS/one.xhtml", options).unwrap();
        zip.write_all(b"<html/>").unwrap();
        zip.finish().unwrap();
        path
    }

    #[test]
    fn lists_files_and_reads_them_by_name() {
        let dir = tempfile::tempdir().unwrap();
        let mut book = BookFile::open(&book(dir.path())).unwrap();
        assert_eq!(
            book.entries().unwrap(),
            [Entry {
                name: "OEBPS/one.xhtml".to_owned(),
                size: 7,
            }]
        );
        assert_eq!(book.read("OEBPS/one.xhtml").unwrap(), b"<html/>");
    }

    #[test]
    fn names_never_leave_the_zip() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret.txt"), "secret").unwrap();
        let mut book = BookFile::open(&book(dir.path())).unwrap();
        assert!(book.read("../secret.txt").is_err());
        assert!(book.read("/etc/hosts").is_err());
    }
}
