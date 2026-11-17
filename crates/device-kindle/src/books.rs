//! Books in `documents/`, with what their headers and sidecars say.
//!
//! Like calibre's Kindle drivers, every supported file under `documents/` is
//! a book and `.sdr` sidecar folders are skipped. Headers are read from the
//! start of each file only, and a [`MetaCache`] keeps them between scans so
//! an MTP Kindle isn't read in full every time it connects.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::collections;
use crate::krds::{self, ReadingData};
use crate::mobi::{self, MobiHeader};
use crate::storage::{Entry, Storage, join, split};
use crate::{DOCUMENTS_DIR, Result};

/// Extensions a Kindle reads, from calibre's `KINDLE2.FORMATS`.
pub const BOOK_EXTENSIONS: &[&str] = &[
    "azw", "azw3", "mobi", "prc", "azw1", "tpz", "azw4", "kfx", "pobi", "pdf", "txt",
];
/// Extensions with MOBI headers.
const MOBI_EXTENSIONS: &[&str] = &["azw", "azw3", "mobi", "prc", "azw4"];

/// Header fields worth keeping between scans.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct BookMeta {
    pub size: u64,
    pub modified: Option<i64>,
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub asin: Option<String>,
    pub cdetype: Option<String>,
    pub text_length: u32,
    pub drm: bool,
}

/// Book metadata by device path, checked against size and date.
pub type MetaCache = HashMap<String, BookMeta>;

#[derive(Debug, Clone, PartialEq)]
pub struct KindleBook {
    /// Path from the storage root, e.g. `documents/Title - Author.azw3`.
    pub path: String,
    pub meta: BookMeta,
    pub reading: ReadingData,
    /// From `system/collections.json` on older firmware.
    pub collections: Vec<String>,
}

impl KindleBook {
    pub fn extension(&self) -> String {
        extension(&self.path)
    }

    pub fn title(&self) -> String {
        self.meta
            .title
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| title_from_file_name(&self.path))
    }

    pub fn thumbnail_name(&self) -> Option<String> {
        MobiHeader {
            asin: self.meta.asin.clone(),
            cdetype: self.meta.cdetype.clone(),
            ..MobiHeader::default()
        }
        .thumbnail_name()
    }

    /// Where the book's last page read is, 0 to 100, when known. Positions
    /// in MOBI and KF8 books are byte offsets into the text; failing that,
    /// the reading timer's share read, which counts re-reading, is capped.
    pub fn percent_read(&self) -> Option<u8> {
        let position = self
            .reading
            .last
            .as_ref()
            .or(self.reading.furthest.as_ref())
            .and_then(krds::Position::offset);
        let percent = match position {
            Some(offset) if self.meta.text_length > 0 => {
                offset as f64 / f64::from(self.meta.text_length) * 100.0
            }
            _ => self.reading.total_percent? * 100.0,
        };
        // Clamped to 0..=100 first, so the cast can't truncate or wrap.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(percent.clamp(0.0, 100.0).round() as u8)
    }

    /// When the book was last read, in milliseconds since the epoch.
    pub fn last_read_ms(&self) -> Option<i64> {
        self.reading.last.as_ref().and_then(|p| p.time_ms)
    }
}

fn is_sdr(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("sdr"))
}

pub fn extension(path: &str) -> String {
    let (_, name) = split(path);
    name.rsplit_once('.')
        .map_or_default(|(_, ext)| ext.to_ascii_lowercase())
}

