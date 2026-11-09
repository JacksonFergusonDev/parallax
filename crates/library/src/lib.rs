//! The local library: imported EPUBs and comics (CBZ) kept unmodified in a
//! managed folder, with their (editable) metadata in SQLite. Edits are applied
//! only when a copy is exported for a device, so originals are never
//! rewritten.
//!
//! Layout under the library root:
//! ```text
//! library.sqlite
//! books/<id>/book.epub     original file, byte for byte (book.cbz for comics)
//! books/<id>/cover.jpg     thumbnail for the grid (optional)
//! books/<id>/custom-cover.jpg  user-chosen cover, applied on export (optional)
//! snapshots/                the database before big changes (see `snapshots`)
//! removed/<stamp>-<id>/     removed books' folders, held for 30 days
//! ```

mod authors;
mod color;
mod comics;
mod format;
mod rules;
mod schema;
mod snapshots;
mod stores;

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use chrono::Utc;
use parallax_epub::{Contributor, Cover, Epub, Extent, Identifier, Metadata, MetadataEdit};
use rayon::prelude::*;
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub use authors::normalize_authors;
pub use color::{dominant_color, dominant_colors};
pub use comics::COMICS_KIND;
pub use format::{Direction, Format, Kind};
pub use rules::{Rule, RuleKind};
pub use snapshots::{REMOVED_KEPT_DAYS, SNAPSHOTS_KEPT, Snapshot, SnapshotRestore};
pub use stores::{Annotation, HighlightDraft, ReaderHighlight, ReaderPosition, Reading};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("library database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Epub(#[from] parallax_epub::Error),
    #[error(transparent)]
    Comic(#[from] parallax_comic::Error),
    #[error("{0} isn't a comic, so it has no reading direction")]
    NotAComic(i64),
    #[error("no book with id {0}")]
    NotFound(i64),
    #[error("title can't be empty")]
    EmptyTitle,
    #[error("not a supported image: {0}")]
    BadImage(String),
    #[error("can't go back to that snapshot: {0}")]
    CannotRestore(String),
}

pub type Result<T> = std::result::Result<T, Error>;

const COVER_HEIGHT: u32 = 600;
/// Custom covers are stored at most this tall: the Kobo Libra 2's screen
/// height, which is also the tallest current Kobo screen.
pub const CUSTOM_COVER_MAX_HEIGHT: u32 = 1680;
/// Below this height a cover looks soft on current Kobos; the cover picker
/// uses the same line.
const SMALL_COVER_HEIGHT: u32 = 1200;

/// How well a cover will fill an e-reader's screen.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverQuality {
    /// Visibly soft or pixellated.
    Low,
    /// Slightly soft on the sharpest screens.
    Decent,
    /// At least as tall as the tallest current Kobo screen.
    Excellent,
}

