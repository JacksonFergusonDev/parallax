//! Folders as volumes: a folder of page images, or a folder of chapter
//! archives, made into one CBZ. The rules follow bindery's
//! (`processor.py` `_is_bundle_folder` and `_extract_chapter_folder`, MIT,
//! © 2026 jarynclouatre), reimplemented here.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::cbz::{Cbz, Format, sniff};
use crate::convert::convert_to_cbz;
use crate::entries::{Kind, classify};
use crate::error::{Error, Result, damaged, io};
use crate::natural::natural_cmp;

const ARCHIVE_EXTENSIONS: [&str; 6] = ["cbz", "cbr", "cb7", "zip", "rar", "7z"];
/// Books that aren't page images or archives; a folder mixing them in isn't
/// one volume.
const OTHER_BOOKS: [&str; 3] = ["pdf", "epub", "mobi"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderKind {
    /// Page images (possibly in chapter subfolders): one volume.
    Images,
    /// Chapter archives: one volume if the user asks for it.
    ChapterArchives,
    /// Images or archives mixed with each other or with other books.
    Mixed,
    /// Nothing comic-shaped.
    NotComic,
}

#[derive(Debug, Default)]
struct Contents {
    images: Vec<String>,
    archives: Vec<String>,
    other_books: usize,
    comic_info: Option<String>,
}

pub fn classify_folder(dir: &Path) -> Result<FolderKind> {
    let c = scan(dir)?;
    Ok(
        match (
            !c.images.is_empty(),
            !c.archives.is_empty(),
            c.other_books > 0,
        ) {
            (true, false, false) => FolderKind::Images,
            (false, true, false) => FolderKind::ChapterArchives,
            (false, false, false) => FolderKind::NotComic,
            _ => FolderKind::Mixed,
        },
    )
}

/// Relative paths (with `/`) of everything under `dir`, sorted naturally;
/// hidden files and folders are left out.
fn scan(dir: &Path) -> Result<Contents> {
    let mut contents = Contents::default();
    let mut stack = vec![PathBuf::new()];
    while let Some(rel) = stack.pop() {
        let entries = fs::read_dir(dir.join(&rel)).map_err(io(dir.join(&rel)))?;
        for entry in entries {
            let entry = entry.map_err(io(dir))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || name.eq_ignore_ascii_case("__MACOSX") {
                continue;
            }
            let child = rel.join(&name);
            let file_type = entry.file_type().map_err(io(entry.path()))?;
            if file_type.is_dir() {
                stack.push(child);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let rel_name = child.to_string_lossy().replace('\\', "/");
            let ext = name
                .rsplit_once('.')
                .map_or_default(|(_, e)| e.to_lowercase());
            match classify(&rel_name) {
                Kind::Page => contents.images.push(rel_name),
                Kind::ComicInfo if rel.as_os_str().is_empty() => {
                    contents.comic_info = Some(rel_name);
                }
                _ if ARCHIVE_EXTENSIONS.contains(&ext.as_str()) => {
                    contents.archives.push(rel_name);
                }
                _ if OTHER_BOOKS.contains(&ext.as_str()) => contents.other_books += 1,
                _ => {}
            }
        }
    }
    contents.images.sort_by(|a, b| natural_cmp(a, b));
    contents.archives.sort_by(|a, b| natural_cmp(a, b));
    Ok(contents)
}

/// What a bundle holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bundled {
    pub pages: usize,
    pub chapters: usize,
}

/// A folder of page images (and its ComicInfo.xml) as a CBZ at `dest`.
pub fn folder_to_cbz(dir: &Path, dest: &Path) -> Result<Bundled> {
    let contents = scan(dir)?;
    match classify_folder(dir)? {
        FolderKind::Images => {}
        other => return Err(not_bundlable(dir, other)),
    }
    let mut out = Output::create(dest)?;
    for name in &contents.images {
        let bytes = fs::read(dir.join(name)).map_err(io(dir.join(name)))?;
        out.add(name, &bytes)?;
    }
    if let Some(info) = &contents.comic_info {
        let bytes = fs::read(dir.join(info)).map_err(io(dir.join(info)))?;
        out.add_info(&bytes)?;
    }
    out.finish()?;
    Ok(Bundled {
        pages: contents.images.len(),
        chapters: 1,
    })
}

/// A folder of chapter archives as one CBZ, each chapter's pages in a
/// `NNN - <chapter>` folder so their order survives any reader's sort.
pub fn chapters_to_cbz(dir: &Path, dest: &Path) -> Result<Bundled> {
    let contents = scan(dir)?;
    match classify_folder(dir)? {
        FolderKind::ChapterArchives => {}
        other => return Err(not_bundlable(dir, other)),
    }
    let work = tempfile::tempdir().map_err(io(dir))?;
    let mut out = Output::create(dest)?;
    let mut pages = 0;
    for (i, rel) in contents.archives.iter().enumerate() {
        let path = dir.join(rel);
        let cbz_path = if sniff(&path)? == Format::Zip {
            path.clone()
        } else {
            let converted = work.path().join(format!("{i}.cbz"));
            convert_to_cbz(&path, &converted)?;
            converted
        };
        let mut chapter = Cbz::open(&cbz_path)?;
        let stem = Path::new(rel).file_stem().map_or_else(
            || format!("{}", i + 1),
            |s| s.to_string_lossy().into_owned(),
        );
        let prefix = chapter.wrapper().map(|w| format!("{w}/"));
        for p in 0..chapter.pages().len() {
            let name = chapter.pages()[p].name.clone();
            let inner = prefix
                .as_deref()
                .and_then(|w| name.strip_prefix(w))
                .unwrap_or(&name);
            let bytes = chapter.read_page(p)?;
            out.add(&format!("{:03} - {stem}/{inner}", i + 1), &bytes)?;
            pages += 1;
        }
    }
    out.finish()?;
    Ok(Bundled {
        pages,
        chapters: contents.archives.len(),
    })
}

fn not_bundlable(dir: &Path, kind: FolderKind) -> Error {
    let reason = match kind {
        FolderKind::Images => "it holds page images, not chapter archives",
        FolderKind::ChapterArchives => "it holds chapter archives, not page images",
        FolderKind::Mixed => "it mixes images, archives or other books",
        FolderKind::NotComic => "it has no page images or comic archives",
    };
    Error::NotBundlable {
        path: dir.to_path_buf(),
        reason: reason.to_owned(),
    }
}

/// A CBZ being written beside its destination, swapped in when finished
/// and removed if anything fails.
struct Output {
    dest: PathBuf,
    tmp: tempfile::NamedTempFile,
    zip: Option<ZipWriter<BufWriter<File>>>,
}

impl Output {
    fn create(dest: &Path) -> Result<Self> {
        let dir = dest.parent().unwrap_or(Path::new("."));
        let tmp = tempfile::NamedTempFile::new_in(dir).map_err(io(dir))?;
        let file = tmp.reopen().map_err(io(tmp.path()))?;
        Ok(Self {
            dest: dest.to_path_buf(),
            zip: Some(ZipWriter::new(BufWriter::new(file))),
            tmp,
        })
    }

    fn add(&mut self, name: &str, bytes: &[u8]) -> Result<()> {
        let zip = self.zip.as_mut().expect("open until finish");
        zip.start_file(
            name,
            SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
        )
        .map_err(damaged(&self.dest))?;
        zip.write_all(bytes).map_err(io(&self.dest))
    }

    fn add_info(&mut self, bytes: &[u8]) -> Result<()> {
        let zip = self.zip.as_mut().expect("open until finish");
        zip.start_file("ComicInfo.xml", SimpleFileOptions::default())
            .map_err(damaged(&self.dest))?;
        zip.write_all(bytes).map_err(io(&self.dest))
    }

    fn finish(mut self) -> Result<()> {
        let zip = self.zip.take().expect("open until finish");
        let file = zip
            .finish()
            .map_err(damaged(&self.dest))?
            .into_inner()
            .map_err(|e| Error::Io {
                path: self.dest.clone(),
                source: e.into_error(),
            })?;
        file.sync_all().map_err(io(&self.dest))?;
        self.tmp.persist(&self.dest).map_err(|e| Error::Io {
            path: self.dest.clone(),
            source: e.error,
        })?;
        Ok(())
    }
}
