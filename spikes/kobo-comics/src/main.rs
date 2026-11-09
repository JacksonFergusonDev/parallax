//! Kobo comics spike: makes four versions of each CBZ so they can be compared
//! on a Kobo Libra 2.
//!
//! | Variant | Container | Pages                                       |
//! | ------- | --------- | ------------------------------------------- |
//! | A       | CBZ       | the original file, byte for byte            |
//! | B       | KEPUB     | the original image bytes, repackaged        |
//! | C       | CBZ       | processed for the Libra 2, plus ComicInfo   |
//! | D       | KEPUB     | processed for the Libra 2                   |
//!
//! Processing: greyscale, double-page spreads split right half first (manga
//! order), Lanczos3 downscale to fit the screen, then 16 grey levels saved as
//! 4-bit PNG (lossless at the panel's own depth).
//!
//! KEPUBs are written as fixed-layout EPUB 3 and then run through the same
//! kepubify the app bundles, so the result is what the app would really send.
//!
//! Usage: kobo-comics-spike --out DIR --author NAME FILE.cbz...

use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

use anyhow::{Context, Result, bail, ensure};
use image::imageops::FilterType;
use image::{GrayImage, ImageReader};
use rayon::prelude::*;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Kobo Libra 2 screen, portrait.
const SCREEN: (u32, u32) = (1264, 1680);
/// Grey levels the Libra 2's panel shows.
const LEVELS: u16 = 16;

struct Volume {
    series: String,
    number: u32,
    year: Option<String>,
}

/// One page ready to package.
struct Page {
    /// File name inside the package, e.g. `p0001.jpg`.
    name: String,
    bytes: Vec<u8>,
    width: u32,
    height: u32,
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let mut out = None;
    let mut author = None;
    let mut inputs = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--out" => out = args.next().map(PathBuf::from),
            "--author" => author = args.next(),
            _ => inputs.push(PathBuf::from(arg)),
        }
    }
    let (Some(out), Some(author)) = (out, author) else {
        bail!("usage: kobo-comics-spike --out DIR --author NAME FILE.cbz...");
    };
    ensure!(!inputs.is_empty(), "no input files");
    fs::create_dir_all(&out)?;
    let kepubify = kepubify()?;

    let mut report = String::from(
        "| Volume | Variant | Pages | Size (MB) | Time (s) |\n| --- | --- | --- | --- | --- |\n",
    );
    for input in &inputs {
        let volume = parse_name(input)?;
        let label = format!("{} v{:02}", volume.series, volume.number);
        println!("{label}");
        let source = fs::metadata(input)?.len();

        // A: the original, renamed so the Kobo's filename-based title shows the variant.
        let started = Instant::now();
        let a = out.join(format!("{label} [A original].cbz"));
        fs::copy(input, &a)?;
        row(&mut report, &label, "A original CBZ", None, &a, started)?;

        let started = Instant::now();
        let originals = original_pages(input)?;
        let read_secs = started.elapsed().as_secs_f64();

        // B: original image bytes in a fixed-layout KEPUB.
        let started = Instant::now();
        let b = out.join(format!("{label} [B lossless].kepub.epub"));
        let epub = out.join(format!("{label} [B lossless].epub"));
        write_epub(&epub, &volume, &author, "B", &originals, false)?;
        run_kepubify(&kepubify, &epub, &b)?;
        fs::remove_file(&epub)?;
        verify_lossless(&b, &originals)?;
        row(
            &mut report,
            &label,
            "B lossless KEPUB",
            Some(originals.len()),
            &b,
            started,
        )?;

        // C and D share one processing pass.
        let started = Instant::now();
        let processed = process(&originals)?;
        let process_secs = started.elapsed().as_secs_f64();
        println!(
            "  processed {} pages into {} in {process_secs:.1}s (reading took {read_secs:.1}s)",
            originals.len(),
            processed.len()
        );

        let started = Instant::now();
        let c = out.join(format!("{label} [C processed].cbz"));
        write_cbz(&c, &volume, &author, &processed)?;
        row(
            &mut report,
            &label,
            "C processed CBZ (+ComicInfo)",
            Some(processed.len()),
            &c,
            started,
        )?;
        append_time(&mut report, process_secs);

        let started = Instant::now();
        let d = out.join(format!("{label} [D processed].kepub.epub"));
        let epub = out.join(format!("{label} [D processed].epub"));
        write_epub(&epub, &volume, &author, "D", &processed, true)?;
        run_kepubify(&kepubify, &epub, &d)?;
        fs::remove_file(&epub)?;
        verify_lossless(&d, &processed)?;
        row(
            &mut report,
            &label,
            "D processed KEPUB",
            Some(processed.len()),
            &d,
            started,
        )?;
        append_time(&mut report, process_secs);

        println!("  source {:.0} MB", mb(source));
    }
    fs::write(out.join("report.md"), &report)?;
    print!("{report}");
    Ok(())
}

