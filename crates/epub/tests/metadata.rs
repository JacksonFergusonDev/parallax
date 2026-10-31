use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use erc_epub::{Epub, MetadataEdit};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

const CONTAINER: &str = r#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#;

const EPUB2_OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>The Lies of Locke Lamora</dc:title>
    <dc:creator opf:role="aut" opf:file-as="Lynch, Scott">Scott Lynch</dc:creator>
    <dc:creator opf:role="ill">Some Illustrator</dc:creator>
    <dc:language>en</dc:language>
    <dc:identifier id="uid" opf:scheme="ISBN">9780553588941</dc:identifier>
    <dc:identifier>urn:uuid:1234</dc:identifier>
    <dc:subject>Fantasy</dc:subject>
    <dc:publisher>Bantam</dc:publisher>
    <meta name="calibre:series" content="Gentleman Bastard"/>
    <meta name="calibre:series_index" content="1.0"/>
    <meta name="cover" content="cover-img"/>
  </metadata>
  <manifest>
    <item id="cover-img" href="images/cover%20art.jpg" media-type="image/jpeg"/>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;

const EPUB3_OPF: &str = r##"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" xml:lang="en">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">urn:isbn:9780141439518</dc:identifier>
    <dc:title id="t1">Pride and Prejudice</dc:title>
    <meta refines="#t1" property="title-type">main</meta>
    <dc:creator id="c1">Jane Austen</dc:creator>
    <meta refines="#c1" property="role" scheme="marc:relators">aut</meta>
    <meta refines="#c1" property="file-as">Austen, Jane</meta>
    <dc:creator id="c2">An Editor</dc:creator>
    <meta refines="#c2" property="role" scheme="marc:relators">edt</meta>
    <dc:language>en</dc:language>
    <meta property="belongs-to-collection" id="s1">Austen Novels</meta>
    <meta refines="#s1" property="group-position">2</meta>
    <meta property="dcterms:modified">2026-01-01T00:00:00Z</meta>
  </metadata>
  <manifest>
    <item id="cv" href="cover.png" media-type="image/png" properties="cover-image"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
  </manifest>
  <spine><itemref idref="nav"/></spine>
</package>"##;

