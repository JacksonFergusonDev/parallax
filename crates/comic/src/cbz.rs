//! Reading CBZ files: the pages in reading order, read one at a time.

use std::collections::HashSet;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use zip::ZipArchive;

use crate::comicinfo::ComicInfo;
use crate::entries::{
    Kind, MAX_COMIC_INFO_BYTES, MAX_ENTRIES, MAX_PAGE_BYTES, SkipReason, Skipped, classify,
    normalise, wrapper,
};
use crate::error::{Error, Result, damaged, io};
use crate::natural::natural_cmp;

/// What an archive file is, from its first bytes; the extension is often
/// wrong (a `.cbr` that is really a zip is common).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Zip,
    Rar,
    SevenZip,
}

pub fn sniff(path: &Path) -> Result<Format> {
    let mut head = [0u8; 8];
    let mut file = File::open(path).map_err(io(path))?;
    let n = file.read(&mut head).map_err(io(path))?;
    let head = &head[..n];
    if head.starts_with(b"PK\x03\x04") || head.starts_with(b"PK\x05\x06") {
        Ok(Format::Zip)
    } else if head.starts_with(b"Rar!\x1a\x07") {
        Ok(Format::Rar)
    } else if head.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Ok(Format::SevenZip)
    } else {
        Err(Error::NotAnArchive(path.to_path_buf()))
    }
}

/// A page: its name inside the archive and its uncompressed size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page {
    pub name: String,
    pub size: u64,
}

/// An open CBZ. Pages are in natural order of their full paths, so chapter
/// folders keep their order too.
pub struct Cbz {
    path: PathBuf,
    zip: ZipArchive<BufReader<File>>,
    /// Index into the zip for each page, in reading order.
    indices: Vec<usize>,
    pages: Vec<Page>,
    comic_info: Option<ComicInfo>,
    comic_info_error: Option<String>,
    skipped: Vec<Skipped>,
    wrapper: Option<String>,
}

impl Cbz {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path).map_err(io(path))?;
        let mut zip = ZipArchive::new(BufReader::new(file)).map_err(damaged(path))?;
        if zip.len() > MAX_ENTRIES {
            return Err(Error::TooManyEntries {
                path: path.to_path_buf(),
                limit: MAX_ENTRIES,
            });
        }
        let mut found: Vec<(usize, Page)> = Vec::new();
        let mut info_entry: Option<(usize, String)> = None;
        let mut skipped = Vec::new();
        let mut seen = HashSet::new();
        for i in 0..zip.len() {
            let entry = zip.by_index_raw(i).map_err(damaged(path))?;
            if entry.is_dir() {
                continue;
            }
            let name = normalise(&entry.name().map_err(damaged(path))?);
            if entry.encrypted() {
                return Err(Error::Encrypted(path.to_path_buf()));
            }
            match classify(&name) {
                Kind::Page if !seen.insert(name.to_lowercase()) => skipped.push(Skipped {
                    name,
                    reason: SkipReason::Duplicate,
                }),
                Kind::Page => {
                    let size = entry.size();
                    found.push((i, Page { name, size }));
                }
                // The shallowest ComicInfo.xml wins.
                Kind::ComicInfo => {
                    let depth = name.matches('/').count();
                    if info_entry
                        .as_ref()
                        .is_none_or(|(_, n)| n.matches('/').count() > depth)
                    {
                        info_entry = Some((i, name));
                    }
                }
                Kind::Skip(reason) => skipped.push(Skipped { name, reason }),
            }
        }
        if found.is_empty() {
            return Err(Error::NoPages(path.to_path_buf()));
        }
        found.sort_by(|a, b| natural_cmp(&a.1.name, &b.1.name));
        let (indices, pages): (Vec<usize>, Vec<Page>) = found.into_iter().unzip();
        let names: Vec<String> = pages.iter().map(|p| p.name.clone()).collect();

        let (mut comic_info, mut comic_info_error) = (None, None);
        if let Some((i, name)) = info_entry {
            let bytes = read_entry(&mut zip, i, path, &name, MAX_COMIC_INFO_BYTES)?;
            match ComicInfo::parse(&String::from_utf8_lossy(&bytes)) {
                Ok(info) => comic_info = Some(info),
                Err(e) => comic_info_error = Some(e.to_string()),
            }
        }
        Ok(Self {
            path: path.to_path_buf(),
            zip,
            indices,
            wrapper: wrapper(&names),
            pages,
            comic_info,
            comic_info_error,
            skipped,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn pages(&self) -> &[Page] {
        &self.pages
    }

    pub fn comic_info(&self) -> Option<&ComicInfo> {
        self.comic_info.as_ref()
    }

    /// Why a ComicInfo.xml that was present couldn't be read.
    pub fn comic_info_error(&self) -> Option<&str> {
        self.comic_info_error.as_deref()
    }

    /// Files that aren't pages, and why.
    pub fn skipped(&self) -> &[Skipped] {
        &self.skipped
    }

    /// The single folder all pages are in, if any.
    pub fn wrapper(&self) -> Option<&str> {
        self.wrapper.as_deref()
    }

    /// The page at `index` in reading order.
    pub fn read_page(&mut self, index: usize) -> Result<Vec<u8>> {
        let name = self.pages[index].name.clone();
        read_entry(
            &mut self.zip,
            self.indices[index],
            &self.path,
            &name,
            MAX_PAGE_BYTES,
        )
    }

    /// The width and height of the page at `index`, from its header. Only
    /// the start of the page is read unless the header is further in.
    pub fn page_dimensions(&mut self, index: usize) -> Result<Option<(u32, u32)>> {
        const HEAD: u64 = 64 * 1024;
        let mut head = Vec::new();
        self.zip
            .by_index(self.indices[index])
            .map_err(damaged(&self.path))?
            .take(HEAD)
            .read_to_end(&mut head)
            .map_err(|e| Error::Damaged {
                path: self.path.clone(),
                message: format!("{}: {e}", self.pages[index].name),
            })?;
        if let Some(size) = crate::shape::image_dimensions(&head) {
            return Ok(Some(size));
        }
        if (head.len() as u64) < HEAD {
            return Ok(None);
        }
        let bytes = self.read_page(index)?;
        Ok(crate::shape::image_dimensions(&bytes))
    }

    /// The cover: the page ComicInfo marks as FrontCover, else the first.
    pub fn cover_index(&self) -> usize {
        self.comic_info
            .as_ref()
            .and_then(ComicInfo::front_cover)
            .filter(|&i| i < self.pages.len())
            .unwrap_or(0)
    }
}

fn read_entry(
    zip: &mut ZipArchive<BufReader<File>>,
    index: usize,
    path: &Path,
    name: &str,
    limit: u64,
) -> Result<Vec<u8>> {
    let too_large = || Error::EntryTooLarge {
        path: path.to_path_buf(),
        name: name.to_owned(),
        limit,
    };
    let entry = zip.by_index(index).map_err(damaged(path))?;
    if entry.size() > limit {
        return Err(too_large());
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    // The header's size can lie; never read past the limit.
    entry
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::Damaged {
            path: path.to_path_buf(),
            message: format!("{name}: {e}"),
        })?;
    if bytes.len() as u64 > limit {
        return Err(too_large());
    }
    Ok(bytes)
}
