//! The local library: imported EPUBs kept unmodified in a managed folder, with
//! their (editable) metadata in SQLite. Edits are applied only when a copy is
//! exported for a device, so originals are never rewritten.
//!
//! Layout under the library root:
//! ```text
//! library.sqlite
//! books/<id>/book.epub     original file, byte for byte
//! books/<id>/cover.jpg     thumbnail for the grid (optional)
//! books/<id>/custom-cover.jpg  user-chosen cover, applied on export (optional)
//! ```

mod authors;
mod color;
mod rules;
mod schema;
mod stores;

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use chrono::Utc;
use erc_epub::{Contributor, Cover, Epub, Identifier, Metadata, MetadataEdit};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::Serialize;
use sha2::{Digest, Sha256};

pub use authors::normalize_authors;
pub use color::{dominant_color, dominant_colors};
pub use rules::{Rule, RuleKind};
pub use stores::{Annotation, Reading};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io { path: PathBuf, source: io::Error },
    #[error("library database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Epub(#[from] erc_epub::Error),
    #[error("no book with id {0}")]
    NotFound(i64),
    #[error("title can't be empty")]
    EmptyTitle,
    #[error("not a supported image: {0}")]
    BadImage(String),
}

pub type Result<T> = std::result::Result<T, Error>;

const COVER_HEIGHT: u32 = 600;
/// Custom covers are stored at most this tall: the Kobo Libra 2's screen
/// height, which is also the tallest current Kobo screen.
const CUSTOM_COVER_MAX_HEIGHT: u32 = 1680;

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
    pub description: Option<String>,
    pub identifiers: Vec<Identifier>,
    pub original_name: String,
    pub file_size: u64,
    pub has_cover: bool,
    /// The cover was chosen by the user rather than taken from the EPUB.
    pub custom_cover: bool,
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
        self.identifiers
            .iter()
            .find_map(|i| match i.scheme.as_deref() {
                Some(s) => schemes
                    .iter()
                    .any(|w| s.eq_ignore_ascii_case(w))
                    .then_some(i.value.as_str()),
                None => {
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
        Ok(library)
    }

    /// Reads contributors for books imported before they were tracked. Best
    /// effort: a book whose EPUB can't be read stays unknown (and untouched).
    fn backfill_contributors(&self) {
        let Ok(mut stmt) = self
            .conn
            .prepare("SELECT id FROM books WHERE contributors IS NULL")
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

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn import(&mut self, path: &Path) -> Result<ImportOutcome> {
        let (hash, size) = hash_file(path)?;
        if let Some(book) = self.find_by_hash(&hash)? {
            return Ok(ImportOutcome::AlreadyInLibrary { book });
        }
        let epub = Epub::open(path)?;
        let meta = epub.metadata();
        let original_name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let title = meta
            .title
            .clone()
            .unwrap_or_else(|| original_name.trim_end_matches(".epub").to_owned());
        let authors = normalize_authors(&meta.authors);
        let now = now();

        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO books (title, authors, series, series_index, tags, language, publisher, description,
                                identifiers, file_hash, file_size, original_name, has_cover, added_at, modified_at,
                                contributors)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, 0, ?13, ?13, ?14)",
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
                size as i64,
                original_name,
                now,
                json(&meta.contributors),
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

    pub fn remove(&mut self, id: i64) -> Result<()> {
        if self.conn.execute("DELETE FROM books WHERE id = ?1", [id])? == 0 {
            return Err(Error::NotFound(id));
        }
        let dir = self.book_dir(id);
        fs::remove_dir_all(&dir).map_err(|e| io_err(&dir, e))
    }

    /// The unmodified original.
    pub fn book_file(&self, id: i64) -> PathBuf {
        self.book_dir(id).join("book.epub")
    }

    /// Width and height of the cover sent to devices: the custom one if set,
    /// else the EPUB's own.
    pub fn cover_size(&self, id: i64) -> Option<(u32, u32)> {
        let custom = self.custom_cover_file(id);
        if custom.is_file() {
            return image::image_dimensions(&custom).ok();
        }
        let cover = Epub::open(self.book_file(id)).ok()?.cover().ok()??;
        image::ImageReader::new(std::io::Cursor::new(cover.data))
            .with_guessed_format()
            .ok()?
            .into_dimensions()
            .ok()
    }

    pub fn cover_file(&self, id: i64) -> Option<PathBuf> {
        let path = self.book_dir(id).join("cover.jpg");
        path.is_file().then_some(path)
    }

    /// Writes the book with its current library metadata (and custom cover)
    /// applied to `out`.
    pub fn export(&self, id: i64, out: &Path) -> Result<Book> {
        let book = self.book(id)?;
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
            "UPDATE books SET custom_cover = 1, has_cover = ?1, modified_at = ?2 WHERE id = ?3",
            params![has_thumb, now(), id],
        )?;
        self.book(id)
    }

    /// Goes back to the cover inside the EPUB.
    pub fn reset_cover(&mut self, id: i64) -> Result<Book> {
        self.book(id)?;
        let _ = fs::remove_file(self.custom_cover_file(id));
        let thumb = self.book_dir(id).join("cover.jpg");
        let _ = fs::remove_file(&thumb);
        let original = Epub::open(self.book_file(id))?.cover().ok().flatten();
        let has_cover = original.is_some_and(|c| write_thumbnail(&c.data, &thumb));
        self.conn.execute(
            "UPDATE books SET custom_cover = 0, has_cover = ?1, modified_at = ?2 WHERE id = ?3",
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
        self.conn.execute(
            "INSERT OR REPLACE INTO device_copies (book_id, device_serial, device_path, content_id, sent_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![book_id, device_serial, device_path, content_id, now()],
        )?;
        Ok(())
    }

    pub fn device_copies(&self, device_serial: &str) -> Result<Vec<DeviceCopy>> {
        let mut stmt = self.conn.prepare(
            "SELECT book_id, device_serial, device_path, content_id, sent_at FROM device_copies WHERE device_serial = ?1",
        )?;
        let copies = stmt
            .query_map([device_serial], |r| {
                Ok(DeviceCopy {
                    book_id: r.get(0)?,
                    device_serial: r.get(1)?,
                    device_path: r.get(2)?,
                    content_id: r.get(3)?,
                    sent_at: r.get(4)?,
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
        description: row.get("description")?,
        identifiers: serde_json::from_str(&row.get::<_, String>("identifiers")?)
            .unwrap_or_default(),
        original_name: row.get("original_name")?,
        file_size: row.get::<_, i64>("file_size")? as u64,
        has_cover: row.get("has_cover")?,
        custom_cover: row.get("custom_cover")?,
        added_at: row.get("added_at")?,
        modified_at: row.get("modified_at")?,
    })
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
            description: None,
            identifiers: ids
                .iter()
                .map(|(scheme, value)| Identifier {
                    scheme: scheme.map(str::to_owned),
                    value: (*value).to_owned(),
                })
                .collect(),
            original_name: "t.epub".into(),
            file_size: 1,
            has_cover: false,
            custom_cover: false,
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
