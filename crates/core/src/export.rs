//! Exporting library books into one ZIP archive, or a folder, organised by a
//! template. Each book is written with its edited metadata and cover, as an
//! EPUB (comics stay CBZ) or converted to KEPUB.
//!
//! An archive also holds [`MANIFEST`], which records what the EPUB can't
//! carry (identifiers, publisher, description, language, and whether a comic
//! is manga), so importing the archive into Parallax restores it. The app can
//! add its settings to the manifest; reading history is never included.

use std::collections::{BTreeMap, HashSet};
use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use parallax_convert::Converter;
use parallax_epub::{Contributor, Identifier};
use parallax_library::{Book, Direction, Format, Kind, Library};
use parallax_metadata::genres;
use serde::{Deserialize, Serialize};
use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

/// The file inside an exported archive that describes its books.
pub const MANIFEST: &str = "parallax.json";
/// The manifest's format. Readers ignore fields they don't know.
pub const MANIFEST_VERSION: u32 = 1;
/// Manifests larger than this aren't read.
const MANIFEST_MAX_BYTES: u64 = 64 * 1024 * 1024;

/// The export options the app last used, as JSON it reads back.
pub const EXPORT_OPTIONS: &str = "export_options";

/// Each author's books in a folder, series books numbered.
pub const DEFAULT_TEMPLATE: &str = "{author}/[{series} {series_index} - ]{title}";

/// Ready-made ways to organise an export, by name, with their templates.
pub const PRESETS: [(&str, &str); 5] = [
    ("author", DEFAULT_TEMPLATE),
    (
        "author-series",
        "{author}/[{series}/][{series_index} - ]{title}",
    ),
    (
        "genre",
        "{genre}/{author}/[{series} {series_index} - ]{title}",
    ),
    ("series", "[{series}/][{series_index} - ]{title}"),
    ("flat", "[{author} - ]{title}"),
];

/// The template a preset name stands for.
pub fn preset(name: &str) -> Option<&'static str> {
    PRESETS.iter().find(|(n, _)| *n == name).map(|(_, t)| *t)
}

/// What `{genre}` is for a book with no genre among its tags.
pub const NO_GENRE: &str = "Other";

/// The fields a template may use.
pub const FIELDS: [&str; 10] = [
    "title",
    "author",
    "authors",
    "series",
    "series_index",
    "genre",
    "publisher",
    "year",
    "language",
    "id",
];

/// The file format books are exported in. Comics stay CBZ either way.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BookFormat {
    /// Standard EPUB, which any reader and app opens.
    #[default]
    Epub,
    /// Kobo's KEPUB, converted as books sent to a Kobo are.
    Kepub,
}

impl BookFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "epub" => Some(Self::Epub),
            "kepub" => Some(Self::Kepub),
            _ => None,
        }
    }
}

/// Where the books go.
#[derive(Debug, Clone)]
pub enum Destination {
    /// Into one new ZIP archive at this path, with a manifest.
    Archive(PathBuf),
    /// Into this folder, with the template's subfolders made as needed.
    Folder(PathBuf),
}

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// How books are named, folders separated by `/`.
    pub template: String,
    pub format: BookFormat,
    /// Settings to put in an archive's manifest. Ignored for a folder.
    pub settings: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Exported {
    pub book_id: i64,
    pub title: String,
    /// Where it went: a file path, or its name inside the archive.
    pub result: Result<String, String>,
}

/// What an exported archive says about itself and its books.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    pub parallax: u32,
    #[serde(default)]
    pub exported_at: Option<String>,
    #[serde(default)]
    pub books: Vec<ManifestBook>,
    /// The exporting app's settings that differ from the defaults, when
    /// asked for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub settings: Option<BTreeMap<String, String>>,
}

