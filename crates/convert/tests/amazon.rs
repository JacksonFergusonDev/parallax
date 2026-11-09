//! Amazon books to EPUB through boko, on small books made by calibre (see
//! tests/fixtures/amazon/make.py).

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use parallax_convert::amazon::{InProcess, Kind, sniff};
use parallax_convert::{Converter, Error, ToKepub};
use parallax_epub::Epub;
use tempfile::TempDir;
use zip::ZipArchive;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/amazon")
        .join(name)
}

fn convert(name: &str) -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("book.epub");
    InProcess.convert(&fixture(name), &out).unwrap();
    (dir, out)
}

/// Every file in the EPUB, as (name, bytes).
fn entries(path: &Path) -> Vec<(String, Vec<u8>)> {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut entry = zip.by_index(i).unwrap();
            let mut data = Vec::new();
            entry.read_to_end(&mut data).unwrap();
            (entry.name().unwrap().into_owned(), data)
        })
        .collect()
}

fn has_extension(name: &str, extensions: &[&str]) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| extensions.iter().any(|x| e.eq_ignore_ascii_case(x)))
}

fn html(entries: &[(String, Vec<u8>)]) -> Vec<(String, String)> {
    entries
        .iter()
        .filter(|(n, _)| has_extension(n, &["html", "xhtml"]))
        .map(|(n, d)| (n.clone(), String::from_utf8_lossy(d).into_owned()))
        .collect()
}

fn images(entries: &[(String, Vec<u8>)]) -> usize {
    entries
        .iter()
        .filter(|(n, _)| has_extension(n, &["png", "jpg", "jpeg"]))
        .count()
}

/// Internal links must land on an element that exists, not just in the right
/// file, or the reader ends up at the top of the chapter. Returns how many
/// anchored links there were.
fn assert_anchors_resolve(path: &Path) -> usize {
    let docs = html(&entries(path));
    let mut checked = 0;
    for (name, text) in &docs {
        for rest in text.split("href=\"").skip(1) {
            let href = &rest[..rest.find('"').unwrap()];
            let Some((file, anchor)) = href.split_once('#') else {
                continue;
            };
            let (_, target) = docs
                .iter()
                .find(|(n, _)| {
                    if file.is_empty() {
                        n == name
                    } else {
                        n.ends_with(file)
                    }
                })
                .unwrap_or_else(|| panic!("{href} from {name}: no such file"));
            assert!(
                target.contains(&format!("id=\"{anchor}\"")),
                "{href} from {name}: no such anchor"
            );
            checked += 1;
        }
    }
    checked
}

#[test]
fn azw3_keeps_text_images_styles_and_links() {
    let (_dir, out) = convert("book.azw3");
    let files = entries(&out);
    let text: String = html(&files).into_iter().map(|(_, t)| t).collect();
    assert!(text.contains("The footnote text."));
    assert!(images(&files) >= 2, "cover and plate");
    assert!(files.iter().any(|(n, _)| has_extension(n, &["css"])));
    assert!(
        assert_anchors_resolve(&out) >= 2,
        "footnote and chapter links"
    );

    let epub = Epub::open(&out).unwrap();
    let meta = epub.metadata();
    assert_eq!(meta.title.as_deref(), Some("Amazon Fixture"));
    assert_eq!(meta.authors, ["Test Author"]);
    assert!(epub.cover().unwrap().is_some());
}

#[test]
fn combo_file_uses_the_kf8_half_and_keeps_its_images() {
    let (_dir, out) = convert("combo.mobi");
    let files = entries(&out);
    assert!(images(&files) >= 2, "images live in the first half");
    assert!(
        files.iter().any(|(n, _)| has_extension(n, &["css"])),
        "only the KF8 half has stylesheets"
    );
    assert!(
        assert_anchors_resolve(&out) >= 2,
        "footnote and chapter links"
    );
    assert!(Epub::open(&out).unwrap().cover().unwrap().is_some());
}

/// `combo.mobi` with `image` added as a new last record, named by the KF8 header
/// as its first resource. calibre keeps every resource in the first half, but
/// KindleGen files can have some in the KF8 half.
fn combo_with_kf8_image(image: &[u8]) -> Vec<u8> {
    const COUNT: usize = 76;
    const TABLE: usize = 78;
    const KF8_RECORD: usize = 13;
    const FIRST_IMAGE: usize = 108;
    let combo = fs::read(fixture("combo.mobi")).unwrap();
    let u32_at = |d: &[u8], at: usize| u32::from_be_bytes(d[at..at + 4].try_into().unwrap());

    let records = usize::from(u16::from_be_bytes(
        combo[COUNT..COUNT + 2].try_into().unwrap(),
    ));
    let table_end = TABLE + records * 8;
    let mut book = combo[..table_end].to_vec();
    for i in 0..records {
        let offset = u32_at(&combo, TABLE + i * 8) + 8;
        book[TABLE + i * 8..TABLE + i * 8 + 4].copy_from_slice(&offset.to_be_bytes());
    }
    let new_offset = u32::try_from(combo.len() + 8).unwrap();
    book.extend_from_slice(&new_offset.to_be_bytes());
    book.extend_from_slice(&u32::try_from(records * 2).unwrap().to_be_bytes());
    book.extend_from_slice(&combo[table_end..]);
    book.extend_from_slice(image);
    book[COUNT..COUNT + 2].copy_from_slice(&u16::try_from(records + 1).unwrap().to_be_bytes());

    let header = u32_at(&book, TABLE + KF8_RECORD * 8) as usize + FIRST_IMAGE;
    let first = u32::try_from(records - KF8_RECORD).unwrap();
    book[header..header + 4].copy_from_slice(&first.to_be_bytes());
    book
}

