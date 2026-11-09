//! Device-sized comic output. Only a bounded batch of decoded pages is held at once.
mod epub;
mod raster;

use std::path::{Path, PathBuf};

use erc_convert::Converter;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Cbz;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Profile {
    pub width: u32,
    pub height: u32,
    pub colour: bool,
    /// Enable for profiles verified to display facing halves in landscape.
    pub landscape_spreads: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Spreads {
    #[default]
    Split,
    KeepWhole,
    WholeThenHalves,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Options {
    pub spreads: Spreads,
    pub crop: bool,
    pub autocontrast: bool,
    pub upscale: bool,
    pub rtl: bool,
}

#[derive(Debug)]
pub struct Metadata<'a> {
    pub identifier: &'a str,
    pub title: &'a str,
    pub authors: &'a [String],
    pub credits: &'a [(String, String)],
    pub series: Option<&'a str>,
    pub series_index: Option<&'a str>,
    pub language: Option<&'a str>,
    /// EPUB's dcterms:modified timestamp, in UTC.
    pub modified: &'a str,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Archive(#[from] crate::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Zip(#[from] zip::result::ZipError),
    #[error(transparent)]
    Convert(#[from] erc_convert::Error),
    #[error("could not prepare {page}: {message}")]
    Page { page: String, message: String },
    #[error("invalid comic screen size {0} × {1}")]
    Screen(u32, u32),
}

pub struct Prepared {
    _work: tempfile::TempDir,
    path: PathBuf,
    pub pages: usize,
}

impl Prepared {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Prepare a fixed-layout KEPUB in a private temporary folder. Dropping the
/// result removes the output. Progress counts input pages, in archive order.
pub fn prepare(
    source: &Path,
    metadata: &Metadata<'_>,
    profile: Profile,
    options: Options,
    converter: &dyn Converter,
    mut progress: impl FnMut(usize, usize),
) -> Result<Prepared, Error> {
    if profile.width == 0
        || profile.height == 0
        || u64::from(profile.width) * u64::from(profile.height) > crate::shape::MAX_PIXELS
    {
        return Err(Error::Screen(profile.width, profile.height));
    }
    let work = tempfile::tempdir()?;
    let epub_path = work.path().join("comic.epub");
    let path = work.path().join("comic.kepub.epub");
    let mut source = Cbz::open(source)?;
    let total = source.pages().len();
    let cover = source.cover_index();
    let mut epub = epub::FixedLayout::new(&epub_path)?;
    let mut next = 0;
    // Bound both the decode working set and the number of large encoded inputs.
    while next < total {
        let mut batch = Vec::new();
        let mut pixels = 0;
        let mut bytes = 0;
        while next < total && batch.len() < 8 {
            let name = source.pages()[next].name.clone();
            let (width, height) = source.page_dimensions(next)?.ok_or_else(|| Error::Page {
                page: name.clone(),
                message: "missing or unsupported image dimensions".into(),
            })?;
            let area = u64::from(width) * u64::from(height);
            let size = source.pages()[next].size;
            if !batch.is_empty() && (pixels + area > 32_000_000 || bytes + size > 64 * 1024 * 1024)
            {
                break;
            }
            pixels += area;
            bytes += size;
            batch.push((name, source.read_page(next)?, next == cover));
            next += 1;
        }
        let start = next - batch.len();
        let prepared: Vec<_> = batch
            .into_par_iter()
            .map(|(name, bytes, cover)| {
                raster::process(&bytes, profile, options, cover).map_err(|message| Error::Page {
                    page: name,
                    message,
                })
            })
            .collect::<Result<_, _>>()?;
        for (offset, (pages, cover)) in prepared.into_iter().enumerate() {
            if let Some(cover) = cover {
                epub.add_cover(&cover)?;
            }
            for page in pages {
                epub.add_page(&page)?;
            }
            progress(start + offset + 1, total);
        }
    }
    let pages = epub.finish(metadata, profile, options)?;
    converter.convert_fixed_layout(&epub_path, &path)?;
    Ok(Prepared {
        _work: work,
        path,
        pages,
    })
}

/// KCC's KOBO_TOUCH_DRIVER_REGEX (comic2ebook.py, v13.0.0, ISC). Kobo
/// applies the restriction to each component below the device's mount point.
pub fn safe_kobo_component(name: &str) -> String {
    name.chars()
        .map(|c| {
            if r#"/\?%*:;|"'><$!"#.contains(c) {
                '_'
            } else {
                c
            }
        })
        .collect()
}
