//! Comic archives end to end: CBZ files built here, a 7z made with macOS's
//! bsdtar, and libarchive's own RAR4 and RAR5 test files.

use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use erc_comic::{
    Cbz, ComicInfo, Error, FolderKind, Format, Manga, SkipReason, chapters_to_cbz, classify_folder,
    convert_to_cbz, folder_to_cbz, sniff, write_comic_info,
};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/comics")
        .join(name)
}

/// A greyscale PNG whose size identifies it.
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

fn page_names(cbz: &Cbz) -> Vec<&str> {
    cbz.pages().iter().map(|p| p.name.as_str()).collect()
}

const INFO: &str = r#"<?xml version="1.0"?>
<ComicInfo>
  <Series>Test Series</Series>
  <Volume>2</Volume>
  <Manga>YesAndRightToLeft</Manga>
  <Pages><Page Image="1" Type="FrontCover"/></Pages>
</ComicInfo>"#;

#[test]
fn cbz_pages_come_in_reading_order_with_comic_info() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.cbz");
    let (p1, p2, p10) = (png(10, 14), png(11, 14), png(28, 14));
    write_zip(
        &path,
        &[
            ("Vol 2/p10.png", &p10),
            ("Vol 2/p2.png", &p2),
            ("Vol 2/p1.png", &p1),
            ("Vol 2/ComicInfo.xml", INFO.as_bytes()),
            ("__MACOSX/Vol 2/._p1.png", b"junk"),
            ("Vol 2/scan.nfo", b"nfo"),
        ],
    );
    assert_eq!(sniff(&path).unwrap(), Format::Zip);
    let mut cbz = Cbz::open(&path).unwrap();
    assert_eq!(
        page_names(&cbz),
        ["Vol 2/p1.png", "Vol 2/p2.png", "Vol 2/p10.png"]
    );
    assert_eq!(cbz.wrapper(), Some("Vol 2"));
    assert!(cbz.chapters().is_empty());
    let info = cbz.comic_info().unwrap();
    assert_eq!(info.series(), Some("Test Series"));
    assert_eq!(info.manga(), Manga::YesAndRightToLeft);
    assert_eq!(cbz.cover_index(), 1);
    assert_eq!(cbz.read_page(2).unwrap(), p10);
    assert_eq!(cbz.page_dimensions(2).unwrap(), Some((28, 14)));
    let reasons: Vec<SkipReason> = cbz.skipped().iter().map(|s| s.reason).collect();
    assert!(reasons.contains(&SkipReason::Hidden));
    assert!(reasons.contains(&SkipReason::NotAnImage));
}

#[test]
fn chapter_folders_under_a_volume_wrapper_are_in_reading_order() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.cbz");
    let page = png(4, 6);
    write_zip(
        &path,
        &[
            ("Volume/Chapter 10/1.png", &page),
            ("Volume/Chapter 2/2.png", &page),
            ("Volume/Chapter 2/1.png", &page),
            (
                "ComicInfo.xml",
                br#"<ComicInfo><Pages><Page Image="99" Bookmark="Invalid"/></Pages></ComicInfo>"#,
            ),
        ],
    );
    let cbz = Cbz::open(&path).unwrap();
    let chapters = cbz.chapters();
    let entries: Vec<_> = chapters
        .iter()
        .map(|c| (c.page, c.title.as_str()))
        .collect();
    assert_eq!(entries, [(0, "Chapter 2"), (2, "Chapter 10")]);
}

#[test]
fn unsafe_and_duplicate_names_are_never_pages() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("odd.cbz");
    let page = png(4, 6);
    write_zip(
        &path,
        &[
            ("../escape.png", &page),
            ("/abs.png", &page),
            ("p1.png", &page),
            ("P1.png", &page),
            ("-01.png", &page),
        ],
    );
    let cbz = Cbz::open(&path).unwrap();
    assert_eq!(page_names(&cbz), ["-01.png", "p1.png"]);
    let skipped: Vec<(&str, SkipReason)> = cbz
        .skipped()
        .iter()
        .map(|s| (s.name.as_str(), s.reason))
        .collect();
    assert!(skipped.contains(&("../escape.png", SkipReason::UnsafePath)));
    assert!(skipped.contains(&("/abs.png", SkipReason::UnsafePath)));
    assert!(skipped.contains(&("P1.png", SkipReason::Duplicate)));
}

