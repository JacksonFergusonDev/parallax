//! Moving a calibre library into this one.
//!
//! Only reads the calibre library: `metadata.db` is opened immutable and
//! read-only (calibre may be running), and book files are copied, never
//! moved. calibre's database is the source of truth for its metadata (it only
//! writes metadata into the EPUB when exporting), so title, authors, series,
//! tags, identifiers, publisher, description and cover come from there.
//! EPUB is preferred; a KEPUB-only book (calibre's Kobo plugin keeps one) is
//! cleaned of Kobo markup first; books with neither are skipped. Importing
//! again matches books already imported (by calibre's book uuid) or already
//! in the library (same file, ISBN, or title and first author), so it never
//! duplicates.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use erc_epub::{Epub, Identifier, Metadata, has_drm, strip_kobo_markup};
use erc_library::{ImportOutcome, Library};
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} isn't a calibre library (no metadata.db)")]
    NotALibrary(PathBuf),
    #[error("couldn't read calibre's database: {0}")]
    Database(#[from] rusqlite::Error),
}

/// A book as calibre's database describes it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CalibreBook {
    pub id: i64,
    pub uuid: Option<String>,
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub tags: Vec<String>,
    /// (scheme, value), e.g. ("isbn", "978…"), ("mobi-asin", "B0…").
    pub identifiers: Vec<(String, String)>,
    pub publisher: Option<String>,
    pub language: Option<String>,
    pub description: Option<String>,
    /// Upper-case format → file, for files that exist.
    pub formats: Vec<(String, PathBuf)>,
    pub cover: Option<PathBuf>,
}

impl CalibreBook {
    fn format(&self, name: &str) -> Option<&Path> {
        self.formats
            .iter()
            .find(|(f, _)| f == name)
            .map(|(_, p)| p.as_path())
    }

    /// The file ERC can import, if any.
    fn source(&self) -> Option<(&Path, bool)> {
        self.format("EPUB")
            .map(|p| (p, false))
            .or_else(|| self.format("KEPUB").map(|p| (p, true)))
    }
}

/// Where calibre says its library is, else calibre's default folder, if it
/// exists.
pub fn default_library() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut prefs = Vec::new();
    if let Some(home) = &home {
        prefs.push(home.join("Library/Preferences/calibre/global.py.json"));
        prefs.push(home.join(".config/calibre/global.py.json"));
    }
    if let Some(appdata) = std::env::var_os("APPDATA") {
        prefs.push(PathBuf::from(appdata).join("calibre/global.py.json"));
    }
    let configured = prefs.iter().find_map(|p| {
        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(p).ok()?).ok()?;
        Some(PathBuf::from(json.get("library_path")?.as_str()?))
    });
    configured
        .into_iter()
        .chain(home.map(|h| h.join("Calibre Library")))
        .find(|p| p.join("metadata.db").is_file())
}