/// One book's details, keyed by its file's path inside the archive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManifestBook {
    pub file: String,
    pub title: String,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub contributors: Option<Vec<Contributor>>,
    #[serde(default)]
    pub series: Option<String>,
    #[serde(default)]
    pub series_index: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub publisher: Option<String>,
    #[serde(default)]
    pub published: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub identifiers: Vec<Identifier>,
    #[serde(default)]
    pub kind: Option<Kind>,
    #[serde(default)]
    pub direction: Option<Direction>,
}

impl ManifestBook {
    fn new(file: String, book: &Book) -> Self {
        Self {
            file,
            title: book.title.clone(),
            authors: book.authors.clone(),
            contributors: book.contributors.clone(),
            series: book.series.clone(),
            series_index: book.series_index.clone(),
            tags: book.tags.clone(),
            language: book.language.clone(),
            publisher: book.publisher.clone(),
            published: book.published.clone(),
            description: book.description.clone(),
            identifiers: book.identifiers.clone(),
            kind: Some(book.kind),
            direction: Some(book.direction),
        }
    }
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

/// The book's genre: its first tag that is a standard genre, preferring a
/// specific one over plain Fiction or Nonfiction.
pub fn genre(book: &Book) -> Option<&'static str> {
    let found: Vec<&'static str> = book
        .tags
        .iter()
        .filter_map(|t| genres::canonical(t))
        .collect();
    found
        .iter()
        .find(|g| !matches!(**g, "Fiction" | "Nonfiction"))
        .or_else(|| found.first())
        .copied()
}

