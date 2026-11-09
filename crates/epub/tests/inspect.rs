use std::fs::File;
use std::io::Write;
use std::path::Path;

use parallax_epub::{Epub, MetadataEdit, inspect};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>T</dc:title><dc:identifier id="uid">x</dc:identifier></metadata>
  <manifest>
    <item id="c1" href="Text/c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="Text/c2.xhtml" media-type="application/xhtml+xml"/>
    <item id="img" href="Images/pic.jpg" media-type="image/jpeg"/>
    <item id="font" href="Fonts/Serif.otf" media-type="application/vnd.ms-opentype"/>
  </manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;

const C1: &str = r##"<?xml version="1.0"?>
<html xmlns="http://www.w3.org/1999/xhtml"><body>
<a href="c2.xhtml#n1">fine</a> <a href="#top">fine</a> <a href="https://example.com">fine</a>
<img src="../Images/pic.jpg"/> <img src="../Images/missing%20one.png"/>
<a href="c3.xhtml">broken</a>
</body></html>"##;

fn build(path: &Path) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    for (name, data) in [
        ("META-INF/container.xml", r#"<?xml version="1.0"?><container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container"><rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#.as_bytes()),
        ("OEBPS/content.opf", OPF.as_bytes()),
        ("OEBPS/Text/c1.xhtml", C1.as_bytes()),
        ("OEBPS/Images/pic.jpg", b"jpg"),
        ("OEBPS/Fonts/Serif.otf", b"otf"),
        ("OEBPS/Text/leftover.xhtml", b"<html/>"),
        ("OEBPS/.DS_Store", b"x"),
        ("__MACOSX/OEBPS/._c1.xhtml", b"x"),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(data).unwrap();
    }
    zip.finish().unwrap();
}

#[test]
fn finds_structural_problems() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.epub");
    build(&path);
    let found = inspect(&path).unwrap();
    assert_eq!(found.missing_files, ["OEBPS/Text/c2.xhtml"]);
    assert_eq!(found.unmanifested, ["OEBPS/Text/leftover.xhtml"]);
    assert_eq!(
        found.clutter,
        ["OEBPS/.DS_Store", "__MACOSX/OEBPS/._c1.xhtml"]
    );
    assert_eq!(found.fonts, ["OEBPS/Fonts/Serif.otf"]);
    assert!(!found.drm);
    // c2 is missing too, so its link counts; fragments, anchors and web
    // links don't.
    let targets: Vec<&str> = found.broken_links.iter().map(|(_, t)| t.as_str()).collect();
    assert_eq!(
        targets,
        ["c2.xhtml#n1", "../Images/missing%20one.png", "c3.xhtml"]
    );
    assert!(!found.is_clean());
}

#[test]
fn copies_leave_clutter_out() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.epub");
    build(&path);
    let out = dir.path().join("out.epub");
    Epub::open(&path)
        .unwrap()
        .write_with_metadata(
            &MetadataEdit {
                title: "T".into(),
                ..Default::default()
            },
            &out,
        )
        .unwrap();
    let found = inspect(&out).unwrap();
    assert!(found.clutter.is_empty());
    assert_eq!(found.unmanifested, ["OEBPS/Text/leftover.xhtml"]);
}
