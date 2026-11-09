//! Device-sized comic output. Only a bounded batch of decoded pages is held at once.
mod epub;
mod raster;

use std::path::{Path, PathBuf};

use erc_convert::Converter;
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
    pub custom_cover: Option<&'a [u8]>,
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
    #[error("none of the comic's pages could be read")]
    NoReadablePages,
}

pub struct Prepared {
    _work: tempfile::TempDir,
    path: PathBuf,
    pub pages: usize,
    /// Pages left out because they aren't readable images, by archive name.
    pub skipped: Vec<String>,
}

impl Prepared {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Decoded pixels being processed at once, about 160 MB of RGB.
const MAX_PIXELS_IN_FLIGHT: u64 = 32_000_000;
/// Encoded input bytes being processed at once.
const MAX_BYTES_IN_FLIGHT: u64 = 64 * 1024 * 1024;

/// A page back from the pool, with its input for a possible cover.
struct Done {
    index: usize,
    name: String,
    data: Vec<u8>,
    pages: Result<Vec<raster::Raster>, String>,
    area: u64,
    size: u64,
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
    if let Some(bytes) = metadata.custom_cover {
        epub.add_cover(bytes, "custom cover")?;
    }
    // A damaged cover page passes the cover to the next readable page.
    let mut cover_pending = metadata.custom_cover.is_none();
    let mut skipped = Vec::new();
    // Pages are read in order here and processed on the rayon pool as soon
    // as there is room, so no core waits for the slowest page of a batch.
    // Results are written back in order. Room is bounded by decoded pixels
    // and encoded input bytes in flight, and by how far reading may run
    // ahead of writing while one slow page holds up the rest.
    let ahead = 4 * rayon::current_num_threads().max(1);
    let (sender, results) = std::sync::mpsc::channel::<Done>();
    let mut done = std::collections::BTreeMap::new();
    let (mut pixels, mut bytes) = (0, 0);
    let (mut next_read, mut next_write) = (0, 0);
    while next_write < total {
        let room =
            |pixels: u64, bytes: u64| pixels < MAX_PIXELS_IN_FLIGHT && bytes < MAX_BYTES_IN_FLIGHT;
        if next_read < total && next_read < next_write + ahead && room(pixels, bytes) {
            let index = next_read;
            next_read += 1;
            let name = source.pages()[index].name.clone();
            // An unreadable header is a damaged page, which Check library
            // reports; leave it out rather than failing the whole comic.
            let Some((width, height)) = source.page_dimensions(index)? else {
                done.insert(index, (name, None));
                continue;
            };
            let area = u64::from(width) * u64::from(height);
            let data = source.read_page(index)?;
            let size = data.len() as u64;
            pixels += area;
            bytes += size;
            let sender = sender.clone();
            rayon::spawn(move || {
                let pages = raster::process(&data, profile, options);
                // The receiver is gone only if preparation already failed.
                let _ = sender.send(Done {
                    index,
                    name,
                    data,
                    pages,
                    area,
                    size,
                });
            });
        } else if !done.contains_key(&next_write) {
            let finished = results.recv().expect("a page task is still running");
            pixels -= finished.area;
            bytes -= finished.size;
            done.insert(
                finished.index,
                (finished.name, Some((finished.data, finished.pages))),
            );
        }
        while let Some((name, page)) = done.remove(&next_write) {
            let index = next_write;
            next_write += 1;
            match page {
                Some((data, Ok(pages))) => {
                    if cover_pending && index >= cover {
                        epub.add_cover(&data, &name)?;
                        cover_pending = false;
                    }
                    for page in pages {
                        epub.add_page(&page)?;
                    }
                }
                _ => skipped.push(name),
            }
            progress(index + 1, total);
        }
    }
    if epub.is_empty() {
        return Err(Error::NoReadablePages);
    }
    let pages = epub.finish(metadata, profile, options)?;
    converter.convert_fixed_layout(&epub_path, &path)?;
    Ok(Prepared {
        _work: work,
        path,
        pages,
        skipped,
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
