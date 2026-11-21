//! Kindle workflows against a drive-mode Kindle in a temporary folder: send,
//! update, reading progress and highlights, adding from the Kindle, removal.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parallax_convert::amazon::{EpubToAzw3, InProcess};
use parallax_core::import::DeviceImport;
use parallax_core::kindle::{self, SendOptions};
use parallax_device::Transport;
use parallax_device_kindle::storage::DriveStorage;
use parallax_device_kindle::{KindleDevice, MetaCache};
use parallax_library::{ImportOutcome, Library};

fn fixture(path: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(path)
}

fn kindle(root: &Path) -> KindleDevice {
    fs::create_dir_all(root.join("documents")).unwrap();
    fs::create_dir_all(root.join("system/thumbnails")).unwrap();
    KindleDevice::new(
        Arc::new(DriveStorage::new(root)),
        root.to_string_lossy().into_owned(),
        "G0033W0000000001".into(),
        String::new(),
        Transport::Drive,
        None,
    )
}

fn import(library: &mut Library, path: &Path) -> i64 {
    match library.import(path).unwrap() {
        ImportOutcome::Added { book, .. } | ImportOutcome::AlreadyInLibrary { book } => book.id,
    }
}

/// A KRDS `.azw3f` saying the Kindle is at `position`.
fn sidecar(position: u32) -> Vec<u8> {
    let mut out = b"\x00\x00\x00\x00\x00\x1A\xB1\x26".to_vec();
    let int = |out: &mut Vec<u8>, n: i32| {
        out.push(1);
        out.extend_from_slice(&n.to_be_bytes());
    };
    let utf = |out: &mut Vec<u8>, s: &str| {
        out.push(0);
        out.extend_from_slice(&u16::try_from(s.len()).unwrap().to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    int(&mut out, 1);
    int(&mut out, 1);
    out.push(0xFE);
    utf(&mut out, "lpr");
    int(&mut out, 2);
    out.push(3);
    utf(&mut out, &position.to_string());
    out.push(2);
    out.extend_from_slice(&1_762_000_000_000i64.to_be_bytes());
    out.push(0xFF);
    out
}

/// A KRDS `.azw3r` holding highlights over these (start, end) ranges.
fn highlights(ranges: &[(usize, usize)]) -> Vec<u8> {
    let mut out = b"\x00\x00\x00\x00\x00\x1A\xB1\x26".to_vec();
    let int = |out: &mut Vec<u8>, n: i32| {
        out.push(1);
        out.extend_from_slice(&n.to_be_bytes());
    };
    let long = |out: &mut Vec<u8>, n: i64| {
        out.push(2);
        out.extend_from_slice(&n.to_be_bytes());
    };
    let utf = |out: &mut Vec<u8>, s: &str| {
        out.push(0);
        out.extend_from_slice(&u16::try_from(s.len()).unwrap().to_be_bytes());
        out.extend_from_slice(s.as_bytes());
    };
    let text = |out: &mut Vec<u8>, s: &str| {
        out.push(3);
        utf(out, s);
    };
    int(&mut out, 1);
    int(&mut out, 1);
    out.push(0xFE);
    utf(&mut out, "annotation.cache.object");
    int(&mut out, 1);
    int(&mut out, 1);
    out.push(0xFE);
    utf(&mut out, "saved.avl.interval.tree");
    int(&mut out, i32::try_from(ranges.len()).unwrap());
    for (start, end) in ranges {
        out.push(0xFE);
        utf(&mut out, "annotation.personal.highlight");
        text(&mut out, &start.to_string());
        text(&mut out, &end.to_string());
        long(&mut out, 1_762_000_000_000);
        long(&mut out, 1_762_000_000_000);
        text(&mut out, "");
        out.push(0xFF);
    }
    out.push(0xFF);
    out.push(0xFF);
    out.push(0xFF);
    out
}

#[test]
fn highlights_missing_from_clippings_take_their_text_from_the_book() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let id = import(&mut library, &fixture("epub/minimal.epub"));
    let device = kindle(&dir.path().join("Kindle"));
    let options = SendOptions {
        converter: &EpubToAzw3,
        page_numbers: true,
    };
    let outcomes = kindle::send_library_books(&library, &device, &options, &[id], |_| {});
    let sent = outcomes[0].result.as_ref().unwrap().clone();
    let file = dir.path().join("Kindle").join(&sent.device_path);
    let text = parallax_device_kindle::text::extract(&fs::read(&file).unwrap()).unwrap();
    let html = String::from_utf8_lossy(&text).into_owned();
    // The text of the book's first paragraph, from the Kindle's own copy.
    let p = html.find("<p").unwrap();
    let start = p + html[p..].find('>').unwrap() + 1;
    let end = start + html[start..].find("</p>").unwrap() - 1;
    let expected = parallax_device_kindle::text::passage(&text, start as u64, end as u64).unwrap();
    assert!(!expected.is_empty());

    let stem = sent.device_path.trim_end_matches(".azw3");
    let sdr = dir.path().join("Kindle").join(format!("{stem}.sdr"));
    fs::create_dir_all(&sdr).unwrap();
    fs::write(sdr.join("x.azw3r"), highlights(&[(start, end)])).unwrap();
    let mut cache = MetaCache::new();
    let done = kindle::reconcile(&mut library, &device, &mut cache).unwrap();
    assert_eq!(done.annotations, 1);
    let notes = library.annotations(id).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].kind, "highlight");
    assert_eq!(notes[0].text.as_deref(), Some(expected.as_str()));
    assert_eq!(
        notes[0].position,
        f64::from(parallax_device_kindle::text::location(start as u64))
    );

    // Once My Clippings has the same highlight, only its entry is kept.
    let books = device.books(&mut cache).unwrap();
    let location = parallax_device_kindle::text::location(start as u64);
    fs::write(
        dir.path().join("Kindle/documents/My Clippings.txt"),
        format!(
            "{} (Someone)\n- Your Highlight on Location {location}-{} | Added on Monday, 3 November 2025 21:04:10\n\nFrom clippings.\n==========\n",
            books[0].title(),
            location + 1
        ),
    )
    .unwrap();
    kindle::reconcile(&mut library, &device, &mut cache).unwrap();
    let on_device: Vec<_> = library
        .annotations(id)
        .unwrap()
        .into_iter()
        .filter(|a| a.on_device)
        .collect();
    assert_eq!(on_device.len(), 1);
    assert_eq!(on_device[0].text.as_deref(), Some("From clippings."));
}

