use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};

use erc_epub::{Epub, MetadataEdit};
use erc_library::{Error, ImportOutcome, Library};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

fn png(width: u32, height: u32) -> Vec<u8> {
    let img = image::RgbImage::from_pixel(width, height, image::Rgb([200, 60, 40]));
    let mut out = Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}

/// An EPUB2 with the given metadata; `salt` changes the bytes without
/// changing the metadata.
fn epub(
    dir: &Path,
    name: &str,
    title: &str,
    creator: &str,
    isbn: Option<&str>,
    salt: &str,
) -> PathBuf {
    let isbn = isbn.map_or(String::new(), |i| {
        format!(r#"<dc:identifier opf:scheme="ISBN">{i}</dc:identifier>"#)
    });
    let opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>{title}</dc:title>
    <dc:creator opf:role="aut">{creator}</dc:creator>
    <dc:identifier id="uid">urn:uuid:{salt}</dc:identifier>
    {isbn}
    <meta name="cover" content="cv"/>
  </metadata>
  <manifest><item id="cv" href="cover.png" media-type="image/png"/></manifest>
  <spine/>
</package>"#
    );
    let path = dir.join(name);
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    zip.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
    )
    .unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
    zip.start_file("content.opf", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    zip.start_file("cover.png", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&png(800, 1200)).unwrap();
    zip.finish().unwrap();
    path
}

fn added(outcome: ImportOutcome) -> (erc_library::Book, Option<i64>) {
    match outcome {
        ImportOutcome::Added {
            book,
            possible_duplicate_of,
        } => (book, possible_duplicate_of),
        other => panic!("expected Added, got {other:?}"),
    }
}

#[test]
fn imports_with_normalised_metadata_and_thumbnail() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let src = epub(
        dir.path(),
        "pl.epub",
        "Paradise Lost",
        "Milton, John; Pullman, Philip;",
        Some("9780192806192"),
        "a",
    );

    let (book, dup) = added(lib.import(&src).unwrap());
    assert_eq!(book.title, "Paradise Lost");
    assert_eq!(book.authors, ["John Milton", "Philip Pullman"]);
    assert!(dup.is_none());
    assert!(book.has_cover);
    let thumb = image::open(lib.cover_file(book.id).unwrap()).unwrap();
    assert_eq!(thumb.height(), 600);
    // The stored original is byte-identical to the source.
    assert_eq!(
        fs::read(lib.book_file(book.id)).unwrap(),
        fs::read(&src).unwrap()
    );
}

#[test]
fn detects_exact_and_likely_duplicates() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let a = epub(dir.path(), "a.epub", "Dune", "Frank Herbert", None, "a");
    let (first, _) = added(lib.import(&a).unwrap());

    // Same file again.
    match lib.import(&a).unwrap() {
        ImportOutcome::AlreadyInLibrary { book } => assert_eq!(book.id, first.id),
        other => panic!("{other:?}"),
    }
    // Different file, same work (punctuation/case differences).
    let b = epub(dir.path(), "b.epub", "DUNE.", "Herbert, Frank", None, "b");
    let (_, dup) = added(lib.import(&b).unwrap());
    assert_eq!(dup, Some(first.id));
    // Different work.
    let c = epub(
        dir.path(),
        "c.epub",
        "Dune Messiah",
        "Frank Herbert",
        None,
        "c",
    );
    assert!(added(lib.import(&c).unwrap()).1.is_none());
    assert_eq!(lib.books().unwrap().len(), 3);
}

