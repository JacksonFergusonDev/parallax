use std::fs;
use std::path::{Path, PathBuf};

use erc_convert::Kepubify;
use erc_core::send::{Progress, send_books};
use erc_device_kobo::KoboDevice;

const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/epub")
        .join(name)
}

#[test]
fn converts_and_sends_each_book_independently() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();

    let broken = dir.path().join("Broken.epub");
    fs::write(&broken, b"not an epub").unwrap();
    let sources = vec![fixture("minimal.epub"), broken];

    let mut events = Vec::new();
    let outcomes = send_books(&kobo, &Kepubify::locate().unwrap(), &sources, |p| {
        events.push(p)
    });

    assert!(outcomes[0].result.is_ok());
    assert!(mount.join("KEpub/minimal.kepub.epub").is_file());
    assert!(outcomes[1].result.is_err());
    assert!(!mount.join("KEpub/Broken.kepub.epub").exists());
    assert!(matches!(
        events.first(),
        Some(Progress::Converting {
            index: 1,
            total: 2,
            ..
        })
    ));
    assert!(matches!(
        events.last(),
        Some(Progress::Failed { index: 2, .. })
    ));

    // Sending the same book again is refused rather than overwriting.
    let again = send_books(&kobo, &Kepubify::locate().unwrap(), &sources[..1], |_| {});
    assert!(
        again[0]
            .result
            .as_ref()
            .unwrap_err()
            .contains("already on the device")
    );
}
