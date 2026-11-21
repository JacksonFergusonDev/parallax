//! PDF comics to CBZ. Most comic PDFs are scans with one JPEG per page,
//! which are copied out unchanged. Other pages (vector art, text, several
//! images, masks or unusual colour) are drawn with hayro, a pure-Rust PDF
//! renderer, and saved as JPEG. Like CBR and CB7, the result is a CBZ, so
//! the reader and device sending work on it as on any other comic.

use std::borrow::Cow;
use std::fs;
use std::io::{BufWriter, Cursor, Read, Write};
use std::path::Path;
use std::sync::Arc;

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_syntax::Pdf;
use hayro::hayro_syntax::content::ops::TypedInstruction;
use hayro::hayro_syntax::page::Page;
use hayro::vello_cpu::color::palette::css::WHITE;
use hayro::{PixmapSettings, RenderCache, RenderSettings};
use rayon::prelude::*;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::comicinfo::ComicInfo;
use crate::convert::Converted;
use crate::entries::{MAX_ENTRIES, MAX_PAGE_BYTES, SkipReason, Skipped};
use crate::error::{Error, Result, io};

/// Longest side of a page drawn from vector content, in pixels: enough for
/// a Kobo Elipsa or Kindle Scribe screen.
const DRAWN_LONG_SIDE: f32 = 2560.0;
/// Pages are never drawn larger than this on either side.
const MAX_SIDE: f32 = 6000.0;
/// JPEG quality for drawn pages.
const JPEG_QUALITY: u8 = 90;
/// Pages drawn at once, bounding memory to a few hundred megabytes.
const BATCH: usize = 16;
/// How far a scan's shape may differ from its page's and still count as
/// the whole page.
const ASPECT_TOLERANCE: f64 = 0.02;

/// Whether a file is a PDF, from its first bytes.
pub fn is_pdf(path: &Path) -> bool {
    let mut head = [0u8; 1024];
    let Ok(mut file) = fs::File::open(path) else {
        return false;
    };
    let n = file.read(&mut head).unwrap_or(0);
    // The header may follow a little junk, which readers tolerate.
    head[..n].windows(5).any(|w| w == b"%PDF-")
}

/// One page of the output: its bytes and file extension.
struct Out {
    data: Vec<u8>,
    ext: &'static str,
}

/// Writes every page of the PDF at `src` into a new CBZ at `dest`, with a
/// ComicInfo.xml holding the PDF's title and author when it has them.
/// `dest` appears only once it is complete.
pub fn pdf_to_cbz(src: &Path, dest: &Path) -> Result<Converted> {
    let data: Arc<Vec<u8>> = Arc::new(fs::read(src).map_err(io(src))?);
    let pdf = open(src, &data)?;
    let count = pdf.pages().len();
    if count == 0 {
        return Err(Error::NoPages(src.to_path_buf()));
    }
    if count > MAX_ENTRIES {
        return Err(Error::TooManyEntries {
            path: src.to_path_buf(),
            limit: MAX_ENTRIES,
        });
    }
    let info = comic_info(&pdf);

    let dir = dest.parent().unwrap_or(Path::new("."));
    let tmp = tempfile::NamedTempFile::new_in(dir).map_err(io(dir))?;
    let mut zip = ZipWriter::new(BufWriter::new(tmp.as_file()));
    let stored = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Stored)
        .large_file(false);
    let zip_err = |e: zip::result::ZipError| Error::Damaged {
        path: dest.to_path_buf(),
        message: e.to_string(),
    };
    let mut result = Converted {
        pages: 0,
        comic_info: info.is_some(),
        skipped: Vec::new(),
    };
    let width = count.to_string().len().max(3);
    let indices: Vec<usize> = (0..count).collect();
    for batch in indices.chunks(BATCH) {
        // Each thread parses its own copy of the document: hayro's pages
        // borrow from the document and can't be shared between threads.
        let pages: Vec<(usize, Option<Out>)> = batch
            .par_iter()
            .map_init(
                || Pdf::new(Arc::clone(&data)).ok(),
                |pdf, &index| {
                    let out = pdf.as_ref().and_then(|pdf| {
                        let page = pdf.pages().get(index)?;
                        page_image(page)
                    });
                    (index, out)
                },
            )
            .collect();
        for (index, out) in pages {
            let name = format!("{:0width$}", index + 1);
            match out {
                Some(out) if out.data.len() as u64 <= MAX_PAGE_BYTES => {
                    zip.start_file(format!("{name}.{}", out.ext), stored)
                        .map_err(zip_err)?;
                    zip.write_all(&out.data).map_err(io(dest))?;
                    result.pages += 1;
                }
                _ => result.skipped.push(Skipped {
                    name: format!("page {}", index + 1),
                    reason: SkipReason::Unreadable,
                }),
            }
        }
    }
    if result.pages == 0 {
        return Err(Error::NoPages(src.to_path_buf()));
    }
    if let Some(info) = &info {
        zip.start_file("ComicInfo.xml", SimpleFileOptions::default())
            .map_err(zip_err)?;
        zip.write_all(info.to_xml().as_bytes()).map_err(io(dest))?;
    }
    let writer = zip.finish().map_err(zip_err)?;
    drop(writer);
    tmp.persist(dest).map_err(|e| Error::Io {
        path: dest.to_path_buf(),
        source: e.error,
    })?;
    Ok(result)
}