#[test]
fn send_update_sync_and_remove() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let id = import(&mut library, &fixture("epub/minimal.epub"));
    let device = kindle(&dir.path().join("Kindle"));
    let options = SendOptions {
        converter: &EpubToAzw3,
        page_numbers: true,
    };

    let outcomes = kindle::send_library_books(&library, &device, &options, &[id], |_| {});
    let sent = outcomes[0].result.as_ref().unwrap().clone();
    assert!(
        sent.device_path.starts_with("documents/")
            && Path::new(&sent.device_path)
                .extension()
                .is_some_and(|e| e == "azw3")
    );
    let file = dir.path().join("Kindle").join(&sent.device_path);
    assert!(file.is_file());
    let copies = library.device_copies(device.serial()).unwrap();
    assert_eq!(copies.len(), 1);
    assert_eq!(copies[0].content_id, sent.device_path);

    // Sending again is refused while the copy is there.
    let again = kindle::send_library_books(&library, &device, &options, &[id], |_| {});
    assert!(
        again[0]
            .result
            .as_ref()
            .unwrap_err()
            .contains("already on the Kindle")
    );

    let mut cache = MetaCache::new();
    let books = device.books(&mut cache).unwrap();
    assert_eq!(books.len(), 1);
    let on = kindle::books_on_device(&library, &device, &books).unwrap();
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].book_id, id);
    let catalog = kindle::catalog(&books);
    assert!(catalog[0].book.sideloaded);

    // The Kindle reads halfway and highlights a line.
    let stem = sent.device_path.trim_end_matches(".azw3");
    let text_length = books[0].meta.text_length;
    fs::write(
        dir.path()
            .join("Kindle")
            .join(format!("{stem}.sdr/x.azw3f")),
        sidecar(text_length / 2),
    )
    .unwrap();
    let title = books[0].title();
    fs::write(
        dir.path().join("Kindle/documents/My Clippings.txt"),
        format!(
            "{title} (Someone)\n- Your Highlight on Location 1-2 | Added on Monday, 3 November 2025 21:04:10\n\nA line.\n==========\nOther Book (X)\n- Your Highlight on Location 9 | Added on Monday, 3 November 2025 21:04:10\n\nNot this one.\n==========\n"
        ),
    )
    .unwrap();
    let done = kindle::reconcile(&mut library, &device, &mut cache).unwrap();
    assert_eq!(done.reading, 1);
    assert_eq!(done.annotations, 1);
    let reading = library.reading(id).unwrap().unwrap();
    assert_eq!(reading.status, "reading");
    assert_eq!(reading.percent, 50);
    let notes = library.annotations(id).unwrap();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].text.as_deref(), Some("A line."));
    assert_eq!(
        notes[0].created.as_deref(),
        Some("2025-11-03T21:04:10+00:00")
    );

    // Updating replaces the file in place and keeps the sidecar folder.
    let updated = kindle::update_on_device(&library, &device, &options, &[id], |_| {});
    assert_eq!(
        updated[0].result.as_ref().unwrap().device_path,
        sent.device_path
    );
    assert!(
        dir.path()
            .join("Kindle")
            .join(format!("{stem}.sdr/x.azw3f"))
            .is_file()
    );

    let removed = kindle::remove_from_device(
        Some(&library),
        &device,
        std::slice::from_ref(&sent.device_path),
        |_, _| {},
    );
    assert!(removed[0].result.is_ok());
    assert_eq!(removed[0].book_id, Some(id));
    assert!(!file.exists());
    assert!(library.device_copies(device.serial()).unwrap().is_empty());
}

