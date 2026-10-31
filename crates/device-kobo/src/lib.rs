//! Kobo e-reader backend.

mod collections;
mod db;
mod error;
mod ops;
mod transfer;
mod version;
mod writer;

use std::fs;
use std::path::{Path, PathBuf};

use erc_device::{Capabilities, DeviceInfo, DeviceKind};

pub use collections::{
    add_to_collection, create_collection, delete_collection, remove_from_collection,
    rename_collection,
};
pub use db::{
    KoboDb, MAX_TESTED_DB_VERSION, MIN_SUPPORTED_DB_VERSION, ONBOARD_PREFIX, parse_kobo_date,
};
pub use error::{Error, Result};
pub use ops::{
    DisplayMetadata, image_id_for, qhash, remove_book, replace_book, set_series, tidy_apple_double,
};
pub use transfer::{BOOKS_DIR, SentBook, device_file_name};
pub use version::{FirmwareVersion, VersionInfo};
pub use writer::{BACKUPS_KEPT, DeviceWriter, restore};

/// Where macOS mounts removable volumes.
pub const MACOS_VOLUMES: &str = "/Volumes";

/// A Kobo whose storage is mounted on this computer.
#[derive(Debug, Clone)]
pub struct KoboDevice {
    mount: PathBuf,
    version: VersionInfo,
}

impl KoboDevice {
    /// Opens the Kobo mounted at `mount`, identified by `.kobo/version`.
    pub fn open(mount: impl Into<PathBuf>) -> Result<Self> {
        let mount = mount.into();
        let version_path = mount.join(".kobo").join("version");
        if !version_path.is_file() {
            return Err(Error::NotAKobo(mount));
        }
        let contents = fs::read_to_string(&version_path).map_err(|source| Error::Io {
            path: version_path,
            source,
        })?;
        Ok(Self {
            version: VersionInfo::parse(&contents)?,
            mount,
        })
    }

    /// Every Kobo mounted directly under `volumes_root` (normally `/Volumes`).
    pub fn detect(volumes_root: &Path) -> Vec<Self> {
        let Ok(entries) = fs::read_dir(volumes_root) else {
            return Vec::new();
        };
        let mut found: Vec<Self> = entries
            .flatten()
            .filter_map(|e| Self::open(e.path()).ok())
            .collect();
        found.sort_by(|a, b| a.mount.cmp(&b.mount));
        found
    }

    pub fn mount(&self) -> &Path {
        &self.mount
    }

    pub fn version(&self) -> &VersionInfo {
        &self.version
    }

    pub fn db_path(&self) -> PathBuf {
        self.mount.join(".kobo").join("KoboReader.sqlite")
    }

    pub fn open_db(&self) -> Result<KoboDb> {
        KoboDb::open_read_only(&self.db_path())
    }

    pub fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::Kobo,
            model: self.version.model_name().to_owned(),
            firmware: self.version.firmware.to_string(),
            serial: self.version.serial.clone(),
            mount: self.mount.clone(),
            capabilities: Capabilities {
                native_collections: true,
                series: true,
                reading_progress: true,
                reading_time: true,
            },
        }
    }
}