fn make_epub(dir: &Path, name: &str, opf: &str, files: &[(&str, &[u8])]) -> PathBuf {
    let path = dir.join(name);
    let mut zip = ZipWriter::new(File::create(&path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(CONTAINER.as_bytes()).unwrap();
    zip.start_file("OEBPS/content.opf", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    for (name, data) in files {
        zip.start_file(format!("OEBPS/{name}"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
    path
}

fn read_opf(path: &Path) -> String {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    let mut s = String::new();
    zip.by_name("OEBPS/content.opf")
        .unwrap()
        .read_to_string(&mut s)
        .unwrap();
    s
}

#[test]
fn reads_epub2_metadata_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(
        dir.path(),
        "b.epub",
        EPUB2_OPF,
        &[("images/cover art.jpg", b"JPEGDATA")],
    );
    let epub = Epub::open(&path).unwrap();
    let m = epub.metadata();
    assert_eq!(m.title.as_deref(), Some("The Lies of Locke Lamora"));
    assert_eq!(m.authors, ["Scott Lynch"]);
    assert_eq!(m.series.as_deref(), Some("Gentleman Bastard"));
    assert_eq!(m.series_index.as_deref(), Some("1.0"));
    assert_eq!(m.tags, ["Fantasy"]);
    assert_eq!(m.publisher.as_deref(), Some("Bantam"));
    assert_eq!(m.identifiers[0].scheme.as_deref(), Some("isbn"));
    assert_eq!(m.identifiers[0].value, "9780553588941");
    assert_eq!(m.identifiers[1].scheme.as_deref(), Some("uuid"));
    let cover = epub.cover().unwrap().unwrap();
    assert_eq!(cover.data, b"JPEGDATA");
    assert_eq!(cover.media_type, "image/jpeg");
}

#[test]
fn reads_epub3_metadata_and_cover() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(
        dir.path(),
        "b.epub",
        EPUB3_OPF,
        &[("cover.png", b"PNGDATA")],
    );
    let epub = Epub::open(&path).unwrap();
    let m = epub.metadata();
    assert_eq!(m.title.as_deref(), Some("Pride and Prejudice"));
    assert_eq!(m.authors, ["Jane Austen"]);
    assert_eq!(m.series.as_deref(), Some("Austen Novels"));
    assert_eq!(m.series_index.as_deref(), Some("2"));
    assert_eq!(m.identifiers[0].scheme.as_deref(), Some("isbn"));
    assert_eq!(epub.cover().unwrap().unwrap().data, b"PNGDATA");
}

fn edit() -> MetadataEdit {
    MetadataEdit {
        title: "New & Improved <Title>".into(),
        authors: vec!["First Author".into(), "Second Author".into()],
        series: Some("My Series".into()),
        series_index: Some("3".into()),
        tags: vec!["Sci-Fi".into(), "Favourites".into()],
    }
}

#[test]
fn edits_epub2_and_preserves_everything_else() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(
        dir.path(),
        "b.epub",
        EPUB2_OPF,
        &[
            ("images/cover art.jpg", b"JPEGDATA"),
            ("c1.xhtml", b"<html/>"),
        ],
    );
    let out = dir.path().join("out.epub");
    Epub::open(&path)
        .unwrap()
        .write_with_metadata(&edit(), &out)
        .unwrap();

    let edited = Epub::open(&out).unwrap();
    let m = edited.metadata();
    assert_eq!(m.title.as_deref(), Some("New & Improved <Title>"));
    assert_eq!(m.authors, ["First Author", "Second Author"]);
    assert_eq!(m.series.as_deref(), Some("My Series"));
    assert_eq!(m.series_index.as_deref(), Some("3"));
    assert_eq!(m.tags, ["Sci-Fi", "Favourites"]);
    // Untouched fields and the illustrator survive; the cover still resolves.
    assert_eq!(m.publisher.as_deref(), Some("Bantam"));
    assert_eq!(m.identifiers.len(), 2);
    let opf = read_opf(&out);
    assert!(opf.contains(r#"<dc:creator opf:role="ill">Some Illustrator</dc:creator>"#));
    assert!(!opf.contains("Scott Lynch") && !opf.contains("Gentleman Bastard"));
    assert!(!opf.contains("\n\n"), "removal left blank lines:\n{opf}");
    assert_eq!(edited.cover().unwrap().unwrap().data, b"JPEGDATA");

    // mimetype first and stored, other entries intact.
    let mut zip = ZipArchive::new(File::open(&out).unwrap()).unwrap();
    let first = zip.by_index(0).unwrap();
    assert_eq!(first.name().unwrap(), "mimetype");
    assert_eq!(first.compression(), CompressionMethod::Stored);
    drop(first);
    assert!(zip.by_name("OEBPS/c1.xhtml").is_ok());
}

#[test]
fn edits_epub3_and_drops_orphaned_refinements() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(
        dir.path(),
        "b.epub",
        EPUB3_OPF,
        &[("cover.png", b"PNGDATA")],
    );
    let out = dir.path().join("out.epub");
    Epub::open(&path)
        .unwrap()
        .write_with_metadata(&edit(), &out)
        .unwrap();

    let m = Epub::open(&out).unwrap().metadata();
    assert_eq!(m.authors, ["First Author", "Second Author"]);
    assert_eq!(m.series.as_deref(), Some("My Series"));
    let opf = read_opf(&out);
    for gone in [
        r##"refines="#t1""##,
        r##"refines="#c1""##,
        r##"refines="#s1""##,
        "Austen Novels",
    ] {
        assert!(!opf.contains(gone), "{gone} still present:\n{opf}");
    }
    // The editor and their role refinement are kept.
    assert!(opf.contains("An Editor") && opf.contains(r##"refines="#c2""##));
    assert!(opf.contains(r#"property="belongs-to-collection" id="erc-series""#));
    assert!(opf.contains("dcterms:modified"));
    roxmltree::Document::parse(&opf).expect("edited OPF is well-formed");
}

#[test]
fn rejects_non_epub() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.epub");
    std::fs::write(&path, b"nope").unwrap();
    assert!(Epub::open(&path).is_err());
}
