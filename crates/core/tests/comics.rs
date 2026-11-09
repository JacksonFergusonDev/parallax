//! Comics through the app's own paths: adding one, and Check library.

use std::fs::File;
use std::io::{Cursor, Write};
use std::path::Path;

use erc_convert::amazon::InProcess;
use erc_core::health::{Check, check_library};
use erc_core::ingest::import_file;
use erc_library::{Format, ImportOutcome, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::GrayImage::new(width, height)
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn write_zip(path: &Path, entries: &[(&str, &[u8])]) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, bytes) in entries {
        zip.start_file(*name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn comics_are_added_through_the_normal_import() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let path = dir.path().join("Vinland Saga v03.cbz");
    write_zip(&path, &[("01.png", &png(20, 30)), ("02.png", &png(20, 30))]);
    let ImportOutcome::Added { book, .. } = import_file(&mut library, &InProcess, &path).unwrap()
    else {
        panic!("expected a new book");
    };
    assert_eq!(book.format, Format::Cbz);
    assert_eq!(book.title, "Vinland Saga Vol. 3");
    assert_eq!(book.pages, Some(2));
}

#[test]
fn check_library_reads_comics_as_comics() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let page = png(1300, 1900);
    let clean = dir.path().join("Clean v01.cbz");
    write_zip(&clean, &[("01.png", &page), ("02.png", &page)]);
    let messy = dir.path().join("Messy v01.cbz");
    write_zip(
        &messy,
        &[
            ("01.png", &page),
            ("02.png", b"not really a png"),
            (".DS_Store", b"junk"),
            ("release.nfo", b"scanned by"),
        ],
    );
    let id = |outcome| match outcome {
        ImportOutcome::Added { book, .. } => book.id,
        ImportOutcome::AlreadyInLibrary { .. } => panic!("expected a new book"),
    };
    let clean_id = id(library.import(&clean).unwrap());
    let messy_id = id(library.import(&messy).unwrap());

    let findings = check_library(&library, |_, _| {}).unwrap();
    let about = |book: i64| -> Vec<Check> {
        findings
            .iter()
            .filter(|f| f.book_ids.contains(&book))
            .map(|f| f.check)
            .collect()
    };
    // Comics without ComicInfo have no author; nothing else is wrong.
    assert_eq!(about(clean_id), [Check::NoAuthor]);
    let messy_checks = about(messy_id);
    for check in [Check::DamagedPages, Check::Clutter, Check::Unmanifested] {
        assert!(
            messy_checks.contains(&check),
            "{check:?} in {messy_checks:?}"
        );
    }
    assert!(!messy_checks.contains(&Check::Unreadable));
    let damaged = findings
        .iter()
        .find(|f| f.check == Check::DamagedPages)
        .unwrap();
    assert_eq!(damaged.detail, "02.png");
}