/// Every book in the calibre library at `root`, by calibre id.
pub fn read_library(root: &Path) -> Result<Vec<CalibreBook>, Error> {
    let db_path = root.join("metadata.db");
    if !db_path.is_file() {
        return Err(Error::NotALibrary(root.to_owned()));
    }
    // Immutable: no locks, no journal files, nothing written next to it.
    let uri = format!(
        "file:{}?mode=ro&immutable=1",
        db_path
            .to_string_lossy()
            .replace('?', "%3f")
            .replace('#', "%23")
    );
    let conn = Connection::open_with_flags(
        uri,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
    )?;

    let mut authors = grouped(
        &conn,
        "SELECT l.book, a.name FROM books_authors_link l JOIN authors a ON a.id = l.author ORDER BY l.id",
    )?;
    let mut tags = grouped(
        &conn,
        "SELECT l.book, t.name FROM books_tags_link l JOIN tags t ON t.id = l.tag ORDER BY t.name COLLATE NOCASE",
    )?;
    let mut series = grouped(
        &conn,
        "SELECT l.book, s.name FROM books_series_link l JOIN series s ON s.id = l.series",
    )?;
    let mut publishers = grouped(
        &conn,
        "SELECT l.book, p.name FROM books_publishers_link l JOIN publishers p ON p.id = l.publisher",
    )?;
    let mut languages = grouped(
        &conn,
        "SELECT l.book, g.lang_code FROM books_languages_link l JOIN languages g ON g.id = l.lang_code ORDER BY l.item_order",
    )?;
    let mut comments = grouped(&conn, "SELECT book, text FROM comments")?;
    let mut identifiers: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT book, type, val FROM identifiers ORDER BY id")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (book, scheme, value) = row?;
            identifiers
                .entry(book)
                .or_default()
                .push((scheme.to_lowercase(), value));
        }
    }
    let mut formats: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT book, format, name FROM data")?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?;
        for row in rows {
            let (book, format, name) = row?;
            formats.entry(book).or_default().push((format, name));
        }
    }

    let mut stmt = conn.prepare(
        "SELECT id, uuid, title, series_index, path, has_cover FROM books ORDER BY sort COLLATE NOCASE, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<f64>>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, Option<bool>>(5)?,
        ))
    })?;
    let mut books = Vec::new();
    for row in rows {
        let (id, uuid, title, index, path, has_cover) = row?;
        let dir = root.join(&path);
        let series = series.remove(&id).and_then(|s| s.into_iter().next());
        let book_formats = formats
            .remove(&id)
            .unwrap_or_default()
            .into_iter()
            .map(|(format, name)| {
                let file = dir.join(format!("{name}.{}", format.to_lowercase()));
                (format.to_uppercase(), file)
            })
            .filter(|(_, file)| file.is_file())
            .collect();
        let cover = dir.join("cover.jpg");
        books.push(CalibreBook {
            id,
            uuid,
            title,
            authors: authors.remove(&id).unwrap_or_default(),
            series_index: series.as_ref().and(index).map(format_index),
            series,
            tags: tags.remove(&id).unwrap_or_default(),
            identifiers: identifiers.remove(&id).unwrap_or_default(),
            publisher: publishers.remove(&id).and_then(|p| p.into_iter().next()),
            language: languages.remove(&id).and_then(|l| l.into_iter().next()),
            description: comments.remove(&id).and_then(|c| c.into_iter().next()),
            formats: book_formats,
            cover: (has_cover.unwrap_or(false) && cover.is_file()).then_some(cover),
        });
    }
    Ok(books)
}

/// `book id → values` from a two-column query.
fn grouped(conn: &Connection, sql: &str) -> rusqlite::Result<HashMap<i64, Vec<String>>> {
    let mut map: HashMap<i64, Vec<String>> = HashMap::new();
    let mut stmt = conn.prepare(sql)?;
    for row in stmt.query_map([], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
    })? {
        let (book, value) = row?;
        if let Some(value) = value.filter(|v| !v.trim().is_empty()) {
            map.entry(book).or_default().push(value);
        }
    }
    Ok(map)
}

/// calibre stores series numbers as REAL: 1.0 → "1", 2.5 → "2.5".
fn format_index(index: f64) -> String {
    if index.fract() == 0.0 {
        format!("{}", index as i64)
    } else {
        format!("{index}")
    }
}

/// What importing the library would do, without changing anything.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CalibrePreview {
    pub total: usize,
    /// Books with an EPUB (or KEPUB) not yet in this library.
    pub new: usize,
    /// Books already imported or already in this library.
    pub already: usize,
    /// Books without an EPUB or KEPUB, by their formats ("PDF", "AZW3, MOBI").
    pub no_epub: Vec<(String, usize)>,
}

pub fn preview(library: &Library, books: &[CalibreBook]) -> CalibrePreview {
    let mut out = CalibrePreview {
        total: books.len(),
        ..CalibrePreview::default()
    };
    let mut no_epub: HashMap<String, usize> = HashMap::new();
    for book in books {
        if book.source().is_none() {
            let mut formats: Vec<&str> = book.formats.iter().map(|(f, _)| f.as_str()).collect();
            formats.sort_unstable();
            let key = if formats.is_empty() {
                "no files".to_owned()
            } else {
                formats.join(", ")
            };
            *no_epub.entry(key).or_default() += 1;
        } else if existing(library, book).is_some() {
            out.already += 1;
        } else {
            out.new += 1;
        }
    }
    out.no_epub = no_epub.into_iter().collect();
    out.no_epub
        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    out
}

