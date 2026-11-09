//! KEPUB cleanup undoes kepubify: the text is unchanged and converting the
//! cleaned book again gives the same KEPUB.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use parallax_convert::{Converter, Kepubify};
use parallax_epub::strip_kobo_markup;
use zip::ZipArchive;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub")
        .join(name)
}

/// Every zip entry's bytes, by name.
fn entries(path: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut zip = ZipArchive::new(File::open(path).unwrap()).unwrap();
    (0..zip.len())
        .map(|i| {
            let mut e = zip.by_index(i).unwrap();
            let mut bytes = Vec::new();
            e.read_to_end(&mut bytes).unwrap();
            (e.name().unwrap().into_owned(), bytes)
        })
        .collect()
}

/// A content document's text with tags removed and whitespace collapsed.
fn text(bytes: &[u8]) -> String {
    let s = String::from_utf8_lossy(bytes);
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_doc(name: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("xhtml") || ext.eq_ignore_ascii_case("html"))
}

#[test]
fn cleanup_undoes_kepubify() {
    let dir = tempfile::tempdir().unwrap();
    let kepubify = Kepubify::locate().unwrap();
    let original = fixture("minimal.epub");
    let kepub = dir.path().join("book.kepub.epub");
    kepubify.convert(&original, &kepub).unwrap();

    let cleaned = dir.path().join("clean.epub");
    let report = strip_kobo_markup(&kepub, &cleaned).unwrap();
    assert!(report.changed());
    assert_eq!(report.unparsable, 0);

    let before = entries(&original);
    let after = entries(&cleaned);
    assert_eq!(
        before.keys().collect::<Vec<_>>(),
        after.keys().collect::<Vec<_>>(),
        "the dummy title page is gone and nothing else was dropped"
    );
    for (name, bytes) in &after {
        let s = String::from_utf8_lossy(bytes);
        for marker in [
            "koboSpan",
            "book-inner",
            "book-columns",
            "kobostylehacks",
            "kepubify",
        ] {
            assert!(!s.contains(marker), "{marker} left in {name}");
        }
        if is_doc(name) {
            assert_eq!(text(bytes), text(&before[name]), "text of {name}");
        }
    }

    // Converting the cleaned book reproduces the same KEPUB content.
    let again = dir.path().join("again.kepub.epub");
    kepubify.convert(&cleaned, &again).unwrap();
    let first = entries(&kepub);
    let second = entries(&again);
    for (name, bytes) in &first {
        if is_doc(name) {
            assert_eq!(
                String::from_utf8_lossy(&second[name]),
                String::from_utf8_lossy(bytes),
                "{name}"
            );
        }
    }

    // A plain EPUB passes through untouched.
    let plain = dir.path().join("plain.epub");
    let report = strip_kobo_markup(&original, &plain).unwrap();
    assert!(!report.changed());
}