/// A title from the file name. Books delivered by Amazon are named
/// `Title-asin_X-type_EBOK-v_N`, as calibre's `WIRELESS_FILE_NAME_PATTERN`
/// expects.
pub fn title_from_file_name(path: &str) -> String {
    let (_, name) = split(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    match stem.find("-asin_") {
        Some(at) if at > 0 => stem[..at].to_owned(),
        _ => stem.to_owned(),
    }
}

/// The sidecar folder for a book: `documents/Book.azw3` → `documents/Book.sdr`.
pub fn sidecar_dir(book_path: &str) -> String {
    let (dir, name) = split(book_path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    join(&[dir, &format!("{stem}.sdr")])
}

/// Reads a book's headers from the start of the file.
pub fn read_meta(storage: &dyn Storage, path: &str, entry: &Entry) -> BookMeta {
    let mut meta = BookMeta {
        size: entry.size,
        modified: entry.modified,
        ..BookMeta::default()
    };
    if !MOBI_EXTENSIONS.contains(&extension(path).as_str()) {
        return meta;
    }
    let Ok(mut prefix) = storage.read_prefix(path, mobi::PREFIX_LEN) else {
        return meta;
    };
    if let Some(needed) = mobi::needed_len(&prefix)
        && needed > prefix.len()
        && (needed as u64) < entry.size
        && let Ok(longer) = storage.read_prefix(path, needed as u64)
    {
        prefix = longer;
    }
    if let Some(header) = mobi::parse(&prefix) {
        meta.title = header.title;
        meta.authors = header.authors;
        meta.asin = header.asin;
        meta.cdetype = header.cdetype;
        meta.text_length = header.text_length;
        meta.drm = header.encryption != 0;
    }
    meta
}

/// Reads the KRDS sidecars of a book whose sidecar folder holds `files`.
pub fn read_reading(storage: &dyn Storage, sdr: &str, files: &[Entry]) -> ReadingData {
    let data: Vec<Vec<u8>> = files
        .iter()
        .filter(|e| !e.is_dir && krds::is_sidecar(&e.name))
        .filter_map(|e| storage.read(&join(&[sdr, &e.name])).ok())
        .collect();
    krds::reading_data(data.iter().map(Vec::as_slice))
}

/// Every book in `documents/`, updating `cache` with what was read.
pub fn scan(storage: &dyn Storage, cache: &mut MetaCache) -> Result<Vec<KindleBook>> {
    let collections = collections::read(storage);
    let mut books = Vec::new();
    let mut seen = Vec::new();
    let mut dirs = vec![DOCUMENTS_DIR.to_owned()];
    while let Some(dir) = dirs.pop() {
        let entries = storage.list(&dir)?;
        let sidecars: HashSet<&str> = entries
            .iter()
            .filter(|e| e.is_dir && is_sdr(&e.name))
            .map(|e| e.name.as_str())
            .collect();
        for entry in &entries {
            let path = join(&[&dir, &entry.name]);
            if entry.name.starts_with('.') {
                continue;
            }
            if entry.is_dir {
                if !is_sdr(&entry.name) {
                    dirs.push(path);
                }
                continue;
            }
            // The clippings file is a .txt, but it's the Kindle's, not a book.
            if path == crate::CLIPPINGS_FILE
                || !BOOK_EXTENSIONS.contains(&extension(&path).as_str())
            {
                continue;
            }
            let meta = match cache.get(&path) {
                Some(m) if m.size == entry.size && m.modified == entry.modified => m.clone(),
                _ => read_meta(storage, &path, entry),
            };
            cache.insert(path.clone(), meta.clone());
            seen.push(path.clone());
            let sdr = sidecar_dir(&path);
            let (_, sdr_name) = split(&sdr);
            let reading = if sidecars.contains(sdr_name) {
                let files = storage.list(&sdr)?;
                read_reading(storage, &sdr, &files)
            } else {
                ReadingData::default()
            };
            books.push(KindleBook {
                collections: collections::for_path(&collections, &path),
                path,
                meta,
                reading,
            });
        }
    }
    cache.retain(|path, _| seen.contains(path));
    books.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(books)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::DriveStorage;

    #[test]
    fn names() {
        assert_eq!(
            title_from_file_name("documents/The Book-asin_B00ABCDEFG-type_EBOK-v_0.azw"),
            "The Book"
        );
        assert_eq!(title_from_file_name("documents/x/Notes.pdf"), "Notes");
        assert_eq!(sidecar_dir("documents/x/Book.azw3"), "documents/x/Book.sdr");
        assert_eq!(extension("documents/A.AZW3"), "azw3");
    }

    #[test]
    fn scans_documents() {
        let dir = tempfile::tempdir().unwrap();
        let s = DriveStorage::new(dir.path());
        let azw3 = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/amazon/book.azw3"),
        )
        .unwrap();
        s.write("documents/Book.azw3", &azw3, &mut |_, _| {})
            .unwrap();
        s.write("documents/sub/Notes.pdf", b"%PDF", &mut |_, _| {})
            .unwrap();
        s.write("documents/My Clippings.txt", b"", &mut |_, _| {})
            .unwrap();
        s.write("documents/Book.sdr/Book.apnx", b"x", &mut |_, _| {})
            .unwrap();
        let (f, _) = crate::krds::tests::sample();
        s.write("documents/Book.sdr/Book.azw3f", &f, &mut |_, _| {})
            .unwrap();

        let mut cache = MetaCache::new();
        let books = scan(&s, &mut cache).unwrap();
        let paths: Vec<_> = books.iter().map(|b| b.path.as_str()).collect();
        assert_eq!(paths, ["documents/Book.azw3", "documents/sub/Notes.pdf"]);
        let book = &books[0];
        assert!(book.meta.text_length > 0);
        assert!(!book.title().is_empty());
        assert_eq!(book.reading.last.as_ref().unwrap().offset(), Some(4600));
        assert!(book.percent_read().is_some());
        assert_eq!(books[1].title(), "Notes");
        assert_eq!(cache.len(), 2);

        // A second scan reuses the cache; removed books leave it.
        s.remove("documents/sub/Notes.pdf").unwrap();
        assert_eq!(scan(&s, &mut cache).unwrap().len(), 1);
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn percent_from_position_or_timer() {
        let mut book = KindleBook {
            path: "documents/a.azw3".into(),
            meta: BookMeta {
                text_length: 10_000,
                ..BookMeta::default()
            },
            reading: ReadingData::default(),
            collections: vec![],
        };
        assert_eq!(book.percent_read(), None);
        book.reading.total_percent = Some(1.7);
        assert_eq!(book.percent_read(), Some(100));
        book.reading.last = Some(crate::krds::Position {
            position: "2500".into(),
            time_ms: None,
        });
        assert_eq!(book.percent_read(), Some(25));
    }
}