#[test]
fn combo_file_keeps_images_from_both_halves() {
    let image = b"\x89PNG\r\n\x1a\nimage in the KF8 half";
    let dir = tempfile::tempdir().unwrap();
    let book = dir.path().join("book.mobi");
    let out = dir.path().join("book.epub");
    fs::write(&book, combo_with_kf8_image(image)).unwrap();

    InProcess.convert(&book, &out).unwrap();

    let files = entries(&out);
    let (_plain_dir, plain) = convert("combo.mobi");
    assert_eq!(images(&files), images(&entries(&plain)) + 1);
    assert!(files.iter().any(|(_, data)| data == image));
}

#[test]
fn old_mobi_without_an_ncx_gets_its_contents_from_the_guide() {
    const RECORD_0: usize = 78;
    const NCX_INDEX: usize = 244;
    let mut book = fs::read(fixture("old.mobi")).unwrap();
    let record0 = u32::from_be_bytes(book[RECORD_0..RECORD_0 + 4].try_into().unwrap()) as usize;
    book[record0 + NCX_INDEX..record0 + NCX_INDEX + 4].fill(0xff);
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("book.mobi");
    let out = dir.path().join("book.epub");
    fs::write(&input, book).unwrap();

    InProcess.convert(&input, &out).unwrap();

    let files = entries(&out);
    let (_, ncx) = files
        .iter()
        .find(|(n, _)| has_extension(n, &["ncx"]))
        .unwrap();
    let ncx = String::from_utf8_lossy(ncx);
    assert!(ncx.matches("<navPoint").count() >= 2, "{ncx}");
}

#[test]
fn old_mobi_converts() {
    let (_dir, out) = convert("old.mobi");
    let files = entries(&out);
    let text: String = html(&files).into_iter().map(|(_, t)| t).collect();
    assert!(text.contains("The footnote text."));
    assert!(images(&files) >= 2);
}

/// `file` with its Mobipocket encryption field set.
fn with_drm_flag(file: &Path, dest: &Path) {
    let mut data = fs::read(file).unwrap();
    let record0 = u32::from_be_bytes(data[78..82].try_into().unwrap()) as usize;
    data[record0 + 12..record0 + 14].copy_from_slice(&2u16.to_be_bytes());
    fs::write(dest, data).unwrap();
}

#[test]
fn drm_is_refused_without_leaving_output() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.epub");
    let azw3 = dir.path().join("locked.azw3");
    with_drm_flag(&fixture("book.azw3"), &azw3);
    let kfx = dir.path().join("locked.kfx");
    fs::write(&kfx, b"\xeaDRMION\xee rest of the file").unwrap();
    for locked in [azw3, kfx] {
        let err = InProcess.convert(&locked, &out).unwrap_err();
        assert!(matches!(err, Error::DrmProtected(_)), "{err}");
        assert!(!out.exists());
    }
}

#[test]
fn corrupt_books_fail_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.epub");
    let data = fs::read(fixture("book.azw3")).unwrap();
    let cut = dir.path().join("cut.azw3");
    fs::write(&cut, &data[..data.len() / 8]).unwrap();
    assert!(InProcess.convert(&cut, &out).is_err());
    assert!(!out.exists());
}

#[test]
fn files_are_recognised_by_content() {
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, data: &[u8]| {
        let path = dir.path().join(name);
        fs::write(&path, data).unwrap();
        path
    };
    assert_eq!(sniff(&fixture("old.mobi")).unwrap(), Some(Kind::Mobi));
    assert_eq!(
        sniff(&write("a.azw", b"CONT\x02")).unwrap(),
        Some(Kind::Kfx)
    );
    assert_eq!(
        sniff(&write("b.azw", b"\xeaDRMION\xee")).unwrap(),
        Some(Kind::EncryptedKfx)
    );
    assert_eq!(sniff(&write("c.azw", b"TPZ0x")).unwrap(), Some(Kind::Topaz));
    assert_eq!(sniff(&write("d.azw3", b"PK\x03\x04")).unwrap(), None);
    assert_eq!(sniff(&write("e.azw3", b"")).unwrap(), None);
}

/// Stands in for kepubify: records what it was given.
struct Copy;

impl Converter for Copy {
    fn convert(&self, input: &Path, output: &Path) -> parallax_convert::Result<()> {
        Epub::open(input).expect("kepubify must be given an EPUB");
        fs::copy(input, output)?;
        Ok(())
    }
}

#[test]
fn to_kepub_goes_through_epub() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("book.kepub.epub");
    ToKepub::new(InProcess, Copy)
        .convert(&fixture("book.azw3"), &out)
        .unwrap();
    assert!(Epub::open(&out).is_ok());

    let (_keep, epub) = convert("book.azw3");
    let direct = dir.path().join("direct.kepub.epub");
    ToKepub::new(InProcess, Copy)
        .convert(&epub, &direct)
        .unwrap();
    assert!(Epub::open(&direct).is_ok());
}

#[cfg(unix)]
#[test]
fn a_child_that_dies_is_an_error_not_a_crash() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.epub");
    let err = parallax_convert::amazon::ChildProcess::new("/usr/bin/false")
        .convert(&fixture("book.azw3"), &out)
        .unwrap_err();
    assert!(err.to_string().contains("crashed"), "{err}");
    assert!(!out.exists());
}
