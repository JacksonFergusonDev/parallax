use std::fs;
use std::path::{Path, PathBuf};

use erc_convert::Kepubify;
use erc_core::send::{Progress, send_books, send_library_books};
use erc_device_kobo::KoboDevice;
use erc_epub::{Epub, MetadataEdit};
use erc_library::{ImportOutcome, Library};

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

#[test]
fn sends_library_books_with_edited_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let mount = dir.path().join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    let kobo = KoboDevice::open(&mount).unwrap();
    let mut library = Library::open(dir.path().join("lib")).unwrap();
    let ImportOutcome::Added { book, .. } = library.import(&fixture("minimal.epub")).unwrap()
    else {
        panic!("fixture already imported");
    };
    library
        .update(
            book.id,
            &MetadataEdit {
                title: "Edited: Title".into(),
                authors: vec!["Ann Author".into()],
                series: Some("Saga".into()),
                series_index: Some("1".into()),
                tags: vec![],
            },
        )
        .unwrap();

    let outcomes = send_library_books(
        &library,
        &kobo,
        &Kepubify::locate().unwrap(),
        &[book.id],
        |_| {},
    );
    let sent = outcomes[0].result.as_ref().unwrap();
    assert_eq!(
        sent.device_path,
        "KEpub/Edited_ Title - Ann Author.kepub.epub"
    );

    // The KEPUB on the device carries the edited metadata.
    let on_device = Epub::open(mount.join(&sent.device_path))
        .unwrap()
        .metadata();
    assert_eq!(on_device.title.as_deref(), Some("Edited: Title"));
    assert_eq!(on_device.series.as_deref(), Some("Saga"));

    // Renaming and sending again is refused rather than leaving two copies.
    let mut edit = library.book(book.id).unwrap().edit();
    edit.title = "Renamed".into();
    library.update(book.id, &edit).unwrap();
    let again = send_library_books(
        &library,
        &kobo,
        &Kepubify::locate().unwrap(),
        &[book.id],
        |_| {},
    );
    assert!(
        again[0]
            .result
            .as_ref()
            .unwrap_err()
            .contains("already on the device")
    );

    let copies = library.device_copies("N000000000001").unwrap();
    assert_eq!(copies[0].book_id, book.id);
    assert_eq!(
        copies[0].content_id,
        format!("file:///mnt/onboard/{}", sent.device_path)
    );
}