#[test]
fn archives_without_pages_or_not_archives_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.cbz");
    write_zip(&empty, &[("readme.txt", b"no pages")]);
    assert!(matches!(Cbz::open(&empty), Err(Error::NoPages(_))));

    let text = dir.path().join("fake.cbr");
    fs::write(&text, "not an archive").unwrap();
    assert!(matches!(sniff(&text), Err(Error::NotAnArchive(_))));
}

#[test]
fn writing_comic_info_keeps_pages_byte_for_byte() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("book.cbz");
    let (a, b) = (png(5, 7), png(6, 7));
    write_zip(&path, &[("a.png", &a), ("b.png", &b)]);
    assert!(Cbz::open(&path).unwrap().comic_info().is_none());

    let mut info = ComicInfo::new();
    info.set("Series", Some("Added"));
    write_comic_info(&path, &info).unwrap();
    let mut cbz = Cbz::open(&path).unwrap();
    assert_eq!(cbz.comic_info().unwrap().series(), Some("Added"));
    assert_eq!(cbz.read_page(0).unwrap(), a);
    assert_eq!(cbz.read_page(1).unwrap(), b);

    info.set("Series", Some("Changed"));
    write_comic_info(&path, &info).unwrap();
    let cbz = Cbz::open(&path).unwrap();
    assert_eq!(cbz.comic_info().unwrap().series(), Some("Changed"));
    assert_eq!(cbz.pages().len(), 2);
    // Only one ComicInfo.xml, and no temporary files left beside it.
    let leftovers: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    assert_eq!(leftovers.len(), 1);
}

#[test]
fn seven_zip_converts_to_cbz() {
    let src = fixture("sample.cb7");
    assert_eq!(sniff(&src).unwrap(), Format::SevenZip);
    let dir = tempfile::tempdir().unwrap();
    let dest = dir.path().join("sample.cbz");
    let converted = convert_to_cbz(&src, &dest).unwrap();
    assert_eq!(converted.pages, 3);
    assert!(converted.comic_info);
    assert!(
        converted
            .skipped
            .iter()
            .any(|s| s.name == "Vol 1/notes.txt" && s.reason == SkipReason::NotAnImage)
    );
    let mut cbz = Cbz::open(&dest).unwrap();
    assert_eq!(
        page_names(&cbz),
        ["Vol 1/p1.png", "Vol 1/p2.png", "Vol 1/p10.png"]
    );
    assert_eq!(cbz.comic_info().unwrap().series(), Some("Fixture"));
    assert_eq!(cbz.page_dimensions(2).unwrap(), Some((5, 3)));
}

#[test]
fn macos_libarchive_reads_rar4_and_rar5() {
    for (name, entry, size) in [
        ("libarchive-rar4.rar", "test.txt", 20),
        ("libarchive-rar5.rar", "test.bin", 1200),
    ] {
        let path = fixture(name);
        assert_eq!(sniff(&path).unwrap(), Format::Rar, "{name}");
        let mut out = Vec::new();
        compress_tools::uncompress_archive_file(File::open(&path).unwrap(), &mut out, entry)
            .unwrap();
        assert_eq!(out.len(), size, "{name}");
        // They hold no pages, so they aren't comics.
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("x.cbz");
        assert!(matches!(
            convert_to_cbz(&path, &dest),
            Err(Error::NoPages(_))
        ));
        assert!(!dest.exists());
    }
}

#[test]
fn damaged_archives_are_errors_not_crashes() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = fs::read(fixture("sample.cb7")).unwrap();
    let cut = dir.path().join("cut.cb7");
    fs::write(&cut, &bytes[..bytes.len() / 2]).unwrap();
    let dest = dir.path().join("cut.cbz");
    assert!(convert_to_cbz(&cut, &dest).is_err());
    assert!(!dest.exists());

    let zip = dir.path().join("cut.cbz");
    let mut whole = Vec::new();
    {
        let mut w = ZipWriter::new(Cursor::new(&mut whole));
        w.start_file("p1.png", SimpleFileOptions::default())
            .unwrap();
        w.write_all(&png(30, 40)).unwrap();
        w.finish().unwrap();
    }
    fs::write(&zip, &whole[..whole.len() - 30]).unwrap();
    assert!(Cbz::open(&zip).is_err());
}