fn mb(bytes: u64) -> f64 {
    bytes as f64 / 1_000_000.0
}

fn row(
    report: &mut String,
    label: &str,
    variant: &str,
    pages: Option<usize>,
    path: &Path,
    started: Instant,
) -> Result<()> {
    let size = fs::metadata(path)?.len();
    let pages = pages.map_or_else(|| "same".to_owned(), |n| n.to_string());
    let secs = started.elapsed().as_secs_f64();
    println!("  {variant}: {:.0} MB in {secs:.1}s", mb(size));
    report.push_str(&format!(
        "| {label} | {variant} | {pages} | {:.0} | {secs:.1} |\n",
        mb(size)
    ));
    Ok(())
}

/// Notes the shared processing time on the row just written.
fn append_time(report: &mut String, process_secs: f64) {
    report.truncate(report.len() - " |\n".len());
    report.push_str(&format!(" + {process_secs:.1} processing |\n"));
}

/// "Berserk v01 (2003) (Digital) (danke-Empire).cbz" → Berserk, 1, 2003.
fn parse_name(path: &Path) -> Result<Volume> {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .context("file name is not UTF-8")?;
    let (series, rest) = stem
        .split_once(" v")
        .with_context(|| format!("no volume number in {stem:?}"))?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let number = digits
        .parse()
        .with_context(|| format!("no volume number in {stem:?}"))?;
    let year = rest
        .split(['(', ')'])
        .find(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_owned);
    Ok(Volume {
        series: series.trim().to_owned(),
        number,
        year,
    })
}

fn is_image(name: &str) -> Option<&'static str> {
    let lower = name.to_ascii_lowercase();
    [("jpg", "jpg"), ("jpeg", "jpg"), ("png", "png")]
        .into_iter()
        .find(|(ext, _)| lower.ends_with(&format!(".{ext}")))
        .map(|(_, ext)| ext)
}

/// The CBZ's images in reading order, bytes untouched. These volumes zero-pad
/// their page numbers, so a plain sort is enough here; the app needs a
/// natural sort.
fn original_pages(cbz: &Path) -> Result<Vec<Page>> {
    let mut zip = ZipArchive::new(File::open(cbz)?)?;
    let mut names: Vec<String> = zip
        .file_names()
        .filter_map(Result::ok)
        .map(std::borrow::Cow::into_owned)
        .filter(|n| is_image(n).is_some() && !n.starts_with("__MACOSX"))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    let mut pages = Vec::with_capacity(names.len());
    for (i, name) in names.iter().enumerate() {
        let mut bytes = Vec::new();
        zip.by_name(name)?.read_to_end(&mut bytes)?;
        let (width, height) = ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_dimensions()
            .with_context(|| format!("reading the size of {name}"))?;
        let ext = is_image(name).unwrap_or("jpg");
        pages.push(Page {
            name: format!("p{:04}.{ext}", i + 1),
            bytes,
            width,
            height,
        });
    }
    Ok(pages)
}

/// Greyscale, split spreads right half first, fit the screen, 4-bit PNG.
fn process(pages: &[Page]) -> Result<Vec<Page>> {
    let parts: Vec<Vec<GrayImage>> = pages
        .par_iter()
        .map(|page| -> Result<Vec<GrayImage>> {
            let gray = image::load_from_memory(&page.bytes)
                .with_context(|| format!("decoding {}", page.name))?
                .to_luma8();
            let (w, h) = gray.dimensions();
            let halves = if w > h {
                // Manga reads right to left, so the right half comes first.
                let half = w / 2;
                vec![
                    image::imageops::crop_imm(&gray, half, 0, w - half, h).to_image(),
                    image::imageops::crop_imm(&gray, 0, 0, half, h).to_image(),
                ]
            } else {
                vec![gray]
            };
            Ok(halves.into_iter().map(fit).collect())
        })
        .collect::<Result<_>>()?;
    parts
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .into_par_iter()
        .enumerate()
        .map(|(i, img)| {
            Ok(Page {
                name: format!("p{:04}.png", i + 1),
                width: img.width(),
                height: img.height(),
                bytes: png_4bit(&img)?,
            })
        })
        .collect()
}

