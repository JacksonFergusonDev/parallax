//! Saving library books to a folder, or into one ZIP archive, named by a
//! template, as calibre's "Save to disk" does. Each book is written with its
//! edited metadata and cover, as it would be sent to a Kobo.

use std::collections::HashSet;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};

use parallax_library::{Book, Format, Library};
use serde::Serialize;
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

/// How saved books are named, folders separated by `/`.
pub const SAVE_TEMPLATE: &str = "save_template";
/// `on` to save into one ZIP archive rather than a folder.
pub const SAVE_ARCHIVE: &str = "save_archive";
/// How hard the archive is compressed: `store`, `fast` or `best`; unset is
/// normal.
pub const SAVE_COMPRESSION: &str = "save_compression";
pub const SAVE_COMPRESSIONS: [&str; 3] = ["store", "fast", "best"];

/// Each author's books in a folder, series books numbered.
pub const DEFAULT_TEMPLATE: &str = "{author}/[{series} {series_index} - ]{title}";

/// The fields a template may use.
pub const FIELDS: [&str; 9] = [
    "title",
    "author",
    "authors",
    "series",
    "series_index",
    "publisher",
    "year",
    "language",
    "id",
];

/// How much to compress the archive. Books are already compressed inside
/// (EPUBs and CBZs are ZIPs of mostly images), so this mostly trades time
/// for a few percent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Copied in as they are: quickest, and barely larger.
    Store,
    /// Deflate level 1.
    Fast,
    /// Deflate level 6, what Finder's Compress uses.
    Normal,
    /// Deflate level 9.
    Best,
}

impl Compression {
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("store") => Self::Store,
            Some("fast") => Self::Fast,
            Some("best") => Self::Best,
            _ => Self::Normal,
        }
    }

    fn options(self) -> SimpleFileOptions {
        let options = SimpleFileOptions::default().large_file(true);
        match self {
            Self::Store => options.compression_method(CompressionMethod::Stored),
            Self::Fast => options
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(1)),
            Self::Normal => options
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(6)),
            Self::Best => options
                .compression_method(CompressionMethod::Deflated)
                .compression_level(Some(9)),
        }
    }
}

/// Where the books go.
#[derive(Debug, Clone)]
pub enum Destination {
    /// Into this folder, with the template's subfolders made as needed.
    Folder(PathBuf),
    /// Into one new ZIP archive at this path.
    Archive(PathBuf, Compression),
}

#[derive(Debug, Clone, Serialize)]
pub struct Saved {
    pub book_id: i64,
    pub title: String,
    /// Where it went: a file path, or its name inside the archive.
    pub result: Result<String, String>,
}

/// A template with no fields, or with an unknown one, can't be used.
pub fn check_template(template: &str) -> Result<(), String> {
    let mut rest = template;
    let mut any = false;
    while let Some(start) = rest.find('{') {
        let after = &rest[start + 1..];
        let end = after
            .find('}')
            .ok_or_else(|| "a { has no closing }".to_owned())?;
        let field = &after[..end];
        if !FIELDS.contains(&field) {
            return Err(format!("{{{field}}} isn't a field you can use"));
        }
        any = true;
        rest = &after[end + 1..];
    }
    if any {
        Ok(())
    } else {
        Err("use at least one field, such as {title}".to_owned())
    }
}

fn field(book: &Book, name: &str) -> String {
    match name {
        "title" => book.title.clone(),
        "author" => book.authors.first().cloned().unwrap_or_default(),
        "authors" => book.authors.join(", "),
        "series" => book.series.clone().unwrap_or_default(),
        "series_index" => book.series_index.clone().unwrap_or_default(),
        "publisher" => book.publisher.clone().unwrap_or_default(),
        "year" => book
            .published
            .as_deref()
            .and_then(|d| d.get(..4))
            .unwrap_or_default()
            .to_owned(),
        "language" => book.language.clone().unwrap_or_default(),
        "id" => book.id.to_string(),
        _ => String::new(),
    }
}

/// Fills in `{field}`s. Text in `[...]` is kept only when every field in it
/// has a value, so `[{series} {series_index} - ]` disappears for a book
/// outside a series.
fn fill(template: &str, book: &Book) -> String {
    let mut out = String::new();
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let name: String = chars.by_ref().take_while(|&c| c != '}').collect();
                out.push_str(&field(book, &name));
            }
            '[' => {
                let inner: String = chars.by_ref().take_while(|&c| c != ']').collect();
                let mut all = true;
                let mut part = String::new();
                let mut inner_chars = inner.chars();
                while let Some(c) = inner_chars.next() {
                    if c == '{' {
                        let name: String = inner_chars.by_ref().take_while(|&c| c != '}').collect();
                        let value = field(book, &name);
                        all &= !value.trim().is_empty();
                        part.push_str(&value);
                    } else {
                        part.push(c);
                    }
                }
                if all {
                    out.push_str(&part);
                }
            }
            c => out.push(c),
        }
    }
    out
}

/// One folder or file name, safe on macOS, Windows and a Kobo's FAT disk.
fn safe_component(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim();
    let short: String = trimmed.chars().take(120).collect();
    let short = short.trim_end();
    if short.is_empty() {
        "Unknown".to_owned()
    } else {
        short.to_owned()
    }
}

/// The relative path `book` is saved at under `template`, with its extension.
pub fn relative_path(template: &str, book: &Book) -> PathBuf {
    let filled = fill(template, book);
    let parts: Vec<String> = filled
        .split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(safe_component)
        .collect();
    let mut path: PathBuf = if parts.is_empty() {
        PathBuf::from(safe_component(&book.title))
    } else {
        parts.iter().collect()
    };
    let ext = if book.format == Format::Cbz {
        "cbz"
    } else {
        "epub"
    };
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    path.set_file_name(format!("{name}.{ext}"));
    path
}