fn open(path: &Path, data: &Arc<Vec<u8>>) -> Result<Pdf> {
    use hayro::hayro_syntax::LoadPdfError;
    Pdf::new(Arc::clone(data)).map_err(|e| match e {
        LoadPdfError::Decryption(_) => Error::Encrypted(path.to_path_buf()),
        LoadPdfError::Invalid => Error::Damaged {
            path: path.to_path_buf(),
            message: "not a readable PDF".into(),
        },
    })
}

/// A page as an image file: the scan itself when the page is one JPEG
/// covering it, otherwise drawn.
fn page_image(page: &Page<'_>) -> Option<Out> {
    if let Some(jpeg) = whole_page_jpeg(page) {
        return Some(Out {
            data: jpeg,
            ext: "jpg",
        });
    }
    // A malformed page must not take the whole import down with it.
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draw(page)))
        .ok()
        .flatten()
        .map(|data| Out { data, ext: "jpg" })
}

/// The JPEG bytes of a page that draws exactly one image and nothing else,
/// when that image is a plain RGB or grey JPEG shaped like the page.
fn whole_page_jpeg(page: &Page<'_>) -> Option<Vec<u8>> {
    let mut image = None;
    let mut ops = page.typed_operations();
    while let Some(op) = ops.next() {
        match op {
            TypedInstruction::SaveState(_)
            | TypedInstruction::RestoreState(_)
            | TypedInstruction::Transform(_)
            | TypedInstruction::BeginMarkedContent(_)
            | TypedInstruction::BeginMarkedContentWithProperties(_)
            | TypedInstruction::EndMarkedContent(_)
            | TypedInstruction::MarkedContentPoint(_)
            | TypedInstruction::MarkedContentPointWithProperties(_)
            | TypedInstruction::RenderingIntent(_)
            | TypedInstruction::FlatnessTolerance(_) => {}
            TypedInstruction::XObject(x) if image.is_none() => {
                image = Some(page.resources().get_x_object(x.0)?);
            }
            _ => return None,
        }
    }
    let stream = image?;
    let dict = stream.dict();
    let name = |key: &[u8]| dict.get::<hayro::hayro_syntax::object::Name<'_>>(key);
    if name(b"Subtype").as_deref() != Some(b"Image".as_slice())
        || dict.contains_key(b"SMask")
        || dict.contains_key(b"Mask")
        || dict.contains_key(b"Decode")
        || dict.contains_key(b"ImageMask")
        // A lone JPEG filter, given as a name (an array is drawn instead).
        || name(b"Filter").as_deref() != Some(b"DCTDecode".as_slice())
    {
        return None;
    }
    let colour = name(b"ColorSpace")?;
    if !matches!(colour.as_ref(), b"DeviceRGB" | b"DeviceGray") {
        return None;
    }
    let (w, h) = (dict.get::<f64>(b"Width")?, dict.get::<f64>(b"Height")?);
    let (pw, ph) = page.render_dimensions();
    let (pw, ph) = (f64::from(pw), f64::from(ph));
    if w <= 0.0 || h <= 0.0 || pw <= 0.0 || ph <= 0.0 {
        return None;
    }
    // A rotated page shows the scan sideways; draw it instead.
    if ((w / h) / (pw / ph) - 1.0).abs() > ASPECT_TOLERANCE {
        return None;
    }
    let data: Cow<'_, [u8]> = stream.raw_data();
    data.starts_with(&[0xFF, 0xD8]).then(|| data.into_owned())
}

