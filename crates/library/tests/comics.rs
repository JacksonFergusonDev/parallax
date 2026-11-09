//! Comics in the library: CBZ kept byte for byte, CB7 converted, metadata
//! from ComicInfo or the file name, kind and direction, export, covers and
//! snapshots.

use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use parallax_comic::{Cbz, Manga};
use parallax_library::{COMICS_KIND, Direction, Error, Format, ImportOutcome, Kind, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn png(width: u32, height: u32) -> Vec<u8> {
    let mut out = Vec::new();
    image::GrayImage::from_pixel(width, height, image::Luma([90]))
        .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Png)
        .unwrap();
    out
}

fn cbz(dir: &Path, name: &str, info: Option<&str>) -> PathBuf {
    let path = dir.join(name);
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    for (page, (w, h)) in [
        ("p1.png", (60, 90)),
        ("p2.png", (61, 90)),
        ("p10.png", (62, 90)),
    ] {
        zip.start_file(page, SimpleFileOptions::default()).unwrap();
        zip.write_all(&png(w, h)).unwrap();
    }
    if let Some(info) = info {
        zip.start_file("ComicInfo.xml", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(info.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

fn added(outcome: ImportOutcome) -> parallax_library::Book {
    match outcome {
        ImportOutcome::Added { book, .. } => book,
        ImportOutcome::AlreadyInLibrary { .. } => panic!("expected a new book"),
    }
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/comics")
        .join(name)
}

#[test]
fn cbz_is_stored_as_it_is_with_metadata_from_its_name() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let src = cbz(dir.path(), "Berserk v01 (2003) (Digital).cbz", None);
    let book = added(lib.import(&src).unwrap());

    assert_eq!(book.format, Format::Cbz);
    assert_eq!((book.kind, book.direction), (Kind::Manga, Direction::Rtl));
    assert_eq!(book.title, "Berserk Vol. 1");
    assert_eq!(book.series.as_deref(), Some("Berserk"));
    assert_eq!(book.series_index.as_deref(), Some("1"));
    assert_eq!(book.published.as_deref(), Some("2003"));
    assert_eq!(book.pages, Some(3));
    assert!(!book.pages_estimated);
    assert_eq!(book.words, None);
    assert!(book.has_cover);

    let stored = lib.book_file(book.id);
    assert!(stored.ends_with("book.cbz"));
    assert_eq!(fs::read(&stored).unwrap(), fs::read(&src).unwrap());
    // The cover is the first page, 60 × 90.
    assert_eq!(lib.cover_size(book.id), Some((60, 90)));
    assert!(lib.cover_source(book.id).unwrap().is_some());

    assert!(matches!(
        lib.import(&src).unwrap(),
        ImportOutcome::AlreadyInLibrary { .. }
    ));
    // Opening again doesn't try to measure the comic as an EPUB.
    drop(lib);
    let lib = Library::open(dir.path().join("lib")).unwrap();
    assert_eq!(lib.book(book.id).unwrap().words, None);
}

#[test]
fn comic_info_decides_metadata_kind_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let info = r#"<ComicInfo><Series>Saga</Series><Number>12</Number><Title>Chapter Twelve</Title>
        <Writer>Brian K. Vaughan</Writer><Penciller>Fiona Staples</Penciller>
        <Manga>No</Manga><Summary>Space opera.</Summary><Year>2013</Year>
        <Pages><Page Image="2" Type="FrontCover"/></Pages></ComicInfo>"#;
    let book = added(
        lib.import(&cbz(dir.path(), "saga.cbz", Some(info)))
            .unwrap(),
    );
    assert_eq!(book.title, "Chapter Twelve");
    assert_eq!(book.series_index.as_deref(), Some("12"));
    assert_eq!(book.authors, ["Brian K. Vaughan"]);
    let contributors = book.contributors.clone().unwrap();
    assert_eq!(contributors.len(), 1);
    assert_eq!(
        (contributors[0].name.as_str(), contributors[0].role.as_str()),
        ("Fiona Staples", "art")
    );
    assert_eq!((book.kind, book.direction), (Kind::Comic, Direction::Ltr));
    assert_eq!(book.description.as_deref(), Some("Space opera."));
    // FrontCover is the third page in reading order, p10.png at 62 × 90.
    assert_eq!(lib.cover_size(book.id), Some((62, 90)));
}

#[test]
fn the_default_kind_is_a_setting() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    lib.set_setting(COMICS_KIND, Some("comic")).unwrap();
    let book = added(
        lib.import(&cbz(dir.path(), "Batman 001.cbz", None))
            .unwrap(),
    );
    assert_eq!((book.kind, book.direction), (Kind::Comic, Direction::Ltr));
    assert_eq!(book.title, "Batman #1");
}

#[test]
fn seven_zip_is_converted_and_known_by_its_original() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let src = dir.path().join("Fixture v01.cb7");
    fs::copy(fixture("sample.cb7"), &src).unwrap();
    let book = added(lib.import(&src).unwrap());
    assert_eq!(book.format, Format::Cbz);
    assert_eq!(book.original_name, "Fixture v01.cb7");
    assert_eq!(book.pages, Some(3));
    // ComicInfo says right to left.
    assert_eq!((book.kind, book.direction), (Kind::Manga, Direction::Rtl));
    assert_eq!(
        parallax_comic::sniff(&lib.book_file(book.id)).unwrap(),
        parallax_comic::Format::Zip
    );
    assert!(matches!(
        lib.import(&src).unwrap(),
        ImportOutcome::AlreadyInLibrary { .. }
    ));
}