fn field(book: &Book, name: &str) -> String {
    match name {
        "title" => book.title.clone(),
        "author" => book.authors.first().cloned().unwrap_or_default(),
        "authors" => book.authors.join(", "),
        "series" => book.series.clone().unwrap_or_default(),
        "series_index" => book.series_index.clone().unwrap_or_default(),
        "genre" => genre(book).unwrap_or(NO_GENRE).to_owned(),
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

/// The extension `book` is exported with in `format`, with its dot.
fn extension(book: &Book, format: BookFormat) -> &'static str {
    match (book.format, format) {
        (Format::Cbz, _) => ".cbz",
        (_, BookFormat::Epub) => ".epub",
        (_, BookFormat::Kepub) => ".kepub.epub",
    }
}

/// The relative path `book` is exported at under `template`, without its
/// extension.
fn relative_stem(template: &str, book: &Book) -> PathBuf {
    let filled = fill(template, book);
    let parts: Vec<String> = filled
        .split('/')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(safe_component)
        .collect();
    if parts.is_empty() {
        PathBuf::from(safe_component(&book.title))
    } else {
        parts.iter().collect()
    }
}

/// The relative path `book` is exported at under `template`, as an EPUB or
/// CBZ.
pub fn relative_path(template: &str, book: &Book) -> PathBuf {
    with_extension(
        &relative_stem(template, book),
        extension(book, BookFormat::Epub),
    )
}

fn with_extension(stem: &Path, ext: &str) -> PathBuf {
    let mut name = stem.as_os_str().to_owned();
    name.push(ext);
    PathBuf::from(name)
}

/// `stem` + `ext`, or with " (2)", " (3)"… before the extension, whichever
/// is not taken.
fn unique(stem: &Path, ext: &str, taken: impl Fn(&Path) -> bool) -> PathBuf {
    let first = with_extension(stem, ext);
    if !taken(&first) {
        return first;
    }
    (2..=u32::MAX)
        .map(|n| {
            let mut name = stem.as_os_str().to_owned();
            name.push(format!(" ({n}){ext}"));
            PathBuf::from(name)
        })
        .find(|p| !taken(p))
        .expect("some number is free")
}

/// Writes `book` to `out` in `format`, using `work` for a KEPUB's EPUB.
fn write_book(
    library: &Library,
    book: &Book,
    format: BookFormat,
    converter: Option<&dyn Converter>,
    work: &Path,
    out: &Path,
) -> Result<(), String> {
    if book.format == Format::Cbz || format == BookFormat::Epub {
        return library
            .export(book.id, out)
            .map(drop)
            .map_err(|e| e.to_string());
    }
    let converter = converter.ok_or("KEPUB conversion isn't available")?;
    let epub = work.join(format!("{}.epub", book.id));
    let result = library
        .export(book.id, &epub)
        .map_err(|e| e.to_string())
        .and_then(|_| converter.convert(&epub, out).map_err(|e| e.to_string()));
    let _ = fs::remove_file(&epub);
    result
}

/// Exports each book; one that fails doesn't stop the rest. `on_progress`
/// gets the index and title of each book as it starts. `converter` is needed
/// only for KEPUB.
pub fn export_books(
    library: &Library,
    ids: &[i64],
    options: &ExportOptions,
    destination: &Destination,
    converter: Option<&dyn Converter>,
    mut on_progress: impl FnMut(usize, &str),
) -> Result<Vec<Exported>, String> {
    check_template(&options.template)?;
    if options.format == BookFormat::Kepub && converter.is_none() {
        return Err("KEPUB conversion isn't available".to_owned());
    }
    let work = tempfile::tempdir().map_err(|e| e.to_string())?;
    let mut outcomes = Vec::new();
    let mut each = |write: &mut dyn FnMut(&Book, PathBuf, &str) -> Result<String, String>| {
        for (index, &id) in ids.iter().enumerate() {
            let book = match library.book(id) {
                Ok(book) => book,
                Err(e) => {
                    outcomes.push(failed(id, &e));
                    continue;
                }
            };
            on_progress(index, &book.title);
            let stem = relative_stem(&options.template, &book);
            let result = write(&book, stem, extension(&book, options.format));
            outcomes.push(Exported {
                book_id: id,
                title: book.title,
                result,
            });
        }
    };
    match destination {
        Destination::Folder(root) => {
            fs::create_dir_all(root).map_err(|e| e.to_string())?;
            each(&mut |book, stem, ext| {
                let path = unique(&root.join(stem), ext, Path::exists);
                path.parent()
                    .map_or(Ok(()), fs::create_dir_all)
                    .map_err(|e| e.to_string())?;
                write_book(library, book, options.format, converter, work.path(), &path)?;
                Ok(path.display().to_string())
            });
        }
        Destination::Archive(path) => {
            // Written beside the destination and moved into place, so a
            // failed export never leaves half an archive under its name.
            let partial = path.with_extension("zip.partial");
            let file = File::create(&partial).map_err(|e| e.to_string())?;
            let mut zip = zip::ZipWriter::new(BufWriter::new(file));
            let mut names = HashSet::from([PathBuf::from(MANIFEST)]);
            let mut manifest = Manifest {
                parallax: MANIFEST_VERSION,
                exported_at: Some(chrono::Utc::now().to_rfc3339()),
                books: Vec::new(),
                settings: options.settings.clone().filter(|s| !s.is_empty()),
            };
            each(&mut |book, stem, ext| {
                let name = unique(&stem, ext, |p| names.contains(p));
                let staged = work.path().join(format!("{}{ext}", book.id));
                let result = write_book(
                    library,
                    book,
                    options.format,
                    converter,
                    work.path(),
                    &staged,
                )
                .and_then(|()| {
                    let entry = name.to_string_lossy().replace('\\', "/");
                    zip.start_file(entry.as_str(), file_options())
                        .map_err(|e| e.to_string())?;
                    io::copy(
                        &mut File::open(&staged).map_err(|e| e.to_string())?,
                        &mut zip,
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(entry)
                });
                let _ = fs::remove_file(&staged);
                if let Ok(entry) = &result {
                    names.insert(name);
                    manifest.books.push(ManifestBook::new(entry.clone(), book));
                }
                result
            });
            let finish = write_manifest(&mut zip, &manifest)
                .and_then(|()| zip.finish().map_err(|e| e.to_string()))
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

/// Books are already compressed inside (EPUBs and CBZs are ZIPs of mostly
/// images), so harder compression only trades time for a few percent.
/// Deflate level 6 is what Finder's Compress uses.
fn file_options() -> SimpleFileOptions {
    SimpleFileOptions::default()
        .large_file(true)
        .compression_method(CompressionMethod::Deflated)
        .compression_level(Some(6))
}

fn write_manifest(
    zip: &mut zip::ZipWriter<BufWriter<File>>,
    manifest: &Manifest,
) -> Result<(), String> {
    zip.start_file(MANIFEST, file_options())
        .map_err(|e| e.to_string())?;
    serde_json::to_writer_pretty(&mut *zip, manifest).map_err(|e| e.to_string())
}

/// The manifest of an archive Parallax exported, if `path` is a ZIP with a
/// readable one.
pub fn read_manifest(path: &Path) -> Option<Manifest> {
    let mut archive = zip::ZipArchive::new(File::open(path).ok()?).ok()?;
    let file = archive.by_name(MANIFEST).ok()?;
    if file.size() > MANIFEST_MAX_BYTES {
        return None;
    }
    let mut text = String::new();
    file.take(MANIFEST_MAX_BYTES)
        .read_to_string(&mut text)
        .ok()?;
    let manifest: Manifest = serde_json::from_str(&text).ok()?;
    (manifest.parallax >= 1).then_some(manifest)
}

/// Gives a book just imported from an exported archive what its EPUB
/// couldn't carry. Only fills what the library doesn't know yet.
pub fn restore_details(
    library: &mut Library,
    id: i64,
    entry: &ManifestBook,
) -> parallax_library::Result<()> {
    let book = library.fill_details(
        id,
        &entry.identifiers,
        entry.publisher.as_deref(),
        entry.description.as_deref(),
        entry.language.as_deref(),
    )?;
    if book.format == Format::Cbz
        && let Some(kind @ (Kind::Comic | Kind::Manga)) = entry.kind
    {
        let direction = entry.direction.unwrap_or(book.direction);
        if (kind, direction) != (book.kind, book.direction) {
            library.set_comic_kind(id, kind, direction)?;
        }
    }
    Ok(())
}

fn failed(id: i64, e: &impl ToString) -> Exported {
    Exported {
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
    fn presets_organise_by_series_genre_and_nothing() {
        let mut b = book();
        b.title = "Leviathan Wakes".into();
        b.authors = vec!["James S. A. Corey".into()];
        b.series = Some("The Expanse".into());
        b.series_index = Some("1".into());
        b.tags = vec!["sci-fi".into(), "Fiction".into()];
        let path = |name| relative_path(preset(name).unwrap(), &b);
        assert_eq!(
            path("author-series"),
            Path::new("James S. A. Corey/The Expanse/1 - Leviathan Wakes.epub")
        );
        assert_eq!(
            path("genre"),
            Path::new("Science Fiction/James S. A. Corey/The Expanse 1 - Leviathan Wakes.epub")
        );
        assert_eq!(
            path("series"),
            Path::new("The Expanse/1 - Leviathan Wakes.epub")
        );
        assert_eq!(
            path("flat"),
            Path::new("James S. A. Corey - Leviathan Wakes.epub")
        );
        b.tags = vec!["Fiction".into()];
        assert_eq!(genre(&b), Some("Fiction"));
        b.tags.clear();
        assert!(relative_path(preset("genre").unwrap(), &b).starts_with(NO_GENRE));
        for (_, template) in PRESETS {
            assert!(check_template(template).is_ok(), "{template}");
        }
    }

    #[test]
    fn kepub_names_keep_their_double_extension_when_numbered() {
        let taken = |p: &Path| p == Path::new("Dune.kepub.epub");
        assert_eq!(
            unique(Path::new("Dune"), ".kepub.epub", taken),
            Path::new("Dune (2).kepub.epub")
        );
    }

    #[test]
    fn checks_templates() {
        assert!(check_template(DEFAULT_TEMPLATE).is_ok());
        assert!(check_template("{genre}/{title}").is_ok());
        assert!(check_template("books").is_err());
        assert!(check_template("{name}").is_err());
        assert!(check_template("{title").is_err());
    }
}
