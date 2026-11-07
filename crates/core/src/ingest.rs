//! Adding a book file to the library, whatever its format. The library stores
//! EPUBs, so Amazon books are converted on the way in.

use std::path::Path;

use erc_convert::{Converter, amazon};
use erc_library::{ImportOutcome, Library};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Library(#[from] erc_library::Error),
    #[error(transparent)]
    Convert(#[from] erc_convert::Error),
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
    let workdir = tempfile::tempdir().map_err(erc_convert::Error::from)?;
    let epub = workdir.path().join("book.epub");
    converter.convert(path, &epub)?;
    Ok(library.import_converted(&epub, path)?)
}