#[test]
fn kind_and_direction_can_change_for_comics_only() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let book = added(lib.import(&cbz(dir.path(), "x v1.cbz", None)).unwrap());
    let changed = lib
        .set_comic_kind(book.id, Kind::Comic, Direction::Ltr)
        .unwrap();
    assert_eq!(
        (changed.kind, changed.direction),
        (Kind::Comic, Direction::Ltr)
    );
    assert!(matches!(
        lib.set_comic_kind(book.id, Kind::Book, Direction::Ltr),
        Err(Error::NotAComic(_))
    ));
}

#[test]
fn export_writes_library_metadata_into_comic_info() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let info = "<ComicInfo><Series>Old</Series><ScanInformation>kept</ScanInformation></ComicInfo>";
    let book = added(lib.import(&cbz(dir.path(), "old.cbz", Some(info))).unwrap());
    let mut edit = book.edit();
    edit.title = "New Title".into();
    edit.series = Some("New Series".into());
    edit.series_index = Some("4".into());
    edit.authors = vec!["Someone".into()];
    lib.update(book.id, &edit).unwrap();

    let out = dir.path().join("out.cbz");
    lib.export(book.id, &out).unwrap();
    let mut exported = Cbz::open(&out).unwrap();
    let written = exported.comic_info().unwrap().clone();
    assert_eq!(written.title(), Some("New Title"));
    assert_eq!(written.series(), Some("New Series"));
    assert_eq!(written.volume(), Some("4"));
    assert_eq!(written.get("Writer"), Some("Someone"));
    assert_eq!(written.get("ScanInformation"), Some("kept"));
    assert_eq!(written.manga(), Manga::YesAndRightToLeft);
    assert_eq!(written.get("PageCount"), Some("3"));
    // Pages untouched, original untouched.
    let mut original = Cbz::open(&lib.book_file(book.id)).unwrap();
    assert_eq!(
        exported.read_page(0).unwrap(),
        original.read_page(0).unwrap()
    );
    assert_eq!(original.comic_info().unwrap().series(), Some("Old"));
}

#[test]
fn comics_survive_remove_and_snapshot_restore() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let book = added(lib.import(&cbz(dir.path(), "keep v2.cbz", None)).unwrap());
    let snapshot = lib.snapshot("remove", Some("keep")).unwrap();
    lib.remove(book.id).unwrap();
    assert!(lib.book(book.id).is_err());

    let file = snapshot.file_name().unwrap().to_string_lossy().into_owned();
    let outcome = lib.restore_snapshot(&file).unwrap();
    assert_eq!(outcome.restored, 1);
    let back = lib.book(book.id).unwrap();
    assert_eq!(back.format, Format::Cbz);
    assert!(lib.book_file(book.id).is_file());
}

#[test]
fn epubs_are_not_mistaken_for_comics() {
    // A zip with pictures and an EPUB container is still an EPUB.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("pictures.epub");
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    zip.start_file("mimetype", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("p1.png", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&png(10, 10)).unwrap();
    zip.finish().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    // It's a broken EPUB, so the EPUB import refuses it.
    assert!(matches!(lib.import(&path), Err(Error::Epub(_))));
}
