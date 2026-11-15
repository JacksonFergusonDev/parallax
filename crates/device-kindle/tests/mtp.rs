//! The MTP path end to end, against mtp-rs's virtual device: a folder that
//! answers MTP requests the way a device does. It checks Parallax's MTP code,
//! not a real Kindle's MTP stack; see the Kindle hardware checks for that.

use std::sync::Arc;

use futures_lite::future::block_on;
use mtp_rs::MtpDevice;
use mtp_rs::virtual_device::{VirtualDeviceConfig, VirtualStorageConfig};
use parallax_device::Transport;
use parallax_device_kindle::mtp::MtpStorage;
use parallax_device_kindle::storage::Storage;
use parallax_device_kindle::{KindleDevice, MetaCache, Outgoing};

fn fixture() -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/amazon/book.azw3"),
    )
    .unwrap()
}

fn cover() -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(60, 90)
        .write_to(&mut out, image::ImageFormat::Png)
        .unwrap();
    out.into_inner()
}

fn open(dir: &std::path::Path, rename: bool) -> MtpStorage {
    std::fs::create_dir_all(dir.join("documents")).unwrap();
    std::fs::create_dir_all(dir.join("system/thumbnails")).unwrap();
    let device = block_on(MtpDevice::builder().open_virtual(VirtualDeviceConfig {
        manufacturer: "Amazon".into(),
        model: "Kindle Paperwhite".into(),
        serial: "G0033W0000000000".into(),
        storages: vec![VirtualStorageConfig {
            description: "Internal Storage".into(),
            capacity: 8 * 1024 * 1024 * 1024,
            backing_dir: dir.to_path_buf(),
            read_only: false,
        }],
        supports_rename: rename,
        ..VirtualDeviceConfig::default()
    }))
    .unwrap();
    MtpStorage::from_device(device).unwrap()
}

#[test]
fn storage_operations() {
    for rename in [true, false] {
        let dir = tempfile::tempdir().unwrap();
        let s = open(dir.path(), rename);
        assert_eq!(s.identity().serial, "G0033W0000000000");
        assert!(s.list("missing/folder").unwrap().is_empty());
        let mut last = 0;
        s.write("documents/a/b.txt", &vec![7; 1_200_000], &mut |w, _| {
            last = w
        })
        .unwrap();
        assert_eq!(last, 1_200_000);
        assert_eq!(s.size("documents/a/b.txt").unwrap(), Some(1_200_000));
        assert!(s.is_dir("documents/a").unwrap());
        assert_eq!(s.read_prefix("documents/a/b.txt", 3).unwrap(), [7, 7, 7]);
        // Replacing keeps one file under the final name.
        s.write("documents/a/b.txt", b"new", &mut |_, _| {})
            .unwrap();
        assert_eq!(s.read("documents/a/b.txt").unwrap(), b"new");
        let names: Vec<_> = s
            .list("documents/a")
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(names, ["b.txt"]);
        // Folders with files in them are emptied before they're deleted.
        s.remove("documents/a").unwrap();
        assert!(!s.exists("documents/a").unwrap());
        assert!(s.free_space().is_some());
    }
}

#[test]
fn send_scan_and_remove_over_mtp() {
    let dir = tempfile::tempdir().unwrap();
    let storage = Arc::new(open(dir.path(), true));
    let kindle = KindleDevice::new(
        storage,
        "mtp:G0033W0000000000".into(),
        "G0033W0000000000".into(),
        "5.18.1".into(),
        Transport::Mtp,
        Some("Kindle Paperwhite"),
    );
    assert_eq!(kindle.model_name(), "Kindle Paperwhite 6");
    let info = kindle.info();
    assert_eq!(info.transport, Transport::Mtp);
    assert_eq!(info.mount.to_string_lossy(), "mtp:G0033W0000000000");

    let data = fixture();
    let cover = cover();
    let sent = kindle
        .send(
            &Outgoing {
                file_name: "Book - Author.azw3",
                data: &data,
                cover: Some(&cover),
                page_numbers: true,
            },
            &mut |_, _| {},
        )
        .unwrap();
    // The files really are in the backing folder, where a Kindle would look.
    assert_eq!(
        std::fs::read(dir.path().join("documents/Book - Author.azw3")).unwrap(),
        data
    );
    assert!(
        dir.path()
            .join("documents/Book - Author.sdr/Book - Author.apnx")
            .is_file()
    );
    let thumbnail = sent.thumbnail.expect("thumbnail written");
    assert!(
        dir.path()
            .join("system/thumbnails")
            .join(&thumbnail)
            .is_file()
    );
    assert!(
        dir.path()
            .join("amazon-cover-bug")
            .join(&thumbnail)
            .is_file()
    );

    let mut cache = MetaCache::new();
    let books = kindle.books(&mut cache).unwrap();
    assert_eq!(books.len(), 1);
    assert_eq!(books[0].path, "documents/Book - Author.azw3");
    assert!(kindle.thumbnail(&books[0]).is_some());

    // Amazon deletes the thumbnail; connecting again puts it back.
    std::fs::remove_file(dir.path().join("system/thumbnails").join(&thumbnail)).unwrap();
    // The virtual device only notices files changed behind its back on rescan.
    let kindle = {
        let storage = Arc::new(open(dir.path(), true));
        KindleDevice::new(
            storage,
            "mtp:x".into(),
            "G0033W0000000000".into(),
            String::new(),
            Transport::Mtp,
            None,
        )
    };
    assert_eq!(kindle.restore_thumbnails().unwrap(), 1);
    assert!(
        dir.path()
            .join("system/thumbnails")
            .join(&thumbnail)
            .is_file()
    );

    kindle.remove(&sent.path).unwrap();
    assert!(!dir.path().join("documents/Book - Author.azw3").exists());
    assert!(!dir.path().join("documents/Book - Author.sdr").exists());
    assert!(
        !dir.path()
            .join("system/thumbnails")
            .join(&thumbnail)
            .exists()
    );
}
