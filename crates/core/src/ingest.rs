//! Adding a book file to the library, whatever its format. The library stores
//! EPUBs, so Amazon books are converted on the way in.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use parallax_comic::{FolderKind, chapters_to_cbz, classify_folder, folder_to_cbz};
use parallax_convert::{Converter, amazon};
use parallax_library::{ImportOutcome, Library};

use crate::export;

mod archive;
pub use archive::is_book_archive;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Comic(#[from] parallax_comic::Error),
    #[error("Choose a folder of comic pages or chapter archives: {0}")]
    ComicFolder(std::path::PathBuf),
    #[error(transparent)]
    Library(#[from] parallax_library::Error),
    #[error(transparent)]
    Convert(#[from] parallax_convert::Error),
    #[error(transparent)]
    Archive(#[from] std::io::Error),
}

/// Imports each supported book in an archive. The returned paths identify
/// members for reporting. Each book commits independently, so an unreadable
/// book or a later archive error doesn't discard earlier imports. A book new
/// to the library gets the details an exported archive's manifest records
/// for it.
pub fn import_archive(
    library: &mut Library,
    converter: &dyn Converter,
    path: &Path,
    mut on_book: impl FnMut(&str),
) -> Vec<(PathBuf, Result<ImportOutcome, Error>)> {
    let manifest = export::read_manifest(path);
    let details: HashMap<&str, &export::ManifestBook> = manifest
        .iter()
        .flat_map(|m| &m.books)
        .map(|b| (b.file.as_str(), b))
        .collect();
    let mut outcomes = Vec::new();
    let result = archive::visit(path, &mut on_book, |name, staged| {
        let mut outcome = import_file(library, converter, staged);
        if let (Ok(ImportOutcome::Added { book, .. }), Some(entry)) =
            (&mut outcome, details.get(name))
        {
            // The book is in; details it can't take are left out.
            if export::restore_details(library, book.id, entry).is_ok()
                && let Ok(restored) = library.book(book.id)
            {
                *book = restored;
            }
        }
        outcomes.push((path.join(name), outcome));
    });
    if let Err(error) = result {
        outcomes.push((path.to_path_buf(), Err(error.into())));
    }
    outcomes
}

/// Imports `path`, using `converter` to make an EPUB if it's an Amazon book.
/// A KEPUB is cleaned of Kobo markup first, so the library keeps a plain EPUB.
pub fn import_file(
    library: &mut Library,
    converter: &dyn Converter,
    path: &Path,
) -> Result<ImportOutcome, Error> {
    if is_kepub(path) {
        if let Some(book) = library.book_from_file(path)? {
            return Ok(ImportOutcome::AlreadyInLibrary { book });
        }
        let workdir = tempfile::tempdir().map_err(parallax_convert::Error::from)?;
        let epub = workdir.path().join("book.epub");
        // An unreadable file falls through for the library to report.
        if parallax_epub::strip_kobo_markup(path, &epub).is_ok() {
            return Ok(library.import_converted(&epub, path)?);
        }
        return Ok(library.import(path)?);
    }
    // An unreadable file falls through for the library to report.
    if !matches!(amazon::sniff(path), Ok(Some(_))) {
        return Ok(library.import(path)?);
    }
    if let Some(book) = library.book_from_file(path)? {
        return Ok(ImportOutcome::AlreadyInLibrary { book });
    }
    let workdir = tempfile::tempdir().map_err(parallax_convert::Error::from)?;
    let epub = workdir.path().join("book.epub");
    converter.convert(path, &epub)?;
    Ok(library.import_converted(&epub, path)?)
}

/// `Dune.kepub.epub` or `Dune.kepub`, as Kobos and Parallax's KEPUB export
/// name them.
fn is_kepub(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    name.ends_with(".kepub.epub") || name.ends_with(".kepub")
}

/// Bundles a folder explicitly chosen as one volume and imports the resulting CBZ.
pub fn import_comic_folder(library: &mut Library, path: &Path) -> Result<ImportOutcome, Error> {
    let kind = classify_folder(path)?;
    let name = path
        .file_name()
        .ok_or_else(|| Error::ComicFolder(path.into()))?;
    let workdir = tempfile::tempdir().map_err(parallax_convert::Error::from)?;
    let mut file = name.to_os_string();
    file.push(".cbz");
    let cbz = workdir.path().join(file);
    match kind {
        FolderKind::Images => folder_to_cbz(path, &cbz)?,
        FolderKind::ChapterArchives => chapters_to_cbz(path, &cbz)?,
        FolderKind::Mixed | FolderKind::NotComic => return Err(Error::ComicFolder(path.into())),
    };
    Ok(library.import(&cbz)?)
}
