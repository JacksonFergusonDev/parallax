use std::fs::File;
use std::io::{Cursor, Read, Write};
use std::path::Path;

use erc_comic::prepare::{Metadata, Options, Profile, Spreads, prepare, safe_kobo_component};
use erc_convert::Kepubify;
use image::{ImageFormat, Rgb, RgbImage};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn sample(path: &Path) {
    let mut zip = ZipWriter::new(File::create(path).unwrap());
    for (name, image) in [
        ("1.png", RgbImage::from_pixel(9, 20, Rgb([140, 140, 140]))),
        (
            "2.png",
            RgbImage::from_fn(31, 20, |x, _| {
                let gray = if x < 15 { 0 } else { 255 };
                Rgb([gray; 3])
            }),
        ),
        ("3.png", RgbImage::from_pixel(36, 20, Rgb([255, 0, 0]))),
    ] {
        let mut bytes = Cursor::new(Vec::new());
        image.write_to(&mut bytes, ImageFormat::Png).unwrap();
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes.get_ref()).unwrap();
    }
    zip.finish().unwrap();
}

fn metadata() -> Metadata<'static> {
    Metadata {
        identifier: "urn:test:comic",
        title: "Title & subtitle",
        authors: &[],
        credits: &[],
        series: Some("Series"),
        series_index: Some("1"),
        language: None,
        modified: "2026-11-08T00:00:00Z",
        custom_cover: None,
    }
}

fn profile() -> Profile {
    Profile {
        width: 24,
        height: 30,
        colour: true,
        landscape_spreads: true,
    }
}

fn entry(zip: &mut ZipArchive<File>, name: &str) -> Vec<u8> {
    let mut data = Vec::new();
    zip.by_name(name).unwrap().read_to_end(&mut data).unwrap();
    data
}

#[test]
fn kepubify_preserves_layout_direction_and_the_rtl_split() {
    let dir = tempfile::tempdir().unwrap();
    let cbz = dir.path().join("test.cbz");
    sample(&cbz);
    let mut progress = Vec::new();
    let prepared = prepare(
        &cbz,
        &metadata(),
        profile(),
        Options {
            rtl: true,
            ..Options::default()
        },
        &Kepubify::locate().unwrap(),
        |page, total| progress.push((page, total)),
    )
    .unwrap();
    assert_eq!(progress, [(1, 3), (2, 3), (3, 3)]);
    assert_eq!(prepared.pages, 4);
    let path = prepared.path().to_owned();
    let mut zip = ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let opf = String::from_utf8(entry(&mut zip, "OEBPS/content.opf")).unwrap();
    assert!(opf.contains("pre-paginated"));
    assert!(opf.contains("page-progression-direction=\"rtl\""));
    assert!(opf.contains("Title &amp; subtitle"));
    assert!(opf.contains("belongs-to-collection"));
    let doc = roxmltree::Document::parse(&opf).unwrap();
    let refs: Vec<_> = doc
        .descendants()
        .filter(|node| node.has_tag_name("itemref"))
        .collect();
    assert_eq!(refs.len(), 4);
    assert_eq!(
        refs[1].attribute("properties"),
        Some("rendition:page-spread-right")
    );
    assert_eq!(
        refs[2].attribute("properties"),
        Some("rendition:page-spread-left")
    );
    let gray = |zip: &mut ZipArchive<File>, n| {
        let bytes = entry(zip, &format!("OEBPS/images/p{n:05}.png"));
        assert_eq!(bytes[24], 4, "bit depth");
        image::load_from_memory(&bytes).unwrap().into_luma8()
    };
    assert_eq!(gray(&mut zip, 1).dimensions(), (9, 20));
    assert_eq!(gray(&mut zip, 2).get_pixel(0, 0).0, [255]);
    assert_eq!(gray(&mut zip, 3).get_pixel(0, 0).0, [0]);
    let rotated = image::load_from_memory(&entry(&mut zip, "OEBPS/images/p00004.jpg")).unwrap();
    assert_eq!((rotated.width(), rotated.height()), (17, 30));
    drop(zip);
    drop(prepared);
    assert!(!path.exists());
}

#[test]
fn whole_modes_emit_an_unrotated_spread_before_any_halves() {
    let dir = tempfile::tempdir().unwrap();
    let cbz = dir.path().join("test.cbz");
    sample(&cbz);
    for (spreads, pages) in [(Spreads::KeepWhole, 3), (Spreads::WholeThenHalves, 5)] {
        let prepared = prepare(
            &cbz,
            &metadata(),
            profile(),
            Options {
                spreads,
                ..Options::default()
            },
            &Kepubify::locate().unwrap(),
            |_, _| {},
        )
        .unwrap();
        assert_eq!(prepared.pages, pages);
        let mut zip = ZipArchive::new(File::open(prepared.path()).unwrap()).unwrap();
        let spread = image::load_from_memory(&entry(&mut zip, "OEBPS/images/p00002.png")).unwrap();
        assert_eq!((spread.width(), spread.height()), (24, 15));
    }
    assert_eq!(safe_kobo_component("S! <V>/ $3"), "S_ _V__ _3");
}