/// `path`, or `path` with " (2)", " (3)"… before the extension, whichever
/// is not taken.
fn unique(path: PathBuf, taken: impl Fn(&Path) -> bool) -> PathBuf {
    if !taken(&path) {
        return path;
    }
    let stem = path
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let ext = path
        .extension()
        .map_or_else(String::new, |e| format!(".{}", e.to_string_lossy()));
    (2..=u32::MAX)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|p| !taken(p))
        .expect("some number is free")
}

/// Saves each book; one that fails doesn't stop the rest. `on_progress`
/// gets the index and title of each book as it starts.
pub fn save_books(
    library: &Library,
    ids: &[i64],
    template: &str,
    destination: &Destination,
    mut on_progress: impl FnMut(usize, &str),
) -> Result<Vec<Saved>, String> {
    check_template(template)?;
    let mut outcomes = Vec::new();
    match destination {
        Destination::Folder(root) => {
            fs::create_dir_all(root).map_err(|e| e.to_string())?;
            for (index, &id) in ids.iter().enumerate() {
                let book = match library.book(id) {
                    Ok(book) => book,
                    Err(e) => {
                        outcomes.push(failed(id, &e));
                        continue;
                    }
                };
                on_progress(index, &book.title);
                let path = unique(root.join(relative_path(template, &book)), Path::exists);
                let result = path
                    .parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .map_err(|e| e.to_string())
                    .and_then(|()| library.export(id, &path).map_err(|e| e.to_string()))
                    .map(|_| path.display().to_string());
                outcomes.push(Saved {
                    book_id: id,
                    title: book.title,
                    result,
                });
            }
        }
        Destination::Archive(path, compression) => {
            let staging = tempfile::tempdir().map_err(|e| e.to_string())?;
            // Written beside the destination and moved into place, so a
            // failed save never leaves half an archive under its name.
            let partial = path.with_extension("zip.partial");
            let file = File::create(&partial).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipWriter::new(BufWriter::new(file));
            let mut names = HashSet::new();
            for (index, &id) in ids.iter().enumerate() {
                let book = match library.book(id) {
                    Ok(book) => book,
                    Err(e) => {
                        outcomes.push(failed(id, &e));
                        continue;
                    }
                };
                on_progress(index, &book.title);
                let name = unique(relative_path(template, &book), |p| names.contains(p));
                let exported = staging.path().join(format!("{id}.book"));
                let result = library
                    .export(id, &exported)
                    .map_err(|e| e.to_string())
                    .and_then(|_| {
                        let entry = name.to_string_lossy().replace('\\', "/");
                        zip.start_file(entry.as_str(), compression.options())
                            .map_err(|e| e.to_string())?;
                        io::copy(
                            &mut File::open(&exported).map_err(|e| e.to_string())?,
                            &mut zip,
                        )
                        .map_err(|e| e.to_string())?;
                        Ok(entry)
                    });
                let _ = fs::remove_file(&exported);
                if result.is_ok() {
                    names.insert(name);
                }
                outcomes.push(Saved {
                    book_id: id,
                    title: book.title,
                    result,
                });
            }
            let finish = zip
                .finish()
                .map_err(|e| e.to_string())
                .and_then(|mut w| w.flush().map_err(|e| e.to_string()))
                .and_then(|()| fs::rename(&partial, path).map_err(|e| e.to_string()));
            if let Err(e) = finish {
                let _ = fs::remove_file(&partial);
                return Err(format!("couldn't finish the archive: {e}"));
            }
        }
    }
    Ok(outcomes)
}

fn failed(id: i64, e: &impl ToString) -> Saved {
    Saved {
        book_id: id,
        title: format!("Book {id}"),
        result: Err(e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book() -> Book {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::open(dir.path()).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/epub/minimal.epub");
        let parallax_library::ImportOutcome::Added { book, .. } = library.import(&fixture).unwrap()
        else {
            panic!();
        };
        book
    }

    #[test]
    fn templates_name_books_and_drop_empty_parts() {
        let mut b = book();
        b.title = "Red Seas: Under Red Skies".into();
        b.authors = vec!["Scott Lynch".into(), "Someone Else".into()];
        b.series = None;
        assert_eq!(
            relative_path(DEFAULT_TEMPLATE, &b),
            Path::new("Scott Lynch/Red Seas_ Under Red Skies.epub")
        );
        b.series = Some("Gentleman Bastard".into());
        b.series_index = Some("2".into());
        assert_eq!(
            relative_path(DEFAULT_TEMPLATE, &b),
            Path::new("Scott Lynch/Gentleman Bastard 2 - Red Seas_ Under Red Skies.epub")
        );
        b.authors.clear();
        assert_eq!(
            relative_path("{author}/{title}", &b),
            Path::new("Red Seas_ Under Red Skies.epub")
        );
        assert_eq!(
            relative_path("../{title}", &b),
            Path::new("Unknown/Red Seas_ Under Red Skies.epub")
        );
    }

    #[test]
    fn checks_templates() {
        assert!(check_template(DEFAULT_TEMPLATE).is_ok());
        assert!(check_template("books").is_err());
        assert!(check_template("{name}").is_err());
        assert!(check_template("{title").is_err());
    }

    #[test]
    fn compression_defaults_to_normal() {
        assert_eq!(Compression::parse(None), Compression::Normal);
        assert_eq!(Compression::parse(Some("store")), Compression::Store);
    }
}
