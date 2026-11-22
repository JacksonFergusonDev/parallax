//! PDF comics: scanned pages copied out unchanged, other pages drawn.

use std::io::Cursor;
use std::path::Path;

use parallax_comic::{Cbz, SkipReason, is_pdf, pdf_to_cbz};

/// A small PDF built object by object, with a correct cross-reference table.
fn pdf(objects: &[Vec<u8>], info: Option<usize>) -> Vec<u8> {
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    let info = info.map_or(String::new(), |i| format!(" /Info {i} 0 R"));
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R{info} >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let mut out = format!("<< {dict} /Length {} >>\nstream\n", data.len()).into_bytes();
    out.extend_from_slice(data);
    out.extend_from_slice(b"\nendstream");
    out
}

fn jpeg(width: u32, height: u32) -> Vec<u8> {
    let image = image::RgbImage::from_fn(width, height, |x, y| {
        image::Rgb([
            u8::try_from(x * 4 % 256).unwrap(),
            u8::try_from(y * 3 % 256).unwrap(),
            128,
        ])
    });
    let mut out = Cursor::new(Vec::new());
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 80)
        .encode_image(&image)
        .unwrap();
    out.into_inner()
}

/// Two pages: a full-page scan, then vector art with no images.
fn comic_pdf(scan: &[u8]) -> Vec<u8> {
    let vector = b"0.2 0.4 0.8 rg 20 20 160 260 re f 0 0 0 RG 4 w 30 30 m 170 270 l S";
    pdf(
        &[
            b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
            b"<< /Type /Pages /Kids [3 0 R 6 0 R] /Count 2 >>".to_vec(),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /XObject << /Im0 4 0 R >> >> /Contents 5 0 R >>".to_vec(),
            stream(
                "/Type /XObject /Subtype /Image /Width 400 /Height 600 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode",
                scan,
            ),
            stream("", b"q 200 0 0 300 0 0 cm /Im0 Do Q"),
            b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Contents 7 0 R >>".to_vec(),
            stream("", vector),
            b"<< /Title (The Long Road) /Author <FEFF004100720074> >>".to_vec(),
        ],
        Some(8),
    )
}

#[test]
fn scans_are_copied_and_other_pages_drawn() {
    let dir = tempfile::tempdir().unwrap();
    let scan = jpeg(400, 600);
    let src = dir.path().join("Road.pdf");
    std::fs::write(&src, comic_pdf(&scan)).unwrap();
    assert!(is_pdf(&src));
    assert!(!is_pdf(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("Cargo.toml")
            .as_path()
    ));

    let dest = dir.path().join("Road.cbz");
    let converted = pdf_to_cbz(&src, &dest).unwrap();
    assert_eq!(converted.pages, 2);
    assert!(converted.comic_info);
    assert!(converted.skipped.is_empty());

    let mut cbz = Cbz::open(&dest).unwrap();
    assert_eq!(cbz.pages().len(), 2);
    // The scan is the same bytes as in the PDF.
    assert_eq!(cbz.read_page(0).unwrap(), scan);
    // The vector page is drawn at the default size, blue in the middle.
    let drawn = image::load_from_memory(&cbz.read_page(1).unwrap())
        .unwrap()
        .to_rgb8();
    assert_eq!(drawn.height(), 2560);
    assert_eq!(drawn.width(), 2560 * 200 / 300);
    let px = drawn.get_pixel(drawn.width() / 4, drawn.height() / 2);
    assert!(px[2] > 150 && px[0] < 100, "{px:?}");
    let corner = drawn.get_pixel(2, 2);
    assert!(corner.0.iter().all(|&c| c > 240), "{corner:?}");

    let info = cbz.comic_info().unwrap();
    assert_eq!(info.title(), Some("The Long Road"));
    assert_eq!(info.get("Writer"), Some("Art"));
}

#[test]
fn a_page_that_cant_be_read_is_skipped() {
    let dir = tempfile::tempdir().unwrap();
    // The second page's image claims to be a JPEG but isn't one.
    let mut bytes = comic_pdf(b"not a jpeg");
    let src = dir.path().join("broken.pdf");
    // Also make the vector page reference a missing object.
    let text = String::from_utf8_lossy(&bytes).replace("/Contents 7 0 R", "/Contents 9 0 R");
    bytes = text.into_bytes();
    std::fs::write(&src, &bytes).unwrap();
    let dest = dir.path().join("broken.cbz");
    match pdf_to_cbz(&src, &dest) {
        Ok(converted) => {
            assert!(converted.pages >= 1);
            assert!(
                converted
                    .skipped
                    .iter()
                    .all(|s| s.reason == SkipReason::Unreadable)
            );
        }
        Err(e) => assert!(matches!(e, parallax_comic::Error::NoPages(_)), "{e}"),
    }
}

#[test]
fn rejects_files_that_arent_pdfs() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("x.pdf");
    std::fs::write(&src, b"%PDF-1.4 but nothing else").unwrap();
    let err = pdf_to_cbz(&src, &dir.path().join("x.cbz")).unwrap_err();
    assert!(
        matches!(
            err,
            parallax_comic::Error::Damaged { .. } | parallax_comic::Error::NoPages(_)
        ),
        "{err}"
    );
    assert!(!dir.path().join("x.cbz").exists());
    let mut leftover = String::new();
    for entry in std::fs::read_dir(dir.path()).unwrap() {
        let mut name = entry.unwrap().file_name().to_string_lossy().into_owned();
        name.push('\n');
        leftover.push_str(&name);
    }
    assert_eq!(leftover.trim(), "x.pdf");
}
