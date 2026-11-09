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
    zip.start_file("ComicInfo.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"<ComicInfo><Pages><Page Image="2" Type="FrontCover"/></Pages></ComicInfo>"#)
        .unwrap();
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

#[test]
fn greyscale_kepubs_keep_the_original_or_custom_colour_cover() {
    let dir = tempfile::tempdir().unwrap();
    let cbz = dir.path().join("test.cbz");
    sample(&cbz);
    let original = entry(
        &mut ZipArchive::new(File::open(&cbz).unwrap()).unwrap(),
        "3.png",
    );
    let mut custom = Cursor::new(Vec::new());
    RgbImage::from_pixel(29, 42, Rgb([20, 190, 90]))
        .write_to(&mut custom, ImageFormat::Jpeg)
        .unwrap();

    let mut profile = profile();
    profile.colour = false;
    for (custom_cover, extension, expected) in [
        (None, "png", original.as_slice()),
        (
            Some(custom.get_ref().as_slice()),
            "jpg",
            custom.get_ref().as_slice(),
        ),
    ] {
        let prepared = prepare(
            &cbz,
            &Metadata {
                custom_cover,
                ..metadata()
            },
            profile,
            Options {
                crop: true,
                autocontrast: true,
                ..Options::default()
            },
            &Kepubify::locate().unwrap(),
            |_, _| {},
        )
        .unwrap();
        let mut zip = ZipArchive::new(File::open(prepared.path()).unwrap()).unwrap();
        assert_eq!(
            entry(&mut zip, &format!("OEBPS/images/cover.{extension}")),
            expected
        );
        let cover_page = entry(&mut zip, "OEBPS/images/p00004.png");
        assert_eq!(
            &cover_page[24..26],
            &[4, 0],
            "reading page is still 4-bit greyscale"
        );
    }
}

fn png(image: &RgbImage) -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image.write_to(&mut bytes, ImageFormat::Png).unwrap();
    bytes.into_inner()
}

#[test]
fn damaged_pages_are_left_out_and_named() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("damaged.cbz");
    let good = png(&RgbImage::from_pixel(9, 20, Rgb([140, 140, 140])));
    let noise = RgbImage::from_fn(90, 200, |x, y| {
        Rgb([u8::try_from((x * 31 + y * 17) % 256).unwrap(); 3])
    });
    let mut truncated = png(&noise);
    truncated.truncate(truncated.len() / 2); // The header reads; the pixels don't.
    let mut zip = ZipWriter::new(File::create(&source).unwrap());
    for (name, bytes) in [
        ("1.png", good.as_slice()),
        ("2.png", b"not an image".as_slice()),
        ("3.png", truncated.as_slice()),
        ("4.png", good.as_slice()),
    ] {
        zip.start_file(name, SimpleFileOptions::default()).unwrap();
        zip.write_all(bytes).unwrap();
    }
    // The cover is the damaged second page, so the next readable page is used.
    zip.start_file("ComicInfo.xml", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(br#"<ComicInfo><Pages><Page Image="1" Type="FrontCover"/></Pages></ComicInfo>"#)
        .unwrap();
    zip.finish().unwrap();

    let mut seen = Vec::new();
    let prepared = prepare(
        &source,
        &metadata(),
        profile(),
        Options::default(),
        &Kepubify::locate().unwrap(),
        |page, pages| seen.push((page, pages)),
    )
    .unwrap();
    assert_eq!(prepared.pages, 2);
    assert_eq!(prepared.skipped, ["2.png", "3.png"]);
    // Each page is counted once, damaged or not.
    assert_eq!(seen.last(), Some(&(4, 4)));
    assert!(seen.windows(2).all(|w| w[0].0 < w[1].0));
    assert!(
        seen.contains(&(3, 4)),
        "the page that failed to decode is counted"
    );
    let mut out = ZipArchive::new(File::open(prepared.path()).unwrap()).unwrap();
    assert_eq!(entry(&mut out, "OEBPS/images/cover.png"), good);
}

#[test]
fn a_comic_with_no_readable_pages_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("broken.cbz");
    let mut zip = ZipWriter::new(File::create(&source).unwrap());
    zip.start_file("1.png", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"not an image").unwrap();
    zip.finish().unwrap();
    let result = prepare(
        &source,
        &metadata(),
        profile(),
        Options::default(),
        &Kepubify::locate().unwrap(),
        |_, _| {},
    );
    assert!(matches!(
        result,
        Err(erc_comic::prepare::Error::NoReadablePages)
    ));
}

#[test]
fn pages_keep_their_order_when_processed_in_parallel() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("many.cbz");
    let mut zip = ZipWriter::new(File::create(&source).unwrap());
    // Uneven sizes make pages finish out of order on the pool.
    let widths: Vec<u32> = (0..60)
        .map(|i| if i % 7 == 0 { 600 } else { 20 + i })
        .collect();
    for (i, &width) in widths.iter().enumerate() {
        zip.start_file(format!("{i:03}.png"), SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&png(&RgbImage::from_pixel(width, 800, Rgb([90, 90, 90]))))
            .unwrap();
    }
    zip.finish().unwrap();
    let mut seen = Vec::new();
    let prepared = prepare(
        &source,
        &metadata(),
        Profile {
            width: 1000,
            height: 1000,
            colour: false,
            landscape_spreads: false,
        },
        Options::default(),
        &Kepubify::locate().unwrap(),
        |page, _| seen.push(page),
    )
    .unwrap();
    assert_eq!(seen, (1..=60).collect::<Vec<_>>());
    let mut out = ZipArchive::new(File::open(prepared.path()).unwrap()).unwrap();
    for (i, &width) in widths.iter().enumerate() {
        let bytes = entry(&mut out, &format!("OEBPS/images/p{:05}.png", i + 1));
        assert_eq!(image::load_from_memory(&bytes).unwrap().width(), width);
    }
}