/// The library book this calibre book already is, if any.
fn existing(library: &Library, book: &CalibreBook) -> Option<i64> {
    if let Some(id) = book
        .uuid
        .as_deref()
        .and_then(|u| library.calibre_link(u).ok().flatten())
        .filter(|&id| library.book(id).is_ok())
    {
        return Some(id);
    }
    let meta = Metadata {
        title: Some(book.title.clone()),
        authors: book.authors.clone(),
        identifiers: identifiers(book),
        ..Metadata::default()
    };
    library.find_similar_to(&meta).ok().flatten()
}

fn identifiers(book: &CalibreBook) -> Vec<Identifier> {
    book.identifiers
        .iter()
        .map(|(scheme, value)| Identifier {
            scheme: Some(scheme.clone()),
            value: value.clone(),
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalibreImport {
    Added {
        book_id: i64,
    },
    /// Already in this library; left as it is.
    Matched {
        book_id: i64,
    },
    Skipped {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize)]
pub struct CalibreImportOutcome {
    pub calibre_id: i64,
    pub title: String,
    pub result: Result<CalibreImport, String>,
}

/// Imports `books` into `library`. `on_progress(index, total, title)` is
/// called before each book.
pub fn import(
    library: &mut Library,
    books: &[CalibreBook],
    mut on_progress: impl FnMut(usize, usize, &str),
) -> Vec<CalibreImportOutcome> {
    let total = books.len();
    books
        .iter()
        .enumerate()
        .map(|(i, book)| {
            on_progress(i + 1, total, &book.title);
            CalibreImportOutcome {
                calibre_id: book.id,
                title: book.title.clone(),
                result: import_one(library, book),
            }
        })
        .collect()
}

fn import_one(library: &mut Library, book: &CalibreBook) -> Result<CalibreImport, String> {
    let link = |library: &Library, book_id: i64| match &book.uuid {
        Some(uuid) => library
            .record_calibre_link(uuid, book_id)
            .map_err(|e| e.to_string()),
        None => Ok(()),
    };
    let Some((source, kepub)) = book.source() else {
        let formats: Vec<&str> = book.formats.iter().map(|(f, _)| f.as_str()).collect();
        return Ok(CalibreImport::Skipped {
            reason: if formats.is_empty() {
                "no book file".to_owned()
            } else {
                format!("no EPUB ({})", formats.join(", "))
            },
        });
    };
    if let Some(id) = existing(library, book) {
        link(library, id)?;
        return Ok(CalibreImport::Matched { book_id: id });
    }
    if has_drm(source).map_err(|e| e.to_string())? {
        return Ok(CalibreImport::Skipped {
            reason: "DRM-protected".to_owned(),
        });
    }
    let workdir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let file = if kepub {
        let stem = source
            .file_stem()
            .map_or_else(|| "book".into(), |s| s.to_string_lossy());
        let clean = workdir.path().join(format!("{stem}.epub"));
        strip_kobo_markup(source, &clean).map_err(|e| e.to_string())?;
        clean
    } else {
        source.to_owned()
    };
    // Fail early with a clear message rather than inside the library.
    Epub::open(&file).map_err(|e| e.to_string())?;
    let added = match library.import(&file).map_err(|e| e.to_string())? {
        ImportOutcome::Added { book: added, .. } => added,
        ImportOutcome::AlreadyInLibrary { book: same } => {
            link(library, same.id)?;
            return Ok(CalibreImport::Matched { book_id: same.id });
        }
    };
    let mut edit = added.edit();
    edit.title = book.title.clone();
    if !book.authors.is_empty() {
        edit.authors = book.authors.clone();
    }
    edit.series = book.series.clone();
    edit.series_index = book.series_index.clone();
    edit.tags = book.tags.clone();
    library.update(added.id, &edit).map_err(|e| e.to_string())?;
    library
        .fill_details(
            added.id,
            &identifiers(book),
            book.publisher.as_deref(),
            book.description.as_deref(),
            book.language.as_deref(),
        )
        .map_err(|e| e.to_string())?;
    if let Some(cover) = &book.cover {
        // calibre's cover is the one its user chose; a broken one keeps the
        // EPUB's own.
        if let Ok(image) = std::fs::read(cover) {
            let _ = library.set_cover(added.id, &image);
        }
    }
    link(library, added.id)?;
    Ok(CalibreImport::Added { book_id: added.id })
}