/// Draws a page on white at about the resolution of its largest image, or
/// at [`DRAWN_LONG_SIDE`] when it has none, and encodes it as JPEG.
fn draw(page: &Page<'_>) -> Option<Vec<u8>> {
    let (pw, ph) = page.render_dimensions();
    if pw <= 0.0 || ph <= 0.0 {
        return None;
    }
    let long = pw.max(ph);
    let scale =
        (largest_image_side(page).unwrap_or(DRAWN_LONG_SIDE) / long).clamp(1.0, MAX_SIDE / long);
    let cache = RenderCache::new();
    let pixmap = hayro::render(
        page,
        &cache,
        &InterpreterSettings::default(),
        &RenderSettings::default(),
        &PixmapSettings {
            x_scale: scale,
            y_scale: scale,
            bg_color: WHITE,
        },
    );
    let (w, h) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
    if w == 0 || h == 0 {
        return None;
    }
    // Drawn on opaque white, so premultiplied RGBA is plain RGBA.
    let rgb: Vec<u8> = pixmap
        .data_as_u8_slice()
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|&[r, g, b, _]| [r, g, b])
        .collect();
    let image = image::RgbImage::from_raw(w, h, rgb)?;
    let mut out = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, JPEG_QUALITY)
        .encode_image(&image)
        .ok()?;
    Some(out.into_inner())
}

/// The longer side, in pixels, of the largest image the page draws.
fn largest_image_side(page: &Page<'_>) -> Option<f32> {
    let mut largest: Option<f32> = None;
    let mut ops = page.typed_operations();
    while let Some(op) = ops.next() {
        if let TypedInstruction::XObject(x) = op
            && let Some(stream) = page.resources().get_x_object(x.0)
        {
            let d = stream.dict();
            if let (Some(w), Some(h)) = (d.get::<f32>(b"Width"), d.get::<f32>(b"Height")) {
                largest = Some(largest.unwrap_or(0.0).max(w.max(h)));
            }
        }
    }
    largest
}

/// The PDF's title and author as ComicInfo, when it has either.
fn comic_info(pdf: &Pdf) -> Option<ComicInfo> {
    let meta = pdf.metadata();
    let title = meta.title.as_deref().and_then(text);
    let author = meta.author.as_deref().and_then(text);
    if title.is_none() && author.is_none() {
        return None;
    }
    let mut info = ComicInfo::new();
    info.set("Title", title.as_deref());
    info.set("Writer", author.as_deref());
    Some(info)
}

/// A PDF text string: UTF-16 with a byte-order mark, UTF-8 with one, or
/// otherwise PDFDocEncoding, read as Latin-1 (they differ only in a few
/// punctuation marks).
fn text(bytes: &[u8]) -> Option<String> {
    let s = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| u16::from_be_bytes(pair))
            .collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        bytes.iter().map(|&b| char::from(b)).collect()
    };
    let s = s.trim().to_owned();
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_pdf_text_strings() {
        assert_eq!(text(b"Saga"), Some("Saga".into()));
        assert_eq!(
            text(&[0xFE, 0xFF, 0x00, b'S', 0x30, 0x42]),
            Some("S\u{3042}".into())
        );
        assert_eq!(text(b"  "), None);
        assert_eq!(text(&[0xE9]), Some("\u{e9}".into()));
    }
}
