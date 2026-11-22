//! Sending books to a Kindle and removing them, with the files calibre writes
//! beside a book (`devices/kindle/driver.py`, `upload_cover`, `upload_apnx`,
//! `delete_single_book`): a cover thumbnail and an APNX page-number file in
//! the book's `.sdr` sidecar folder.

use crate::apnx;
use crate::books::{extension, sidecar_dir};
use crate::mobi;
use crate::storage::{Progress, Storage, join, split};
use crate::thumbnails;
use crate::{DOCUMENTS_DIR, Error, Result};

/// Files beside a book that belong to it, from calibre's `DELETE_EXTS`.
const SIDECAR_EXTENSIONS: &[&str] = &[
    "mbp", "tan", "pdr", "ea", "apnx", "phl", "mbp1", "mbs", "han",
];
/// Extensions APNX files are made for, as calibre's `upload_apnx`.
const APNX_EXTENSIONS: &[&str] = &["azw", "azw3", "mobi", "prc"];
/// Space left free on the Kindle after a send.
const SPARE_BYTES: u64 = 20 * 1024 * 1024;
/// FAT32 allows 255 characters; leave room for `.sdr` and `.apnx` names.
const MAX_NAME_CHARS: usize = 120;

/// A book to send.
pub struct Outgoing<'a> {
    /// File name in `documents/`, from [`file_name`].
    pub file_name: &'a str,
    pub data: &'a [u8],
    /// The cover in any common image format, for the home-screen thumbnail.
    pub cover: Option<&'a [u8]>,
    /// Write estimated page numbers (APNX).
    pub page_numbers: bool,
}

/// What a send wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sent {
    pub path: String,
    pub thumbnail: Option<String>,
    pub page_numbers: bool,
}

/// A file name for `title` and `authors`, as calibre's default template
/// `{title} - {authors}`, safe on FAT32 and MTP.
pub fn file_name(title: &str, authors: &[String], extension: &str) -> String {
    let mut stem = title.trim().to_owned();
    if !authors.is_empty() {
        stem = format!("{stem} - {}", authors.join(" & "));
    }
    let mut safe: String = stem
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    safe = safe
        .trim_matches(|c: char| c == '.' || c.is_whitespace())
        .to_owned();
    if safe.is_empty() {
        "Book".clone_into(&mut safe);
    }
    if safe.chars().count() > MAX_NAME_CHARS {
        let short: String = safe.chars().take(MAX_NAME_CHARS).collect();
        short.trim_end().clone_into(&mut safe);
    }
    format!("{safe}.{extension}")
}

/// Writes a book to `documents/`, replacing any file of the same name, then
/// its page numbers and thumbnail. Reading data in the book's `.sdr` folder
/// is kept, so a book sent again opens where it was.
pub fn send(storage: &dyn Storage, book: &Outgoing, progress: Progress) -> Result<Sent> {
    let path = join(&[DOCUMENTS_DIR, book.file_name]);
    let needed = book.data.len() as u64 + SPARE_BYTES;
    let replacing = storage.size(&path)?.unwrap_or(0);
    if let Some(free) = storage.free_space()
        && free + replacing < needed
    {
        return Err(Error::NoSpace {
            needed,
            free: free + replacing,
        });
    }
    storage.write(&path, book.data, progress)?;
    let header = mobi::parse(book.data);
    let mut sent = Sent {
        path: path.clone(),
        thumbnail: None,
        page_numbers: false,
    };
    let Some(header) = header else {
        return Ok(sent);
    };
    if book.page_numbers && APNX_EXTENSIONS.contains(&extension(&path).as_str()) {
        let guid = apnx::guid_for(header.asin.as_deref().unwrap_or(book.file_name));
        if let Some(data) = apnx::build(&header, &apnx::pages(book.data, &header), &guid) {
            let sdr = sidecar_dir(&path);
            let (_, stem) = split(&sdr);
            let stem = stem.trim_end_matches(".sdr");
            storage.write(
                &join(&[&sdr, &format!("{stem}.apnx")]),
                &data,
                &mut |_, _| {},
            )?;
            sent.page_numbers = true;
        }
    }
    // calibre writes thumbnails only where the Kindle keeps them.
    if let (Some(cover), Some(name)) = (book.cover, header.thumbnail_name())
        && storage.is_dir("system")?
        && let Ok(jpeg) = thumbnails::make(cover)
    {
        thumbnails::write(storage, &path, &name, &jpeg)?;
        sent.thumbnail = Some(name);
    }
    Ok(sent)
}

