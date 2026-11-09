use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use erc_device_kobo::{Error, KoboDevice};

const SCHEMA: &str = include_str!("../../../tests/fixtures/kobo/schema-db174.sql");
const VERSION: &str =
    "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388";

fn fake_kobo(dir: &Path) -> KoboDevice {
    let mount = dir.join("KOBOeReader");
    fs::create_dir_all(mount.join(".kobo")).unwrap();
    fs::write(mount.join(".kobo/version"), VERSION).unwrap();
    rusqlite::Connection::open(mount.join(".kobo/KoboReader.sqlite"))
        .unwrap()
        .execute_batch(SCHEMA)
        .unwrap();
    KoboDevice::open(mount).unwrap()
}

fn book(dir: &Path) -> PathBuf {
    let src = dir.join("src.kepub.epub");
    fs::write(&src, vec![7u8; 3 * 1024 * 1024 + 17]).unwrap();
    src
}

#[test]
fn sends_book_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let src = book(dir.path());

    let sent = kobo.send_file(&src, "KEpub/Some Book.kepub.epub").unwrap();
    assert_eq!(
        sent.content_id,
        "file:///mnt/onboard/KEpub/Some Book.kepub.epub"
    );
    assert_eq!(sent.bytes, fs::metadata(&src).unwrap().len());
    let dest = kobo.mount().join("KEpub/Some Book.kepub.epub");
    assert_eq!(fs::read(&dest).unwrap(), fs::read(&src).unwrap());
    // No temp files left, Spotlight disabled.
    let leftovers: Vec<_> = fs::read_dir(kobo.mount().join("KEpub"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(leftovers.len(), 1);
    assert!(kobo.mount().join(".metadata_never_index").exists());
}

#[test]
fn refuses_to_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let src = book(dir.path());
    kobo.send_file(&src, "KEpub/a.kepub.epub").unwrap();
    let err = kobo.send_file(&src, "KEpub/a.kepub.epub").unwrap_err();
    assert!(matches!(err, Error::AlreadyOnDevice(_)));
}

#[test]
fn missing_source_leaves_nothing_behind() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    assert!(
        kobo.send_file(&dir.path().join("nope.epub"), "KEpub/x.kepub.epub")
            .is_err()
    );
    let entries: Vec<_> = fs::read_dir(kobo.mount().join("KEpub"))
        .unwrap()
        .flatten()
        .collect();
    assert!(entries.is_empty());
}

/// Extended attributes on the source must not reach the device. (macOS adds
/// its own `com.apple.provenance` to every new file; that one is expected.)
#[cfg(target_os = "macos")]
#[test]
fn does_not_copy_extended_attributes() {
    let dir = tempfile::tempdir().unwrap();
    let kobo = fake_kobo(dir.path());
    let src = book(dir.path());
    let ok = Command::new("xattr")
        .args(["-w", "com.apple.metadata:kMDItemWhereFroms", "test"])
        .arg(&src)
        .status()
        .unwrap();
    assert!(ok.success());
    kobo.send_file(&src, "KEpub/x.kepub.epub").unwrap();
    let out = Command::new("xattr")
        .arg(kobo.mount().join("KEpub/x.kepub.epub"))
        .output()
        .unwrap();
    let attrs = String::from_utf8_lossy(&out.stdout);
    assert!(
        !attrs.contains("kMDItemWhereFroms"),
        "xattrs copied: {attrs}"
    );
}

/// Sends a book to a real FAT32 volume (a disk image) and checks that no `._*`
/// files are left. Mounts an image, so it is opt-in: `just test-fat`.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "mounts a disk image: just test-fat"]
fn leaves_no_apple_double_files_on_fat32() {
    let dir = tempfile::tempdir().unwrap();
    let image = dir.path().join("kobo.dmg");
    let mount = dir.path().join("mnt");
    fs::create_dir(&mount).unwrap();
    let run = |cmd: &str, args: &[&str]| {
        let out = Command::new(cmd).args(args).output().unwrap();
        assert!(
            out.status.success(),
            "{cmd} {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let (image, mount_str) = (image.to_str().unwrap(), mount.to_str().unwrap());
    run(
        "diskutil",
        &[
            "image",
            "create",
            "blank",
            "--format",
            "RAW",
            "--size",
            "40m",
            "--volumeName",
            "KOBOTEST",
            "--fs",
            "MS-DOS",
            image,
        ],
    );
    run(
        "hdiutil",
        &["attach", "-nobrowse", "-mountpoint", mount_str, image],
    );

    let result = std::panic::catch_unwind(|| {
        // Set up the fake device with plain writes, then clear their `._*`
        // files so only send_file's behaviour is measured.
        fs::create_dir_all(mount.join(".kobo")).unwrap();
        fs::write(mount.join(".kobo/version"), VERSION).unwrap();
        rusqlite::Connection::open(mount.join(".kobo/KoboReader.sqlite"))
            .unwrap()
            .execute_batch(SCHEMA)
            .unwrap();
        for e in ["._.kobo", ".kobo/._version", ".kobo/._KoboReader.sqlite"] {
            let _ = fs::remove_file(mount.join(e));
        }
        let kobo = KoboDevice::open(&mount).unwrap();
        let src = book(dir.path());
        kobo.send_file(&src, "KEpub/Genre/New Book.kepub.epub")
            .unwrap();
        let mut apple_double = Vec::new();
        for sub in ["", "KEpub", "KEpub/Genre", ".erc"] {
            for e in fs::read_dir(mount.join(sub)).unwrap().flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with("._") {
                    apple_double.push(format!("{sub}/{name}"));
                }
            }
        }
        apple_double
    });
    run("hdiutil", &["detach", mount_str]);
    let apple_double = result.unwrap();
    assert!(apple_double.is_empty(), "left behind: {apple_double:?}");
}
