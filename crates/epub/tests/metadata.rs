use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use parallax_epub::{Contributor, Epub, MetadataEdit};
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
        contributors: None,
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
    assert!(opf.contains(r#"property="belongs-to-collection" id="parallax-series""#));
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

fn cover(data: &[u8], media_type: &str) -> parallax_epub::Cover {
    parallax_epub::Cover {
        data: data.to_vec(),
        media_type: media_type.into(),
    }
}

#[test]
fn replaces_same_type_cover_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(
        dir.path(),
        "b.epub",
        EPUB2_OPF,
        &[("images/cover art.jpg", b"OLDJPEG")],
    );
    let out = dir.path().join("out.epub");
    let epub = Epub::open(&path).unwrap();
    epub.write(&edit(), Some(&cover(b"NEWJPEG", "image/jpeg")), &out)
        .unwrap();

    let edited = Epub::open(&out).unwrap();
    assert_eq!(edited.cover().unwrap().unwrap().data, b"NEWJPEG");
    let opf = read_opf(&out);
    assert!(
        !opf.contains("parallax-cover"),
        "should reuse the existing item:\n{opf}"
    );
    let mut zip = ZipArchive::new(File::open(&out).unwrap()).unwrap();
    let names: Vec<String> = zip.file_names().map(|n| n.unwrap().into_owned()).collect();
    assert_eq!(
        names
            .iter()
            .filter(|n| n.ends_with("cover art.jpg"))
            .count(),
        1
    );
    assert!(zip.by_name("OEBPS/images/cover art.jpg").is_ok());
}

#[test]
fn adds_cover_item_when_type_differs_epub2() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(
        dir.path(),
        "b.epub",
        EPUB2_OPF,
        &[("images/cover art.jpg", b"OLDJPEG")],
    );
    let out = dir.path().join("out.epub");
    Epub::open(&path)
        .unwrap()
        .write(&edit(), Some(&cover(b"NEWPNG", "image/png")), &out)
        .unwrap();

    let edited = Epub::open(&out).unwrap();
    let c = edited.cover().unwrap().unwrap();
    assert_eq!(
        (c.data.as_slice(), c.media_type.as_str()),
        (b"NEWPNG".as_slice(), "image/png")
    );
    let opf = read_opf(&out);
    assert_eq!(opf.matches(r#"name="cover""#).count(), 1, "{opf}");
    assert!(opf.contains(
        r#"<item id="parallax-cover" href="parallax-cover.png" media-type="image/png"/>"#
    ));
    roxmltree::Document::parse(&opf).expect("well-formed");
}

#[test]
fn adds_cover_item_epub3_and_moves_cover_image_property() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(dir.path(), "b.epub", EPUB3_OPF, &[("cover.png", b"OLDPNG")]);
    let out = dir.path().join("out.epub");
    Epub::open(&path)
        .unwrap()
        .write(&edit(), Some(&cover(b"NEWJPEG", "image/jpeg")), &out)
        .unwrap();

    assert_eq!(
        Epub::open(&out).unwrap().cover().unwrap().unwrap().data,
        b"NEWJPEG"
    );
    let opf = read_opf(&out);
    assert_eq!(opf.matches("cover-image").count(), 1, "{opf}");
    assert!(opf.contains(r#"<item id="cv" href="cover.png" media-type="image/png"/>"#));
    assert!(opf.contains(r#"properties="cover-image"/>"#));
}

#[test]
fn adds_cover_to_book_without_one() {
    let dir = tempfile::tempdir().unwrap();
    let opf = EPUB2_OPF
        .replace(r#"<meta name="cover" content="cover-img"/>"#, "")
        .replace(
            r#"<item id="cover-img" href="images/cover%20art.jpg" media-type="image/jpeg"/>"#,
            "",
        );
    let path = make_epub(dir.path(), "b.epub", &opf, &[]);
    assert!(Epub::open(&path).unwrap().cover().unwrap().is_none());
    let out = dir.path().join("out.epub");
    Epub::open(&path)
        .unwrap()
        .write(&edit(), Some(&cover(b"NEWJPEG", "image/jpeg")), &out)
        .unwrap();
    assert_eq!(
        Epub::open(&out).unwrap().cover().unwrap().unwrap().data,
        b"NEWJPEG"
    );
}

fn people() -> Vec<Contributor> {
    vec![
        Contributor {
            name: "Ann Translator".into(),
            role: "trl".into(),
        },
        Contributor {
            name: "Bo & Illustrator".into(),
            role: "ILL".into(),
        },
    ]
}

#[test]
fn reads_contributors_with_roles() {
    let dir = tempfile::tempdir().unwrap();
    let epub2 = make_epub(dir.path(), "2.epub", EPUB2_OPF, &[]);
    let epub3 = make_epub(dir.path(), "3.epub", EPUB3_OPF, &[]);
    assert_eq!(
        Epub::open(&epub2).unwrap().metadata().contributors,
        [Contributor {
            name: "Some Illustrator".into(),
            role: "ill".into()
        }]
    );
    assert_eq!(
        Epub::open(&epub3).unwrap().metadata().contributors,
        [Contributor {
            name: "An Editor".into(),
            role: "edt".into()
        }]
    );
}

#[test]
fn writes_contributors_epub2_keeping_the_book_producer() {
    let dir = tempfile::tempdir().unwrap();
    let opf = EPUB2_OPF.replace(
        "<dc:language>en</dc:language>",
        "<dc:contributor opf:role=\"bkp\">calibre (7.0) [https://calibre-ebook.com]</dc:contributor>\n    <dc:language>en</dc:language>",
    );
    let path = make_epub(dir.path(), "b.epub", &opf, &[]);
    let out = dir.path().join("out.epub");
    let edit = MetadataEdit {
        contributors: Some(people()),
        ..edit()
    };
    Epub::open(&path)
        .unwrap()
        .write_with_metadata(&edit, &out)
        .unwrap();

    let m = Epub::open(&out).unwrap().metadata();
    assert_eq!(
        m.contributors,
        [
            Contributor {
                name: "Ann Translator".into(),
                role: "trl".into()
            },
            Contributor {
                name: "Bo & Illustrator".into(),
                role: "ill".into()
            },
        ]
    );
    let opf = read_opf(&out);
    assert!(!opf.contains("Some Illustrator"));
    assert!(opf.contains(r#"<dc:contributor opf:role="trl">Ann Translator</dc:contributor>"#));
    assert!(opf.contains(r#"<dc:contributor opf:role="bkp">calibre (7.0)"#));
    assert!(!opf.contains("\n\n"), "removal left blank lines:\n{opf}");
}

#[test]
fn writes_contributors_epub3_and_none_leaves_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(dir.path(), "b.epub", EPUB3_OPF, &[]);

    // None: the editor and its refinements are untouched.
    let kept = dir.path().join("kept.epub");
    Epub::open(&path)
        .unwrap()
        .write_with_metadata(&edit(), &kept)
        .unwrap();
    let opf = read_opf(&kept);
    assert!(
        opf.contains(r##"<meta refines="#c2" property="role" scheme="marc:relators">edt</meta>"##)
    );

    let out = dir.path().join("out.epub");
    let edit = MetadataEdit {
        contributors: Some(people()),
        ..edit()
    };
    Epub::open(&path)
        .unwrap()
        .write_with_metadata(&edit, &out)
        .unwrap();
    let opf = read_opf(&out);
    assert!(!opf.contains("An Editor") && !opf.contains("#c2"));
    assert!(opf.contains(
        r#"<dc:contributor id="parallax-contributor-1">Ann Translator</dc:contributor>"#
    ));
    assert!(opf.contains(
        r##"<meta refines="#parallax-contributor-2" property="role" scheme="marc:relators">ill</meta>"##
    ));
    assert_eq!(Epub::open(&out).unwrap().metadata().contributors, {
        let mut p = people();
        p[1].role = "ill".into();
        p
    });
}

fn opf_with(metadata: &str, manifest: &str, spine: &str) -> String {
    format!(
        r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:title>T</dc:title>{metadata}
  </metadata>
  <manifest>{manifest}</manifest>
  <spine>{spine}</spine>
</package>"#
    )
}

fn published(dates: &str) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(dir.path(), "b.epub", &opf_with(dates, "", ""), &[]);
    Epub::open(path).unwrap().metadata().published
}

#[test]
fn reads_the_publication_date() {
    assert_eq!(
        published("<dc:date>1997-02-15T05:00:00+00:00</dc:date>").as_deref(),
        Some("1997-02-15T05:00:00+00:00")
    );
    assert_eq!(
        published(
            r#"<dc:date opf:event="modification">2020-01-01</dc:date>
               <dc:date opf:event="publication">1965</dc:date>"#
        )
        .as_deref(),
        Some("1965")
    );
}

#[test]
fn ignores_dates_that_are_not_publication_dates() {
    assert_eq!(published(""), None);
    assert_eq!(
        published(r#"<dc:date opf:event="creation">2020-01-01</dc:date>"#),
        None
    );
    // calibre's "undefined".
    assert_eq!(
        published("<dc:date>0101-01-01T00:00:00+00:00</dc:date>"),
        None
    );
    assert_eq!(published("<dc:date>Spring 1999</dc:date>"), None);
}

const CHAPTER: &str = r#"<item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>"#;
const CHAPTER_REF: &str = r#"<itemref idref="c1"/>"#;

fn prose(words: usize) -> Vec<u8> {
    format!("<html><body><p>{}</p></body></html>", "word ".repeat(words)).into_bytes()
}

#[test]
fn estimates_pages_from_the_spines_words() {
    let dir = tempfile::tempdir().unwrap();
    let not_in_spine = r#"<item id="x" href="x.xhtml" media-type="application/xhtml+xml"/>"#;
    let opf = opf_with("", &format!("{CHAPTER}{not_in_spine}"), CHAPTER_REF);
    let chapter = prose(3000);
    let unread = prose(9000);
    let files: &[(&str, &[u8])] = &[("c1.xhtml", &chapter), ("x.xhtml", &unread)];
    let path = make_epub(dir.path(), "b.epub", &opf, files);
    let extent = Epub::open(path).unwrap().extent().unwrap();
    assert_eq!(extent.words, 3000);
    assert_eq!(extent.pages, Some(10));
    assert!(extent.pages_estimated);
}

#[test]
fn prefers_the_print_page_list() {
    let dir = tempfile::tempdir().unwrap();
    let ncx = r#"<item id="n" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#;
    let opf = opf_with("", &format!("{CHAPTER}{ncx}"), CHAPTER_REF);
    let list = format!(
        "<ncx><pageList>{}</pageList></ncx>",
        "<pageTarget/>".repeat(12)
    );
    let chapter = prose(3000);
    let files: &[(&str, &[u8])] = &[("c1.xhtml", &chapter), ("toc.ncx", list.as_bytes())];
    let path = make_epub(dir.path(), "b.epub", &opf, files);
    let extent = Epub::open(path).unwrap().extent().unwrap();
    assert_eq!(extent.pages, Some(12));
    assert!(!extent.pages_estimated);
}

#[test]
fn a_stub_page_list_is_not_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let ncx = r#"<item id="n" href="toc.ncx" media-type="application/x-dtbncx+xml"/>"#;
    let opf = opf_with("", &format!("{CHAPTER}{ncx}"), CHAPTER_REF);
    let list = "<ncx><pageList><pageTarget/></pageList></ncx>";
    let chapter = prose(3000);
    let files: &[(&str, &[u8])] = &[("c1.xhtml", &chapter), ("toc.ncx", list.as_bytes())];
    let path = make_epub(dir.path(), "b.epub", &opf, files);
    let extent = Epub::open(path).unwrap().extent().unwrap();
    assert_eq!(extent.pages, Some(10));
    assert!(extent.pages_estimated);
}

#[test]
fn picture_pages_count_towards_the_estimate() {
    let dir = tempfile::tempdir().unwrap();
    let manifest = r#"<item id="a" href="a.xhtml" media-type="application/xhtml+xml"/>
                      <item id="b" href="b.xhtml" media-type="application/xhtml+xml"/>"#;
    let opf = opf_with("", manifest, r#"<itemref idref="a"/><itemref idref="b"/>"#);
    let page: &[u8] = b"<html><body><img src='p.jpg'/></body></html>";
    let files: &[(&str, &[u8])] = &[("a.xhtml", page), ("b.xhtml", page)];
    let path = make_epub(dir.path(), "b.epub", &opf, files);
    let extent = Epub::open(path).unwrap().extent().unwrap();
    assert_eq!(extent.words, 0);
    assert_eq!(extent.pages, Some(2));
}

#[test]
fn an_empty_book_has_no_page_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_epub(dir.path(), "b.epub", &opf_with("", "", ""), &[]);
    let extent = Epub::open(path).unwrap().extent().unwrap();
    assert_eq!(extent.pages, None);
}

#[test]
fn encrypted_books_cannot_be_measured() {
    let dir = tempfile::tempdir().unwrap();
    let opf = opf_with("", CHAPTER, CHAPTER_REF);
    let path = make_epub(dir.path(), "b.epub", &opf, &[("c1.xhtml", &prose(10))]);
    let mut zip =
        ZipWriter::new_append(File::options().read(true).write(true).open(&path).unwrap()).unwrap();
    zip.start_file("META-INF/rights.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"<rights/>").unwrap();
    zip.finish().unwrap();
    assert!(Epub::open(path).unwrap().extent().is_err());
}