/// Removes a book with its sidecar folder, the sidecar files calibre's
/// Kindle driver deletes beside it, and its thumbnail.
pub fn remove(storage: &dyn Storage, path: &str) -> Result<()> {
    let header_name = storage
        .read_prefix(path, mobi::PREFIX_LEN)
        .ok()
        .and_then(|b| mobi::parse(&b))
        .and_then(|h| h.thumbnail_name());
    thumbnails::remove(storage, path, header_name.as_deref())?;
    storage.remove(path)?;
    storage.remove(&sidecar_dir(path))?;
    let (dir, name) = split(path);
    let stem = name.rsplit_once('.').map_or(name, |(stem, _)| stem);
    for entry in storage.list(dir)? {
        let Some((entry_stem, ext)) = entry.name.rsplit_once('.') else {
            continue;
        };
        if !entry.is_dir
            && entry_stem == stem
            && SIDECAR_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str())
        {
            storage.remove(&join(&[dir, &entry.name]))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::DriveStorage;

    fn fixture() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/amazon/book.azw3"),
        )
        .unwrap()
    }

    fn cover() -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(60, 90)
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        out.into_inner()
    }

    #[test]
    fn file_names() {
        assert_eq!(
            file_name("A: B?", &["X".into(), "Y".into()], "azw3"),
            "A_ B_ - X & Y.azw3"
        );
        assert_eq!(file_name("  ", &[], "pdf"), "Book.pdf");
        assert_eq!(
            file_name(&"a".repeat(300), &[], "azw3").len(),
            MAX_NAME_CHARS + 5
        );
    }

    #[test]
    fn send_writes_book_apnx_and_thumbnail_then_remove_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let s = DriveStorage::new(dir.path());
        s.create_dir_all("system/thumbnails").unwrap();
        let data = fixture();
        let cover = cover();
        let header = mobi::parse(&data).unwrap();
        let mut last = 0;
        let sent = send(
            &s,
            &Outgoing {
                file_name: "Book - Author.azw3",
                data: &data,
                cover: Some(&cover),
                page_numbers: true,
            },
            &mut |w, _| last = w,
        )
        .unwrap();
        assert_eq!(last, data.len() as u64);
        assert_eq!(sent.path, "documents/Book - Author.azw3");
        assert_eq!(s.read(&sent.path).unwrap(), data);
        assert!(sent.page_numbers);
        assert!(
            s.exists("documents/Book - Author.sdr/Book - Author.apnx")
                .unwrap()
        );
        assert_eq!(sent.thumbnail, header.thumbnail_name());
        if let Some(name) = &sent.thumbnail {
            assert!(s.exists(&format!("system/thumbnails/{name}")).unwrap());
        }

        s.write("documents/Book - Author.mbp", b"x", &mut |_, _| {})
            .unwrap();
        s.write("documents/Book - Author 2.mbp", b"x", &mut |_, _| {})
            .unwrap();
        remove(&s, &sent.path).unwrap();
        let left: Vec<_> = s
            .list("documents")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(left, ["Book - Author 2.mbp"]);
        assert!(s.list("system/thumbnails").unwrap().is_empty());
    }

    #[test]
    fn no_thumbnail_without_system_folder_and_none_for_pdf() {
        let dir = tempfile::tempdir().unwrap();
        let s = DriveStorage::new(dir.path());
        let cover = cover();
        let sent = send(
            &s,
            &Outgoing {
                file_name: "Notes.pdf",
                data: b"%PDF-1.4",
                cover: Some(&cover),
                page_numbers: true,
            },
            &mut |_, _| {},
        )
        .unwrap();
        assert_eq!(sent.thumbnail, None);
        assert!(!sent.page_numbers);
    }
}