impl CoverQuality {
    pub fn of_height(height: u32) -> Self {
        match height {
            h if h >= CUSTOM_COVER_MAX_HEIGHT => Self::Excellent,
            h if h >= SMALL_COVER_HEIGHT => Self::Decent,
            _ => Self::Low,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Book {
    pub id: i64,
    pub title: String,
    pub authors: Vec<String>,
    /// `None` when not read from the EPUB yet; exports then leave the EPUB's
    /// contributors as they are.
    pub contributors: Option<Vec<Contributor>>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    /// ISO 8601, to whatever precision the EPUB gives.
    pub published: Option<String>,
    pub description: Option<String>,
    pub identifiers: Vec<Identifier>,
    /// `None` when the EPUB couldn't be measured.
    pub words: Option<u64>,
    pub pages: Option<u64>,
    /// The pages were worked out from the words, not listed by the publisher.
    pub pages_estimated: bool,
    pub original_name: String,
    pub file_size: u64,
    /// The stored file: EPUB, or CBZ for comics.
    pub format: Format,
    pub kind: Kind,
    /// Which way pages turn; books are always left to right.
    pub direction: Direction,
    pub has_cover: bool,
    /// The cover was chosen by the user rather than taken from the EPUB.
    pub custom_cover: bool,
    /// The custom cover came from the upscaler.
    pub cover_upscaled: bool,
    pub added_at: String,
    pub modified_at: String,
}

impl Book {
    pub fn edit(&self) -> MetadataEdit {
        MetadataEdit {
            title: self.title.clone(),
            authors: self.authors.clone(),
            series: self.series.clone(),
            series_index: self.series_index.clone(),
            tags: self.tags.clone(),
            contributors: self.contributors.clone(),
        }
    }

    pub fn isbn(&self) -> Option<&str> {
        self.identifier(&["isbn"])
    }

    /// Amazon's id for the book: calibre stores one read from a Kindle file
    /// as `mobi-asin:` and one found on Amazon as `amazon:`.
    pub fn asin(&self) -> Option<&str> {
        self.identifier(&["mobi-asin", "amazon", "asin"])
    }

    /// The first identifier with one of these schemes, given either as the
    /// scheme or as a `scheme:` prefix on the value (how calibre writes some).
    fn identifier(&self, schemes: &[&str]) -> Option<&str> {
        self.identifiers.iter().find_map(|i| {
            if let Some(s) = i.scheme.as_deref() {
                schemes
                    .iter()
                    .any(|w| s.eq_ignore_ascii_case(w))
                    .then_some(i.value.as_str())
            } else {
                let (s, value) = i.value.split_once(':')?;
                schemes
                    .iter()
                    .any(|w| s.eq_ignore_ascii_case(w))
                    .then_some(value.trim())
            }
        })
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportOutcome {
    Added {
        book: Book,
        /// Another book with the same ISBN or title and first author.
        possible_duplicate_of: Option<i64>,
    },
    /// The exact same file is already in the library.
    AlreadyInLibrary { book: Book },
}

/// Where a book has been sent.
#[derive(Debug, Clone, Serialize)]
pub struct DeviceCopy {
    pub book_id: i64,
    pub device_serial: String,
    pub device_path: String,
    pub content_id: String,
    pub sent_at: String,
    pub comic_profile: Option<String>,
}

pub struct Library {
    root: PathBuf,
    conn: Connection,
}

impl Library {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self> {
        let root = root.into();
        fs::create_dir_all(root.join("books")).map_err(|e| io_err(&root, e))?;
        let conn = Connection::open(root.join("library.sqlite"))?;
        schema::migrate(&conn)?;
        let library = Self { root, conn };
        library.backfill_contributors();
        library.backfill_extents();
        Ok(library)
    }

    /// Reads contributors for books imported before they were tracked. Best
    /// effort: a book whose EPUB can't be read stays unknown (and untouched).
    fn backfill_contributors(&self) {
        let Ok(mut stmt) = self
            .conn
            .prepare("SELECT id FROM books WHERE contributors IS NULL AND format = 'epub'")
        else {
            return;
        };
        let ids: Vec<i64> = stmt
            .query_map([], |r| r.get(0))
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default();
        for id in ids {
            if let Ok(epub) = Epub::open(self.book_file(id)) {
                let _ = self.conn.execute(
                    "UPDATE books SET contributors = ?1 WHERE id = ?2",
                    params![json(&epub.metadata().contributors), id],
                );
            }
        }
    }

    /// Reads the publication date and length of books imported before they
    /// were tracked, across cores as this reads every book in full. Best
    /// effort: a book that can't be measured stays unknown.
    fn backfill_extents(&self) {
        let Ok(mut stmt) = self
            .conn
            .prepare("SELECT id FROM books WHERE words IS NULL AND format = 'epub'")
        else {
            return;
        };
        let books: Vec<(i64, PathBuf)> = stmt
            .query_map([], |r| r.get(0))
            .map(|rows| rows.flatten().collect::<Vec<i64>>())
            .unwrap_or_default()
            .into_iter()
            .map(|id| (id, self.book_file(id)))
            .collect();
        let measured: Vec<(i64, Option<String>, Extent)> = books
            .par_iter()
            .filter_map(|(id, file)| {
                let epub = Epub::open(file).ok()?;
                Some((*id, epub.metadata().published, epub.extent().ok()?))
            })
            .collect();
        let Ok(tx) = self.conn.unchecked_transaction() else {
            return;
        };
        for (id, published, extent) in measured {
            let _ = tx.execute(
                "UPDATE books SET published = ?1, words = ?2, pages = ?3, pages_estimated = ?4
                 WHERE id = ?5",
                params![
                    published,
                    extent.words,
                    extent.pages,
                    extent.pages_estimated,
                    id
                ],
            );
        }
        let _ = tx.commit();
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Imports an EPUB, or a comic (CBZ, CBR or CB7, which is kept as CBZ).
    pub fn import(&mut self, path: &Path) -> Result<ImportOutcome> {
        if let Some(format) = comics::comic_format(path) {
            return self.import_any_comic(path, format);
        }
        self.import_from(path, path)
    }

    /// Imports `epub`, a conversion of `original`. The book is identified by
    /// `original`, so adding that file again finds it even though conversion
    /// needn't produce identical bytes.
    pub fn import_converted(&mut self, epub: &Path, original: &Path) -> Result<ImportOutcome> {
        self.import_from(epub, original)
    }

    /// The library's book made from exactly the file at `path`, if any.
    pub fn book_from_file(&self, path: &Path) -> Result<Option<Book>> {
        self.find_by_hash(&hash_file(path)?.0)
    }

    fn import_from(&mut self, path: &Path, original: &Path) -> Result<ImportOutcome> {
        let (hash, _) = hash_file(original)?;
        if let Some(book) = self.find_by_hash(&hash)? {
            return Ok(ImportOutcome::AlreadyInLibrary { book });
        }
        let size = fs::metadata(path).map_err(|e| io_err(path, e))?.len();
        let epub = Epub::open(path)?;
        let meta = epub.metadata();
        let extent = epub.extent().ok();
        let original_name = original
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let title = meta.title.clone().unwrap_or_else(|| {
            original
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned()
        });
        let authors = normalize_authors(&meta.authors);
        let now = now();

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO books (title, authors, series, series_index, tags, language, publisher, description,
                                identifiers, file_hash, file_size, original_name, has_cover, added_at, modified_at,
                                contributors, published, words, pages, pages_estimated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?13, ?14, ?15, ?16, ?17, ?18)",
            params![
                title,
                json(&authors),
                meta.series,
                meta.series_index,
                json(&meta.tags),
                meta.language,
                meta.publisher,
                meta.description,
                json(&meta.identifiers),
                hash,
                size,
                original_name,
                now,
                json(&meta.contributors),
                meta.published,
                extent.map(|e| e.words),
                extent.and_then(|e| e.pages),
                extent.is_none_or(|e| e.pages_estimated),
            ],
        )?;
        let id = tx.last_insert_rowid();
        let dir = book_dir(&self.root, id);
        let stored = (|| -> Result<bool> {
            fs::create_dir_all(&dir).map_err(|e| io_err(&dir, e))?;
            let dest = dir.join("book.epub");
            fs::copy(path, &dest).map_err(|e| io_err(&dest, e))?;
            Ok(epub
                .cover()
                .ok()
                .flatten()
                .is_some_and(|c| write_thumbnail(&c.data, &dir.join("cover.jpg"))))
        })();
        match stored {
            Ok(has_cover) => {
                tx.execute(
                    "UPDATE books SET has_cover = ?1 WHERE id = ?2",
                    params![has_cover, id],
                )?;
                tx.commit()?;
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(e);
            }
        }
        let book = self.book(id)?;
        let possible_duplicate_of = self.find_similar(&book)?;
        Ok(ImportOutcome::Added {
            book,
            possible_duplicate_of,
        })
    }

    pub fn books(&self) -> Result<Vec<Book>> {
        let mut stmt = self
            .conn
            .prepare("SELECT * FROM books ORDER BY title COLLATE NOCASE, id")?;
        let books = stmt
            .query_map([], book_from_row)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(books)
    }

    pub fn book(&self, id: i64) -> Result<Book> {
        self.conn
            .query_row("SELECT * FROM books WHERE id = ?1", [id], book_from_row)
            .optional()?
            .ok_or(Error::NotFound(id))
    }

    pub fn update(&mut self, id: i64, edit: &MetadataEdit) -> Result<Book> {
        self.write_metadata(id, edit)
    }

    /// Applies an enrichment and its source in the same transaction.
    pub fn update_with_details(
        &mut self,
        id: i64,
        edit: &MetadataEdit,
        identifiers: &[Identifier],
        publisher: Option<&str>,
        description: Option<&str>,
    ) -> Result<Book> {
        let tx = self.conn.unchecked_transaction()?;
        self.write_metadata(id, edit)?;
        let book = self.write_details(id, identifiers, publisher, description, None)?;
        tx.commit()?;
        Ok(book)
    }

    fn write_metadata(&self, id: i64, edit: &MetadataEdit) -> Result<Book> {
        let title = edit.title.trim();
        if title.is_empty() {
            return Err(Error::EmptyTitle);
        }
        let clean = |v: &[String]| -> Vec<String> {
            v.iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect()
        };
        let opt = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        };
        let changed = self.conn.execute(
            "UPDATE books SET title = ?1, authors = ?2, series = ?3, series_index = ?4, tags = ?5, modified_at = ?6
             WHERE id = ?7",
            params![
                title,
                json(&clean(&edit.authors)),
                opt(&edit.series),
                opt(&edit.series).and(opt(&edit.series_index)),
                json(&clean(&edit.tags)),
                now(),
                id
            ],
        )?;
        if changed == 0 {
            return Err(Error::NotFound(id));
        }
        if let Some(contributors) = &edit.contributors {
            let clean: Vec<Contributor> = contributors
                .iter()
                .filter(|c| !c.name.trim().is_empty())
                .map(|c| Contributor {
                    name: c.name.trim().to_owned(),
                    role: match c.role.trim() {
                        "" => "ctb".to_owned(),
                        r => r.to_lowercase(),
                    },
                })
                .collect();
            self.conn.execute(
                "UPDATE books SET contributors = ?1 WHERE id = ?2",
                params![json(&clean), id],
            )?;
        }
        self.book(id)
    }

    /// Replaces a book's tags without marking it edited, for tidying names
    /// (the Kobo doesn't show tags, so copies sent there stay current).
    pub fn retag(&mut self, id: i64, tags: &[String]) -> Result<Book> {
        let changed = self.conn.execute(
            "UPDATE books SET tags = ?1 WHERE id = ?2",
            params![json(&tags), id],
        )?;
        if changed == 0 {
            return Err(Error::NotFound(id));
        }
        self.book(id)
    }

    /// Fills in what the library doesn't know yet: identifiers with a scheme
    /// it lacks, and an empty publisher, description or language. Nothing it
    /// already has is replaced.
    pub fn fill_details(
        &mut self,
        id: i64,
        identifiers: &[Identifier],
        publisher: Option<&str>,
        description: Option<&str>,
        language: Option<&str>,
    ) -> Result<Book> {
        self.write_details(id, identifiers, publisher, description, language)
    }

    fn write_details(
        &self,
        id: i64,
        identifiers: &[Identifier],
        publisher: Option<&str>,
        description: Option<&str>,
        language: Option<&str>,
    ) -> Result<Book> {
        let mut book = self.book(id)?;
        for new in identifiers {
            let Some(scheme) = new.scheme.as_deref() else {
                continue;
            };
            if book.identifier(&[scheme]).is_none() && !new.value.trim().is_empty() {
                book.identifiers.push(new.clone());
            }
        }
        let fill = |current: Option<String>, new: Option<&str>| {
            current.filter(|c| !c.trim().is_empty()).or_else(|| {
                new.map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
            })
        };
        self.conn.execute(
            "UPDATE books SET identifiers = ?1, publisher = ?2, description = ?3, language = ?4
             WHERE id = ?5",
            params![
                json(&book.identifiers),
                fill(book.publisher, publisher),
                fill(book.description, description),
                fill(book.language, language),
                id
            ],
        )?;
        self.book(id)
    }

    /// Removes a book. Its files are held for [`REMOVED_KEPT_DAYS`], so going
    /// back to a snapshot from before this brings it back.
    pub fn remove(&mut self, id: i64) -> Result<()> {
        let book = self.book(id)?;
        let hash: String =
            self.conn
                .query_row("SELECT file_hash FROM books WHERE id = ?1", [id], |r| {
                    r.get(0)
                })?;
        self.conn.execute("DELETE FROM books WHERE id = ?1", [id])?;
        self.hold_book_dir(id, &hash, &book.title)
    }

    /// The unmodified original: `book.epub`, or `book.cbz` for comics.
    pub fn book_file(&self, id: i64) -> PathBuf {
        format::stored_file(&self.book_dir(id))
    }

    /// Width and height of the cover sent to devices: the custom one if set,
    /// else the EPUB's own.
    pub fn cover_size(&self, id: i64) -> Option<(u32, u32)> {
        self.book_files(id).cover_size()
    }

    /// Where a book's files are, for reading them on other threads: the
    /// database connection can't be shared, but paths can.
    pub fn book_files(&self, id: i64) -> BookFiles {
        BookFiles {
            book: self.book_file(id),
            custom_cover: self.custom_cover_file(id),
        }
    }

    pub fn cover_file(&self, id: i64) -> Option<PathBuf> {
        let path = self.book_dir(id).join("cover.jpg");
        path.is_file().then_some(path)
    }

    /// Writes the book with its current library metadata (and custom cover)
    /// applied to `out`.
    /// Comics get a CBZ with their metadata in its ComicInfo.xml.
    pub fn export(&self, id: i64, out: &Path) -> Result<Book> {
        let book = self.book(id)?;
        if book.format == Format::Cbz {
            self.export_comic(&book, out)?;
            return Ok(book);
        }
        let epub = Epub::open(self.book_file(id))?;
        let cover = if book.custom_cover {
            let jpeg = fs::read(self.custom_cover_file(id))
                .map_err(|e| io_err(&self.custom_cover_file(id), e))?;
            Some(match epub.cover_media_type().as_deref() {
                // Match a PNG original so it is replaced in place.
                Some("image/png") => Cover {
                    data: reencode(&jpeg, image::ImageFormat::Png)?,
                    media_type: "image/png".into(),
                },
                _ => Cover {
                    data: jpeg,
                    media_type: "image/jpeg".into(),
                },
            })
        } else {
            None
        };
        epub.write(&book.edit(), cover.as_ref(), out)?;
        Ok(book)
    }

    /// Uses `image` (any common format) as the book's cover from now on.
    pub fn set_cover(&mut self, id: i64, image: &[u8]) -> Result<Book> {
        self.book(id)?;
        let img = image::load_from_memory(image).map_err(|e| Error::BadImage(e.to_string()))?;
        let img = if img.height() > CUSTOM_COVER_MAX_HEIGHT {
            img.resize(
                u32::MAX,
                CUSTOM_COVER_MAX_HEIGHT,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            img
        };
        let path = self.custom_cover_file(id);
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90)
            .encode_image(&img.to_rgb8())
            .map_err(|e| Error::BadImage(e.to_string()))?;
        fs::write(&path, &jpeg).map_err(|e| io_err(&path, e))?;
        let has_thumb = write_thumbnail(&jpeg, &self.book_dir(id).join("cover.jpg"));
        self.conn.execute(
            "UPDATE books SET custom_cover = 1, cover_upscaled = 0, has_cover = ?1, modified_at = ?2 WHERE id = ?3",
            params![has_thumb, now(), id],
        )?;
        self.book(id)
    }

    /// Uses the upscaler's output as the book's cover, remembering where it
    /// came from.
    pub fn set_upscaled_cover(&mut self, id: i64, image: &[u8]) -> Result<Book> {
        self.set_cover(id, image)?;
        self.conn.execute(
            "UPDATE books SET cover_upscaled = 1 WHERE id = ?1",
            params![id],
        )?;
        self.book(id)
    }

    /// The best image of the book's current cover: the custom one if set, else
    /// the EPUB's own. Not the grid thumbnail, which is much smaller.
    pub fn cover_source(&self, id: i64) -> Result<Option<Vec<u8>>> {
        let custom = self.custom_cover_file(id);
        if custom.is_file() {
            return Ok(Some(fs::read(&custom).map_err(|e| io_err(&custom, e))?));
        }
        if self.book(id)?.format == Format::Cbz {
            return self.comic_cover(id);
        }
        Ok(Epub::open(self.book_file(id))?.cover()?.map(|c| c.data))
    }

    /// Goes back to the cover inside the EPUB.
    pub fn reset_cover(&mut self, id: i64) -> Result<Book> {
        self.book(id)?;
        let _ = fs::remove_file(self.custom_cover_file(id));
        let thumb = self.book_dir(id).join("cover.jpg");
        let _ = fs::remove_file(&thumb);
        let original = if self.book(id)?.format == Format::Cbz {
            self.comic_cover(id).ok().flatten()
        } else {
            Epub::open(self.book_file(id))?
                .cover()
                .ok()
                .flatten()
                .map(|c| c.data)
        };
        let has_cover = original.is_some_and(|c| write_thumbnail(&c, &thumb));
        self.conn.execute(
            "UPDATE books SET custom_cover = 0, cover_upscaled = 0, has_cover = ?1, modified_at = ?2 WHERE id = ?3",
            params![has_cover, now(), id],
        )?;
        self.book(id)
    }

    fn custom_cover_file(&self, id: i64) -> PathBuf {
        self.book_dir(id).join("custom-cover.jpg")
    }

    pub fn record_device_copy(
        &self,
        book_id: i64,
        device_serial: &str,
        device_path: &str,
        content_id: &str,
    ) -> Result<()> {
        self.record_device_copy_profile(book_id, device_serial, device_path, content_id, None)
    }

    pub fn record_device_copy_profile(
        &self,
        book_id: i64,
        device_serial: &str,
        device_path: &str,
        content_id: &str,
        comic_profile: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO device_copies
                 (book_id, device_serial, device_path, content_id, sent_at, comic_profile)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                book_id,
                device_serial,
                device_path,
                content_id,
                now(),
                comic_profile
            ],
        )?;
        Ok(())
    }

    pub fn device_copies(&self, device_serial: &str) -> Result<Vec<DeviceCopy>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, device_serial, device_path, content_id, sent_at, comic_profile FROM device_copies WHERE device_serial = ?1",
        )?;
        let copies = stmt
            .query_map([device_serial], |r| {
                Ok(DeviceCopy {
                    book_id: r.get(0)?,
                    device_serial: r.get(1)?,
                    device_path: r.get(2)?,
                    content_id: r.get(3)?,
                    sent_at: r.get(4)?,
                    comic_profile: r.get(5)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(copies)
    }

    /// Forgets that a book is on a device (after removing it there).
    pub fn remove_device_copy(&self, book_id: i64, device_serial: &str) -> Result<()> {
        self.conn.execute(
            "DELETE FROM device_copies WHERE book_id = ?1 AND device_serial = ?2",
            params![book_id, device_serial],
        )?;
        Ok(())
    }

    fn book_dir(&self, id: i64) -> PathBuf {
        book_dir(&self.root, id)
    }

    fn find_by_hash(&self, hash: &str) -> Result<Option<Book>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM books WHERE file_hash = ?1",
                [hash],
                book_from_row,
            )
            .optional()?)
    }

    /// A different book that is probably the same work: same ISBN, or same
    /// title and first author ignoring case and punctuation.
    fn find_similar(&self, book: &Book) -> Result<Option<i64>> {
        self.find_matching(&book.title, &book.authors, book.isbn(), Some(book.id))
    }

    /// A library book with the same ISBN, or the same title and first
    /// author, as an EPUB's metadata.
    pub fn find_similar_to(&self, meta: &Metadata) -> Result<Option<i64>> {
        let isbn = meta
            .identifiers
            .iter()
            .find(|i| i.scheme.as_deref() == Some("isbn"))
            .map(|i| i.value.as_str());
        let title = meta.title.as_deref().unwrap_or_default();
        self.find_matching(title, &normalize_authors(&meta.authors), isbn, None)
    }

    fn find_matching(
        &self,
        title: &str,
        authors: &[String],
        isbn: Option<&str>,
        exclude: Option<i64>,
    ) -> Result<Option<i64>> {
        let key = similarity_key(title, authors);
        Ok(self
            .books()?
            .into_iter()
            .filter(|other| Some(other.id) != exclude)
            .find(|other| {
                (isbn.is_some() && isbn == other.isbn())
                    || similarity_key(&other.title, &other.authors) == key
            })
            .map(|other| other.id))
    }
}

fn book_dir(root: &Path, id: i64) -> PathBuf {
    root.join("books").join(id.to_string())
}

fn similarity_key(title: &str, authors: &[String]) -> String {
    let norm = |s: &str| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    format!(
        "{}|{}",
        norm(title),
        norm(authors.first().map_or("", String::as_str))
    )
}

fn book_from_row(row: &Row) -> rusqlite::Result<Book> {
    let list = |col: &str| -> rusqlite::Result<Vec<String>> {
        Ok(serde_json::from_str(&row.get::<_, String>(col)?).unwrap_or_default())
    };
    Ok(Book {
        id: row.get("id")?,
        title: row.get("title")?,
        authors: list("authors")?,
        series: row.get("series")?,
        series_index: row.get("series_index")?,
        tags: list("tags")?,
        contributors: row
            .get::<_, Option<String>>("contributors")?
            .and_then(|s| serde_json::from_str(&s).ok()),
        language: row.get("language")?,
        publisher: row.get("publisher")?,
        published: row.get("published")?,
        description: row.get("description")?,
        identifiers: serde_json::from_str(&row.get::<_, String>("identifiers")?)
            .unwrap_or_default(),
        words: row.get("words")?,
        pages: row.get("pages")?,
        pages_estimated: row.get("pages_estimated")?,
        original_name: row.get("original_name")?,
        file_size: row.get("file_size")?,
        format: row.get("format")?,
        kind: row.get("kind")?,
        direction: row.get("direction")?,
        has_cover: row.get("has_cover")?,
        custom_cover: row.get("custom_cover")?,
        cover_upscaled: row.get("cover_upscaled")?,
        added_at: row.get("added_at")?,
        modified_at: row.get("modified_at")?,
    })
}

/// A book's files on disk; see [`Library::book_files`].
#[derive(Debug, Clone)]
pub struct BookFiles {
    pub book: PathBuf,
    custom_cover: PathBuf,
}

impl BookFiles {
    /// See [`Library::cover_size`].
    pub fn cover_size(&self) -> Option<(u32, u32)> {
        if self.custom_cover.is_file() {
            return image::image_dimensions(&self.custom_cover).ok();
        }
        if self.book.extension().is_some_and(|e| e == "cbz") {
            let mut comic = parallax_comic::Cbz::open(&self.book).ok()?;
            let index = comic.cover_index();
            return comic.page_dimensions(index).ok()?;
        }
        let cover = Epub::open(&self.book).ok()?.cover().ok()??;
        image::ImageReader::new(std::io::Cursor::new(cover.data))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()
    }
}

/// Decodes `data` and writes a JPEG thumbnail. Returns false if the image
/// can't be decoded (the book is still imported, just without a grid cover).
fn write_thumbnail(data: &[u8], out: &Path) -> bool {
    let Ok(img) = image::load_from_memory(data) else {
        return false;
    };
    let img = if img.height() > COVER_HEIGHT {
        img.resize(
            u32::MAX,
            COVER_HEIGHT,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    img.to_rgb8()
        .save_with_format(out, image::ImageFormat::Jpeg)
        .is_ok()
}

fn reencode(data: &[u8], format: image::ImageFormat) -> Result<Vec<u8>> {
    let img = image::load_from_memory(data).map_err(|e| Error::BadImage(e.to_string()))?;
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, format)
        .map_err(|e| Error::BadImage(e.to_string()))?;
    Ok(out.into_inner())
}

fn hash_file(path: &Path) -> Result<(String, u64)> {
    let mut file = fs::File::open(path).map_err(|e| io_err(path, e))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buf).map_err(|e| io_err(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        size += n as u64;
    }
    let hex = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((hex, size))
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("serialising plain data")
}

fn now() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn io_err(path: &Path, source: io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_ids(ids: &[(Option<&str>, &str)]) -> Book {
        Book {
            id: 1,
            title: "T".into(),
            authors: vec![],
            contributors: None,
            series: None,
            series_index: None,
            tags: vec![],
            language: None,
            publisher: None,
            published: None,
            description: None,
            identifiers: ids
                .iter()
                .map(|(scheme, value)| Identifier {
                    scheme: scheme.map(str::to_owned),
                    value: (*value).to_owned(),
                })
                .collect(),
            words: None,
            pages: None,
            pages_estimated: true,
            original_name: "t.epub".into(),
            file_size: 1,
            format: Format::Epub,
            kind: Kind::Book,
            direction: Direction::Ltr,
            has_cover: false,
            custom_cover: false,
            cover_upscaled: false,
            added_at: String::new(),
            modified_at: String::new(),
        }
    }

    #[test]
    fn identifiers_read_from_scheme_or_value_prefix() {
        let calibre = with_ids(&[
            (None, "calibre:106"),
            (None, "mobi-asin:B0736185ZL"),
            (None, "isbn:9780441013593"),
        ]);
        assert_eq!(calibre.asin(), Some("B0736185ZL"));
        assert_eq!(calibre.isbn(), Some("9780441013593"));
        let schemed = with_ids(&[(Some("uuid"), "x"), (Some("AMAZON"), "B01MZI77C0")]);
        assert_eq!(schemed.asin(), Some("B01MZI77C0"));
        assert_eq!(schemed.isbn(), None);
    }
}
