use std::fs;
use std::io::Read;
use std::path::Path;

use parallax_convert::{Converter, Error, Kepubify};

fn fixture(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub")
        .join(name)
}

/// Minimal check that the output is a zip containing a spine document with
/// Kobo spans, without pulling in a zip crate: the span marker appears in a
/// deflated entry, so look for the stored `mimetype` header instead and
/// rely on kepubify's own validation for the rest.
fn looks_like_epub(path: &Path) -> bool {
    let mut head = [0u8; 58];
    fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .is_ok()
        && &head[..2] == b"PK"
        && head.windows(20).any(|w| w == b"application/epub+zip")
}

#[test]
fn converts_epub_to_kepub() {
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("book.kepub.epub");
    Kepubify::locate()
        .unwrap()
        .convert(&fixture("minimal.epub"), &out)
        .unwrap();
    assert!(looks_like_epub(&out));
}

#[test]
fn reports_invalid_input() {
    let dir = tempfile::tempdir().unwrap();
    let bad = dir.path().join("broken.epub");
    fs::write(&bad, b"not a zip").unwrap();
    let err = Kepubify::locate()
        .unwrap()
        .convert(&bad, &dir.path().join("out.kepub.epub"))
        .unwrap_err();
    assert!(matches!(err, Error::Failed { .. }), "{err}");
}

#[test]
fn missing_binary_is_a_clear_error() {
    let err = Kepubify::new("/nonexistent/kepubify")
        .convert(&fixture("minimal.epub"), Path::new("/tmp/never.kepub.epub"))
        .unwrap_err();
    assert!(matches!(err, Error::Spawn(_)));
}
