//! Home-screen covers for books sent by cable, ported from calibre
//! (`devices/kindle/driver.py` and `devices/mtp/driver.py`).
//!
//! A Kindle shows `system/thumbnails/thumbnail_{ASIN}_{cdetype}_portrait.jpg`
//! for a sideloaded book. When it goes online, Amazon replaces these with
//! generic covers for books it doesn't sell (see calibre's FAQ and
//! <https://www.mobileread.com/forums/showthread.php?t=329945>). Like calibre,
//! each thumbnail is also kept in `amazon-cover-bug/` and put back whenever
//! the Kindle connects. An index file there, named by the SHA-1 of the
//! book's path, records which thumbnail belongs to which book so removing a
//! book removes its thumbnail without reading the book.

use std::io::Cursor;

use image::ImageFormat;
use image::imageops::FilterType;

use crate::storage::{Storage, join};
use crate::{Error, Result};

pub const THUMBNAILS_DIR: &str = "system/thumbnails";
pub const CACHE_DIR: &str = "amazon-cover-bug";
/// calibre's `THUMBNAIL_HEIGHT` for Kindles: 500 since the Oasis (2017).
pub const THUMBNAIL_HEIGHT: u32 = 500;

/// A JPEG thumbnail of `cover` (any format `image` reads), 500 pixels high.
pub fn make(cover: &[u8]) -> Result<Vec<u8>> {
    let img = image::load_from_memory(cover).map_err(|e| Error::Image(e.to_string()))?;
    let img = if img.height() > THUMBNAIL_HEIGHT {
        img.resize(u32::MAX, THUMBNAIL_HEIGHT, FilterType::Lanczos3)
    } else {
        img
    };
    let mut out = Cursor::new(Vec::new());
    img.to_rgb8()
        .write_to(&mut out, ImageFormat::Jpeg)
        .map_err(|e| Error::Image(e.to_string()))?;
    Ok(out.into_inner())
}

fn index_name(book_path: &str) -> String {
    sha1_smol::Sha1::from(book_path.trim_matches('/'))
        .digest()
        .to_string()
}

/// Writes `jpeg` as the thumbnail `name` for the book at `book_path`.
pub fn write(storage: &dyn Storage, book_path: &str, name: &str, jpeg: &[u8]) -> Result<()> {
    storage.write(&join(&[THUMBNAILS_DIR, name]), jpeg, &mut |_, _| {})?;
    storage.write(&join(&[CACHE_DIR, name]), jpeg, &mut |_, _| {})?;
    storage.write(
        &join(&[CACHE_DIR, &index_name(book_path)]),
        name.as_bytes(),
        &mut |_, _| {},
    )
}

/// The thumbnail name recorded for a book, if Parallax or calibre wrote one.
pub fn recorded(storage: &dyn Storage, book_path: &str) -> Option<String> {
    let name = storage
        .read(&join(&[CACHE_DIR, &index_name(book_path)]))
        .ok()?;
    let name = String::from_utf8(name).ok()?.trim().to_owned();
    is_thumbnail(&name).then_some(name)
}

/// Removes a book's thumbnail, its cached copy and its index entry.
pub fn remove(storage: &dyn Storage, book_path: &str, name: Option<&str>) -> Result<()> {
    let name = name
        .map(str::to_owned)
        .or_else(|| recorded(storage, book_path));
    if let Some(name) = name {
        storage.remove(&join(&[THUMBNAILS_DIR, &name]))?;
        storage.remove(&join(&[CACHE_DIR, &name]))?;
    }
    storage.remove(&join(&[CACHE_DIR, &index_name(book_path)]))
}

fn is_thumbnail(name: &str) -> bool {
    name.starts_with("thumbnail_")
        && std::path::Path::new(name)
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("jpg"))
        && !name.contains('/')
}

/// Puts back thumbnails Amazon replaced or deleted. Returns how many.
pub fn restore(storage: &dyn Storage) -> Result<usize> {
    if !storage.is_dir(THUMBNAILS_DIR)? {
        return Ok(0);
    }
    let cached = storage.list(CACHE_DIR)?;
    if cached.is_empty() {
        return Ok(0);
    }
    let current: std::collections::HashMap<String, u64> = storage
        .list(THUMBNAILS_DIR)?
        .into_iter()
        .map(|e| (e.name, e.size))
        .collect();
    let mut restored = 0;
    for entry in cached.iter().filter(|e| !e.is_dir && is_thumbnail(&e.name)) {
        if current.get(&entry.name) != Some(&entry.size) {
            let data = storage.read(&join(&[CACHE_DIR, &entry.name]))?;
            storage.write(&join(&[THUMBNAILS_DIR, &entry.name]), &data, &mut |_, _| {})?;
            restored += 1;
        }
    }
    Ok(restored)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::DriveStorage;

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut out = Cursor::new(Vec::new());
        image::RgbImage::new(w, h)
            .write_to(&mut out, ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn makes_500px_jpeg() {
        let jpeg = make(&png(800, 1200)).unwrap();
        let img = image::load_from_memory(&jpeg).unwrap();
        assert_eq!(img.height(), 500);
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
        // Small covers aren't enlarged.
        assert_eq!(
            image::load_from_memory(&make(&png(100, 150)).unwrap())
                .unwrap()
                .height(),
            150
        );
        assert!(make(b"not an image").is_err());
    }

    #[test]
    fn write_restore_remove() {
        let dir = tempfile::tempdir().unwrap();
        let s = DriveStorage::new(dir.path());
        s.create_dir_all(THUMBNAILS_DIR).unwrap();
        let name = "thumbnail_abc_EBOK_portrait.jpg";
        write(&s, "documents/Book.azw3", name, b"jpeg").unwrap();
        assert_eq!(recorded(&s, "documents/Book.azw3").as_deref(), Some(name));

        // Amazon swaps in its generic cover, then deletes another.
        s.write(&join(&[THUMBNAILS_DIR, name]), b"generic!", &mut |_, _| {})
            .unwrap();
        assert_eq!(restore(&s).unwrap(), 1);
        assert_eq!(s.read(&join(&[THUMBNAILS_DIR, name])).unwrap(), b"jpeg");
        s.remove(&join(&[THUMBNAILS_DIR, name])).unwrap();
        assert_eq!(restore(&s).unwrap(), 1);
        assert_eq!(restore(&s).unwrap(), 0);

        remove(&s, "documents/Book.azw3", None).unwrap();
        assert!(!s.exists(&join(&[THUMBNAILS_DIR, name])).unwrap());
        assert!(s.list(CACHE_DIR).unwrap().is_empty());
    }
}