#[test]
fn edits_are_exported_but_originals_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let src = epub(dir.path(), "a.epub", "Old Title", "Someone", None, "a");
    let (book, _) = added(lib.import(&src).unwrap());

    let updated = lib
        .update(
            book.id,
            &MetadataEdit {
                title: "  New Title ".into(),
                authors: vec!["Ann Author".into(), " ".into()],
                series: Some("Saga".into()),
                series_index: Some("2".into()),
                tags: vec!["Sci-Fi".into()],
            },
        )
        .unwrap();
    assert_eq!(updated.title, "New Title");
    assert_eq!(updated.authors, ["Ann Author"]);

    let out = dir.path().join("export.epub");
    lib.export(book.id, &out).unwrap();
    let m = Epub::open(&out).unwrap().metadata();
    assert_eq!(m.title.as_deref(), Some("New Title"));
    assert_eq!(m.series.as_deref(), Some("Saga"));
    assert_eq!(m.tags, ["Sci-Fi"]);
    assert_eq!(
        fs::read(lib.book_file(book.id)).unwrap(),
        fs::read(&src).unwrap()
    );

    assert!(matches!(
        lib.update(
            book.id,
            &MetadataEdit {
                title: " ".into(),
                ..Default::default()
            }
        ),
        Err(Error::EmptyTitle)
    ));
}

#[test]
fn persists_and_removes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("lib");
    let id = {
        let mut lib = Library::open(&root).unwrap();
        let (book, _) = added(
            lib.import(&epub(
                dir.path(),
                "a.epub",
                "Emma",
                "Jane Austen",
                None,
                "a",
            ))
            .unwrap(),
        );
        lib.record_device_copy(
            book.id,
            "N123",
            "KEpub/Emma - Jane Austen.kepub.epub",
            "file:///mnt/onboard/KEpub/Emma - Jane Austen.kepub.epub",
        )
        .unwrap();
        book.id
    };
    let mut lib = Library::open(&root).unwrap();
    assert_eq!(lib.books().unwrap()[0].title, "Emma");
    assert_eq!(lib.device_copies("N123").unwrap()[0].book_id, id);

    lib.remove(id).unwrap();
    assert!(lib.books().unwrap().is_empty());
    assert!(lib.device_copies("N123").unwrap().is_empty());
    assert!(!lib.book_file(id).exists());
}

#[test]
fn failed_import_leaves_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let bad = dir.path().join("bad.epub");
    fs::write(&bad, b"not a zip").unwrap();
    assert!(lib.import(&bad).is_err());
    assert!(lib.books().unwrap().is_empty());
    assert_eq!(
        fs::read_dir(dir.path().join("lib/books")).unwrap().count(),
        0
    );
}

#[test]
fn custom_cover_is_used_on_export_and_can_be_reset() {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path().join("lib")).unwrap();
    let (book, _) = added(
        lib.import(&epub(
            dir.path(),
            "a.epub",
            "Dune",
            "Frank Herbert",
            None,
            "a",
        ))
        .unwrap(),
    );
    let original_thumb = fs::read(lib.cover_file(book.id).unwrap()).unwrap();

    // A tall JPEG gets capped to the Kobo screen height.
    let tall = {
        let img = image::RgbImage::from_pixel(1400, 2100, image::Rgb([10, 120, 200]));
        let mut out = Cursor::new(Vec::new());
        img.write_to(&mut out, image::ImageFormat::Jpeg).unwrap();
        out.into_inner()
    };
    let updated = lib.set_cover(book.id, &tall).unwrap();
    assert!(updated.custom_cover && updated.has_cover);
    assert_ne!(
        fs::read(lib.cover_file(book.id).unwrap()).unwrap(),
        original_thumb
    );

    let out = dir.path().join("out.epub");
    lib.export(book.id, &out).unwrap();
    let cover = Epub::open(&out).unwrap().cover().unwrap().unwrap();
    // The fixture's cover is a PNG, so the replacement is re-encoded to PNG
    // and swapped in place.
    assert_eq!(cover.media_type, "image/png");
    let img = image::load_from_memory(&cover.data).unwrap();
    assert_eq!((img.width(), img.height()), (1120, 1680));

    let reset = lib.reset_cover(book.id).unwrap();
    assert!(!reset.custom_cover && reset.has_cover);
    assert_eq!(
        fs::read(lib.cover_file(book.id).unwrap()).unwrap(),
        original_thumb
    );
    assert!(matches!(
        lib.set_cover(book.id, b"not an image"),
        Err(Error::BadImage(_))
    ));
}