#[test]
fn comics_are_sent_as_fixed_layout_azw3() {
    use parallax_device_kindle::mobi;

    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let cbz = dir.path().join("Comic.cbz");
    let mut zip = zip::ZipWriter::new(fs::File::create(&cbz).unwrap());
    for (i, shade) in [40u8, 90, 160].into_iter().enumerate() {
        zip.start_file(
            format!("p{}.png", i + 1),
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        let mut png = Vec::new();
        image::GrayImage::from_fn(600, 900, |x, _| {
            image::Luma([shade.wrapping_add((x % 7) as u8)])
        })
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .unwrap();
        std::io::Write::write_all(&mut zip, &png).unwrap();
    }
    zip.finish().unwrap();
    // Comics are taken as right-to-left manga unless set otherwise.
    let id = import(&mut library, &cbz);
    let device = kindle(&dir.path().join("Kindle"));
    let options = SendOptions {
        converter: &EpubToAzw3,
        page_numbers: true,
    };
    let outcomes = kindle::send_library_books(&library, &device, &options, &[id], |_| {});
    let sent = outcomes[0].result.as_ref().unwrap();
    assert!(
        Path::new(&sent.device_path)
            .extension()
            .is_some_and(|e| e == "azw3"),
        "{}",
        sent.device_path
    );
    let bytes = fs::read(dir.path().join("Kindle").join(&sent.device_path)).unwrap();

    let header = mobi::parse(&bytes).unwrap();
    assert_eq!(header.mobi_version, 8);
    assert_eq!(header.title.as_deref(), Some("Comic"));
    let records = mobi::exth_records(&bytes);
    let get = |k: u32| {
        records
            .iter()
            .find(|(kind, _)| *kind == k)
            .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
    };
    let screen = device.info().screen;
    assert_eq!(get(122).as_deref(), Some("true"));
    assert_eq!(get(123).as_deref(), Some("comic"));
    assert_eq!(
        get(126),
        Some(format!("{}x{}", screen.width, screen.height))
    );
    assert_eq!(get(527).as_deref(), Some("rtl"));

    // One page document per page, each showing a JPEG.
    let text = parallax_device_kindle::text::extract(&bytes).unwrap();
    let html = String::from_utf8_lossy(&text);
    assert_eq!(html.matches("<img").count(), 3, "{html}");
    assert!(html.contains("name=\"viewport\""), "{html}");
    let offsets = mobi::record_offsets(&bytes).unwrap();
    let jpegs = offsets
        .iter()
        .filter(|&&o| bytes[o..].starts_with(&[0xFF, 0xD8]))
        .count();
    assert!(jpegs >= 3, "{jpegs} JPEG records");
    // Fixed-layout pages need no page numbers.
    let stem = sent.device_path.trim_end_matches(".azw3");
    assert!(
        !dir.path()
            .join("Kindle")
            .join(format!("{stem}.sdr"))
            .join(format!(
                "{}.apnx",
                Path::new(stem).file_name().unwrap().to_string_lossy()
            ))
            .exists()
    );
}

#[test]
fn adds_sideloaded_books_and_skips_store_books() {
    let dir = tempfile::tempdir().unwrap();
    let mut library = Library::open(dir.path().join("library")).unwrap();
    let root = dir.path().join("Kindle");
    let device = kindle(&root);
    fs::copy(
        fixture("amazon/book.azw3"),
        root.join("documents/Sideloaded.azw3"),
    )
    .unwrap();
    fs::copy(
        fixture("amazon/book.azw3"),
        root.join("documents/Bought-asin_B00ABCDEFG-type_EBOK-v_0.azw"),
    )
    .unwrap();
    fs::write(root.join("documents/Notes.pdf"), b"%PDF-1.4").unwrap();

    let books = device.books(&mut MetaCache::new()).unwrap();
    let outcomes = kindle::import_from_device(
        &mut library,
        &device,
        &books,
        None,
        &InProcess,
        |_, _, _| {},
    );
    let by_path = |p: &str| {
        outcomes
            .iter()
            .find(|o| o.content_id.ends_with(p))
            .unwrap()
            .result
            .clone()
            .unwrap()
    };
    let DeviceImport::Added { book_id } = by_path("Sideloaded.azw3") else {
        panic!("not added: {:?}", by_path("Sideloaded.azw3"));
    };
    assert!(library.book(book_id).is_ok());
    assert!(
        matches!(by_path("v_0.azw"), DeviceImport::Skipped { reason } if reason.contains("Kindle Store"))
    );
    assert!(
        matches!(by_path("Notes.pdf"), DeviceImport::Skipped { reason } if reason.contains(".pdf"))
    );

    // A second import links nothing new.
    let again = kindle::import_from_device(
        &mut library,
        &device,
        &books,
        Some(&["documents/Sideloaded.azw3".into()]),
        &InProcess,
        |_, _, _| {},
    );
    assert!(
        matches!(&again[0].result, Ok(DeviceImport::Skipped { reason }) if reason.contains("already"))
    );
}