#[test]
fn folders_of_images_become_one_volume() {
    let dir = tempfile::tempdir().unwrap();
    let vol = dir.path().join("Vol 1");
    fs::create_dir_all(vol.join("Chapter 10")).unwrap();
    fs::create_dir_all(vol.join("Chapter 2")).unwrap();
    fs::create_dir_all(vol.join(".hidden")).unwrap();
    fs::write(vol.join("Chapter 10/01.png"), png(3, 4)).unwrap();
    fs::write(vol.join("Chapter 2/02.png"), png(3, 4)).unwrap();
    fs::write(vol.join("Chapter 2/01.png"), png(3, 4)).unwrap();
    fs::write(vol.join(".hidden/x.png"), png(3, 4)).unwrap();
    fs::write(vol.join("info.txt"), "ignored").unwrap();
    assert_eq!(classify_folder(&vol).unwrap(), FolderKind::Images);

    let dest = dir.path().join("vol.cbz");
    let bundled = folder_to_cbz(&vol, &dest).unwrap();
    assert_eq!(bundled.pages, 3);
    let cbz = Cbz::open(&dest).unwrap();
    assert_eq!(
        page_names(&cbz),
        ["Chapter 2/01.png", "Chapter 2/02.png", "Chapter 10/01.png"]
    );
    // Chapter archives aren't page images.
    assert!(matches!(
        chapters_to_cbz(&vol, &dir.path().join("no.cbz")),
        Err(Error::NotBundlable { .. })
    ));
}

#[test]
fn folders_of_chapters_become_one_volume_in_order() {
    let dir = tempfile::tempdir().unwrap();
    let series = dir.path().join("Series");
    fs::create_dir(&series).unwrap();
    let page = png(3, 4);
    write_zip(&series.join("Ch 10.cbz"), &[("p1.png", &page)]);
    write_zip(
        &series.join("Ch 2.cbz"),
        &[("Ch 2/p2.png", &page), ("Ch 2/p1.png", &page)],
    );
    fs::copy(fixture("sample.cb7"), series.join("Ch 1.cb7")).unwrap();
    fs::write(series.join(".DS_Store"), "junk").unwrap();
    assert_eq!(
        classify_folder(&series).unwrap(),
        FolderKind::ChapterArchives
    );

    let dest = dir.path().join("bundle.cbz");
    let bundled = chapters_to_cbz(&series, &dest).unwrap();
    assert_eq!((bundled.chapters, bundled.pages), (3, 6));
    let cbz = Cbz::open(&dest).unwrap();
    assert_eq!(
        page_names(&cbz),
        [
            "001 - Ch 1/p1.png",
            "001 - Ch 1/p2.png",
            "001 - Ch 1/p10.png",
            "002 - Ch 2/p1.png",
            "002 - Ch 2/p2.png",
            "003 - Ch 10/p1.png",
        ]
    );
}

#[test]
fn mixed_and_empty_folders_are_not_volumes() {
    let dir = tempfile::tempdir().unwrap();
    let mixed = dir.path().join("mixed");
    fs::create_dir(&mixed).unwrap();
    fs::write(mixed.join("p1.png"), png(3, 4)).unwrap();
    write_zip(&mixed.join("Ch 1.cbz"), &[("p1.png", &png(3, 4))]);
    assert_eq!(classify_folder(&mixed).unwrap(), FolderKind::Mixed);

    let with_pdf = dir.path().join("pdf");
    fs::create_dir(&with_pdf).unwrap();
    fs::write(with_pdf.join("p1.png"), png(3, 4)).unwrap();
    fs::write(with_pdf.join("book.pdf"), "%PDF").unwrap();
    assert_eq!(classify_folder(&with_pdf).unwrap(), FolderKind::Mixed);

    let empty = dir.path().join("empty");
    fs::create_dir(&empty).unwrap();
    fs::write(empty.join("notes.txt"), "nothing").unwrap();
    assert_eq!(classify_folder(&empty).unwrap(), FolderKind::NotComic);
    assert!(folder_to_cbz(&empty, &dir.path().join("x.cbz")).is_err());
}