/// Downscales to fit the screen, keeping the aspect ratio. Never upscales.
fn fit(img: GrayImage) -> GrayImage {
    let (w, h) = img.dimensions();
    let scale = (f64::from(SCREEN.0) / f64::from(w)).min(f64::from(SCREEN.1) / f64::from(h));
    if scale >= 1.0 {
        return img;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (nw, nh) = (
        (f64::from(w) * scale).round().max(1.0) as u32,
        (f64::from(h) * scale).round().max(1.0) as u32,
    );
    image::imageops::resize(&img, nw, nh, FilterType::Lanczos3)
}

/// 16 grey levels, two pixels per byte. A 4-bit grey value n displays as
/// n * 17, so the levels land exactly on 0, 17, … 255.
fn png_4bit(img: &GrayImage) -> Result<Vec<u8>> {
    let (w, h) = img.dimensions();
    let row_bytes = w.div_ceil(2) as usize;
    let mut packed = vec![0u8; row_bytes * h as usize];
    for (y, row) in img.rows().enumerate() {
        for (x, px) in row.enumerate() {
            #[allow(clippy::cast_possible_truncation)]
            let level = ((u16::from(px.0[0]) * (LEVELS - 1) + 127) / 255) as u8;
            let byte = &mut packed[y * row_bytes + x / 2];
            *byte |= if x % 2 == 0 { level << 4 } else { level };
        }
    }
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, w, h);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Four);
    encoder.set_compression(png::Compression::High);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&packed)?;
    writer.finish()?;
    Ok(out)
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn media_type(name: &str) -> &'static str {
    if name.ends_with(".png") {
        "image/png"
    } else {
        "image/jpeg"
    }
}

fn write_cbz(path: &Path, volume: &Volume, author: &str, pages: &[Page]) -> Result<()> {
    let mut zip = ZipWriter::new(File::create(path)?);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    for page in pages {
        zip.start_file(&page.name, stored)?;
        zip.write_all(&page.bytes)?;
    }
    // Tests whether the Kobo reads ComicInfo at all.
    let year = volume
        .year
        .as_deref()
        .map(|y| format!("  <Year>{y}</Year>\n"))
        .unwrap_or_default();
    let info = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n  <Title>{series} v{n:02} [C processed]</Title>\n  <Series>{series} C</Series>\n  <Number>{n}</Number>\n  <Volume>{n}</Volume>\n{year}  <Writer>{author}</Writer>\n  <PageCount>{count}</PageCount>\n  <LanguageISO>en</LanguageISO>\n  <Manga>YesAndRightToLeft</Manga>\n</ComicInfo>\n",
        series = xml_escape(&volume.series),
        n = volume.number,
        author = xml_escape(author),
        count = pages.len(),
    );
    zip.start_file("ComicInfo.xml", SimpleFileOptions::default())?;
    zip.write_all(info.as_bytes())?;
    zip.finish()?;
    Ok(())
}

