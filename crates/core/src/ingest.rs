//! Adding a book file to the library, whatever its format. The library stores
//! EPUBs, so Amazon books are converted on the way in.

use std::path::Path;

use parallax_comic::{FolderKind, chapters_to_cbz, classify_folder, folder_to_cbz};
use parallax_convert::{Converter, amazon};
use parallax_library::{ImportOutcome, Library};

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
}

/// Imports `path`, using `converter` to make an EPUB if it's an Amazon book.
pub fn import_file(
    library: &mut Library,
    converter: &dyn Converter,
    path: &Path,
) -> Result<ImportOutcome, Error> {
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
