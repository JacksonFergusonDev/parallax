//! Reading and editing EPUB metadata.
//!
//! Edits are splices into the original OPF text: only the elements this crate
//! manages (title, creators, subjects, series) are removed and re-added, so
//! everything else in the package document survives byte for byte. Every
//! other zip entry is copied without recompression.

mod opf;

use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

pub use opf::Identifier;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("not a valid EPUB: {0}")]
    Invalid(String),
    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),
}

pub type Result<T> = std::result::Result<T, Error>;

/// Metadata this app reads. Only the fields in [`MetadataEdit`] are written.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub authors: Vec<String>,
    pub series: Option<String>,
    /// Kept as text: "1", "2.5".
    pub series_index: Option<String>,
    pub tags: Vec<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub identifiers: Vec<Identifier>,
}

/// The editable subset of [`Metadata`]. Writing replaces all of these fields.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MetadataEdit {
    pub title: String,
    pub authors: Vec<String>,
    pub series: Option<String>,
    pub series_index: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Cover {
    pub data: Vec<u8>,
    pub media_type: String,
}

/// An EPUB on disk with its package document loaded.
pub struct Epub {
    path: PathBuf,
    opf_path: String,
    opf: String,
}

impl Epub {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut zip = open_zip(&path)?;
        let container = read_entry(&mut zip, "META-INF/container.xml")?;
        let opf_path = opf::rootfile_path(&container)?;
        let opf = read_entry(&mut zip, &opf_path)?;
        // Fail early on unparsable package documents.
        roxmltree::Document::parse(&opf).map_err(|e| Error::Invalid(format!("{opf_path}: {e}")))?;
        Ok(Self {
            path,
            opf_path,
            opf,
        })
    }

    pub fn metadata(&self) -> Metadata {
        opf::read_metadata(&self.doc())
    }

    /// The cover image declared in the manifest, if any.
    pub fn cover(&self) -> Result<Option<Cover>> {
        let Some((href, media_type)) = opf::cover_item(&self.doc()) else {
            return Ok(None);
        };
        let entry = resolve_href(&self.opf_path, &href);
        let mut zip = open_zip(&self.path)?;
        let data = match read_entry_bytes(&mut zip, &entry) {
            Ok(data) => data,
            Err(_) => return Ok(None),
        };
        Ok(Some(Cover { data, media_type }))
    }

    /// Writes a copy of this EPUB to `out` with `edit` applied.
    pub fn write_with_metadata(&self, edit: &MetadataEdit, out: &Path) -> Result<()> {
        let new_opf = opf::apply_edit(&self.opf, edit)?;
        let mut src = open_zip(&self.path)?;
        let file = File::create(out).map_err(|e| io_err(out, e))?;
        let mut dst = ZipWriter::new(BufWriter::new(file));

        // The OCF spec requires `mimetype` first and stored uncompressed.
        dst.start_file(
            "mimetype",
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )?;
        dst.write_all(b"application/epub+zip")
            .map_err(|e| io_err(out, e))?;

        for i in 0..src.len() {
            let entry = src.by_index_raw(i)?;
            let name = entry.name()?.into_owned();
            if name == "mimetype" {
                continue;
            }
            if name == self.opf_path {
                drop(entry);
                dst.start_file(
                    name,
                    SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
                )?;
                dst.write_all(new_opf.as_bytes())
                    .map_err(|e| io_err(out, e))?;
            } else {
                dst.raw_copy_file(entry)?;
            }
        }
        dst.finish()?.flush().map_err(|e| io_err(out, e))?;
        Ok(())
    }

    fn doc(&self) -> roxmltree::Document<'_> {
        roxmltree::Document::parse(&self.opf).expect("validated in open()")
    }
}

fn open_zip(path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(path).map_err(|e| io_err(path, e))?;
    ZipArchive::new(BufReader::new(file))
        .map_err(|e| Error::Invalid(format!("{}: {e}", path.display())))
}

fn read_entry_bytes(zip: &mut ZipArchive<BufReader<File>>, name: &str) -> Result<Vec<u8>> {
    let mut entry = zip
        .by_name(name)
        .map_err(|_| Error::Invalid(format!("missing {name}")))?;
    let mut buf = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut buf)
        .map_err(|e| Error::Invalid(format!("{name}: {e}")))?;
    Ok(buf)
}

fn read_entry(zip: &mut ZipArchive<BufReader<File>>, name: &str) -> Result<String> {
    let bytes = read_entry_bytes(zip, name)?;
    let text = String::from_utf8_lossy(&bytes);
    Ok(text.trim_start_matches('\u{feff}').to_owned())
}

/// Resolves a manifest href (relative to the OPF, possibly percent-encoded)
/// to a zip entry name.
fn resolve_href(opf_path: &str, href: &str) -> String {
    let href = percent_decode(href.split('#').next().unwrap_or(href));
    let mut parts: Vec<&str> = opf_path
        .rsplit_once('/')
        .map_or(vec![], |(dir, _)| dir.split('/').collect());
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Some(b) = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn io_err(path: &Path, source: std::io::Error) -> Error {
    Error::Io {
        path: path.to_path_buf(),
        source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_hrefs() {
        assert_eq!(
            resolve_href("OEBPS/content.opf", "images/c%20v.jpg"),
            "OEBPS/images/c v.jpg"
        );
        assert_eq!(
            resolve_href("OEBPS/content.opf", "../cover.jpg"),
            "cover.jpg"
        );
        assert_eq!(resolve_href("content.opf", "cover.jpg#x"), "cover.jpg");
    }
}