/// A fixed-layout, right-to-left EPUB 3 with one page per image.
fn write_epub(
    path: &Path,
    volume: &Volume,
    author: &str,
    variant: &str,
    pages: &[Page],
    sized_for_screen: bool,
) -> Result<()> {
    let series = format!("{} {variant}", volume.series);
    let title = format!(
        "{} v{:02} [{variant} {}]",
        volume.series,
        volume.number,
        if variant == "B" {
            "lossless"
        } else {
            "processed"
        }
    );
    let id = format!(
        "urn:parallax-spike:{}-v{:02}-{variant}",
        volume.series.to_lowercase().replace(' ', "-"),
        volume.number
    );

    let mut zip = ZipWriter::new(File::create(path)?);
    let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    let deflated = SimpleFileOptions::default();
    zip.start_file("mimetype", stored)?;
    zip.write_all(b"application/epub+zip")?;
    zip.start_file("META-INF/container.xml", deflated)?;
    zip.write_all(br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>
"#)?;

    let mut manifest = String::new();
    let mut spine = String::new();
    for (i, page) in pages.iter().enumerate() {
        let n = i + 1;
        let cover = if i == 0 {
            " properties=\"cover-image\""
        } else {
            ""
        };
        let image_id = if i == 0 {
            "cover".to_owned()
        } else {
            format!("img{n}")
        };
        manifest.push_str(&format!(
            "    <item id=\"{image_id}\" href=\"images/{}\" media-type=\"{}\"{cover}/>\n    <item id=\"page{n}\" href=\"pages/page{n:04}.xhtml\" media-type=\"application/xhtml+xml\"/>\n",
            page.name,
            media_type(&page.name),
        ));
        spine.push_str(&format!("    <itemref idref=\"page{n}\"/>\n"));

        let (w, h) = (page.width, page.height);
        let xhtml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <title>Page {n}</title>
  <meta name="viewport" content="width={w}, height={h}"/>
  <style>html, body {{ margin: 0; padding: 0; width: {w}px; height: {h}px; }} img {{ display: block; width: {w}px; height: {h}px; }}</style>
</head>
<body><img src="../images/{name}" alt=""/></body>
</html>
"#,
            name = page.name,
        );
        zip.start_file(format!("OEBPS/pages/page{n:04}.xhtml"), deflated)?;
        zip.write_all(xhtml.as_bytes())?;
        zip.start_file(format!("OEBPS/images/{}", page.name), stored)?;
        zip.write_all(&page.bytes)?;
    }

    let resolution = if sized_for_screen {
        format!(
            "    <meta name=\"original-resolution\" content=\"{}x{}\"/>\n",
            SCREEN.0, SCREEN.1
        )
    } else {
        String::new()
    };
    let date = volume
        .year
        .as_deref()
        .map(|y| format!("    <dc:date>{y}</dc:date>\n"))
        .unwrap_or_default();
    let opf = format!(
        r##"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" prefix="rendition: http://www.idpf.org/vocab/rendition/#">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf">
    <dc:identifier id="uid">{id}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:creator id="creator">{author}</dc:creator>
    <meta refines="#creator" property="role" scheme="marc:relators">aut</meta>
    <dc:language>en</dc:language>
{date}    <meta property="dcterms:modified">2026-11-08T00:00:00Z</meta>
    <meta property="rendition:layout">pre-paginated</meta>
    <meta property="rendition:orientation">portrait</meta>
    <meta property="rendition:spread">none</meta>
    <meta name="cover" content="cover"/>
    <meta name="calibre:series" content="{series}"/>
    <meta name="calibre:series_index" content="{n}"/>
    <meta property="belongs-to-collection" id="series">{series}</meta>
    <meta refines="#series" property="collection-type">series</meta>
    <meta refines="#series" property="group-position">{n}</meta>
{resolution}  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
{manifest}  </manifest>
  <spine toc="ncx" page-progression-direction="rtl">
{spine}  </spine>
</package>
"##,
        title = xml_escape(&title),
        author = xml_escape(author),
        series = xml_escape(&series),
        n = volume.number,
    );
    zip.start_file("OEBPS/content.opf", deflated)?;
    zip.write_all(opf.as_bytes())?;

    zip.start_file("OEBPS/nav.xhtml", deflated)?;
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>{title}</title></head>
<body><nav epub:type="toc"><ol><li><a href="pages/page0001.xhtml">{title}</a></li></ol></nav></body>
</html>
"#,
            title = xml_escape(&title)
        )
        .as_bytes(),
    )?;
    zip.start_file("OEBPS/toc.ncx", deflated)?;
    zip.write_all(
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head><meta name="dtb:uid" content="{id}"/></head>
  <docTitle><text>{title}</text></docTitle>
  <navMap><navPoint id="p1" playOrder="1"><navLabel><text>{title}</text></navLabel><content src="pages/page0001.xhtml"/></navPoint></navMap>
</ncx>
"#,
            title = xml_escape(&title)
        )
        .as_bytes(),
    )?;
    zip.finish()?;
    Ok(())
}

fn kepubify() -> Result<PathBuf> {
    if let Ok(path) = std::env::var("PARALLAX_KEPUBIFY") {
        return Ok(PathBuf::from(path));
    }
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../app/src-tauri/binaries/kepubify-aarch64-apple-darwin");
    ensure!(
        bundled.is_file(),
        "kepubify not found at {}; run `just setup`",
        bundled.display()
    );
    Ok(bundled)
}

/// The same flags the app's converter uses, plus no dummy title page, which
/// would add a reflowable page to a fixed-layout book.
fn run_kepubify(binary: &Path, input: &Path, output: &Path) -> Result<()> {
    let out = Command::new(binary)
        .arg("--no-add-dummy-titlepage")
        .arg("--output")
        .arg(output)
        .arg(input)
        .output()?;
    ensure!(
        out.status.success() && output.is_file(),
        "kepubify failed: {}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

/// Every image in the KEPUB must be byte-identical to the page it came from,
/// and kepubify must have kept the fixed-layout and RTL settings.
fn verify_lossless(kepub: &Path, pages: &[Page]) -> Result<()> {
    let mut zip = ZipArchive::new(File::open(kepub)?)?;
    for page in pages {
        let mut bytes = Vec::new();
        zip.by_name(&format!("OEBPS/images/{}", page.name))
            .with_context(|| format!("{} missing from the KEPUB", page.name))?
            .read_to_end(&mut bytes)?;
        ensure!(
            Sha256::digest(&bytes) == Sha256::digest(&page.bytes),
            "{} changed inside the KEPUB",
            page.name
        );
    }
    let mut opf = String::new();
    zip.by_name("OEBPS/content.opf")?.read_to_string(&mut opf)?;
    for needle in [
        "pre-paginated",
        "page-progression-direction=\"rtl\"",
        "calibre:series",
    ] {
        ensure!(opf.contains(needle), "kepubify dropped {needle}");
    }
    Ok(())
}
