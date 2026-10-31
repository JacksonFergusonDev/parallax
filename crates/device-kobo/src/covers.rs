//! Read Nickel's cached thumbnails without modifying the mounted device.

use std::fs;
use std::path::{Component, Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::KoboDevice;

impl KoboDevice {
    /// Copy a cached cover into the host cache. Content-addressed names keep
    /// webview URLs fresh when Nickel regenerates a cover. Missing or unreadable
    /// thumbnails are optional and must not prevent listing the device's books.
    pub fn cache_cover(&self, image_id: &str, host_cache: &Path) -> Option<PathBuf> {
        // ImageId is database input, not a trusted path component.
        let mut components = Path::new(image_id).components();
        if !matches!(components.next(), Some(Component::Normal(_))) || components.next().is_some() {
            return None;
        }
        // LIBRARY_FULL is large enough for desktop cards without loading the
        // much larger full-screen cover for every book.
        for kind in ["N3_LIBRARY_FULL", "N3_FULL", "N3_LIBRARY_GRID"] {
            let source = self
                .cover_cache_dir(image_id)
                .join(format!("{image_id} - {kind}.parsed"));
            let Ok(bytes) = fs::read(source) else {
                continue;
            };
            // .parsed files contain ordinary images; give the host copy an
            // image extension so the asset protocol serves the correct MIME.
            let extension = if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
                "jpg"
            } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
                "png"
            } else {
                continue;
            };
            let hash: String = Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            let target = host_cache.join(format!("{hash}.{extension}"));
            fs::create_dir_all(host_cache).ok()?;
            if !target.is_file() {
                fs::write(&target, bytes).ok()?;
            }
            return Some(target);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_nickel_covers_into_host_cache() {
        let dir = tempfile::tempdir().unwrap();
        let mount = dir.path().join("device");
        fs::create_dir_all(mount.join(".kobo")).unwrap();
        fs::write(
            mount.join(".kobo/version"),
            "N000000000001,4.1.15,4.38.23697,4.1.15,4.1.15,00000000-0000-0000-0000-000000000388",
        )
        .unwrap();
        let device = KoboDevice::open(&mount).unwrap();
        let cache = dir.path().join("host-cache");
        let id = "custom-image-id";
        let source_dir = device.cover_cache_dir(id);
        fs::create_dir_all(&source_dir).unwrap();
        let source = source_dir.join(format!("{id} - N3_LIBRARY_FULL.parsed"));
        let first = b"\xff\xd8\xfffirst";
        fs::write(&source, first).unwrap();
        let cover = device.cache_cover(id, &cache).unwrap();
        assert_eq!(fs::read(&cover).unwrap(), first);
        assert_eq!(cover.extension().unwrap(), "jpg");
        assert_eq!(fs::read(&source).unwrap(), first);
        assert_eq!(device.cache_cover(id, &cache), Some(cover.clone()));
        fs::write(&source, b"\xff\xd8\xffchanged").unwrap();
        assert_ne!(device.cache_cover(id, &cache).unwrap(), cover);
        fs::write(&source, b"invalid image").unwrap();
        assert!(device.cache_cover(id, &cache).is_none());
        let fallback = source_dir.join(format!("{id} - N3_LIBRARY_GRID.parsed"));
        fs::write(fallback, first).unwrap();
        assert_eq!(device.cache_cover(id, &cache), Some(cover));
        assert!(device.cache_cover("missing", &cache).is_none());
        assert!(device.cache_cover("../escape", &cache).is_none());
        assert!(device.cache_cover("/absolute", &cache).is_none());
    }
}
