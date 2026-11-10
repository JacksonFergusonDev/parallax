//! Kobo e-reader backend.

mod annotations;
mod backups;
mod collections;
mod covers;
mod db;
mod error;
mod maintenance;
mod ops;
mod report;
mod screen;
mod state;
mod stats;
mod support;
mod transfer;
mod version;
mod writer;

use std::fs;
use std::path::{Path, PathBuf};

use parallax_device::{Capabilities, DeviceInfo, DeviceKind};

pub use annotations::{Annotation, AnnotationKind};
pub use backups::{
    BACKUPS_KEPT, BackupInfo, BackupPolicy, DAILY_BACKUPS_KEPT, DEFAULT_MAX_BYTES, Keep,
    REMOVAL_BACKUPS_KEPT, REMOVED_KEPT_DAYS, RestoreOutcome, backups_size, delete_all_backups,
};
pub use collections::{
    add_to_collection, create_collection, delete_collection, remove_from_collection,
    rename_collection,
};
pub use db::{
    KoboDb, MAX_TESTED_DB_VERSION, MIN_SUPPORTED_DB_VERSION, ONBOARD_PREFIX, parse_kobo_date,
};
pub use error::{Error, Result};
pub use maintenance::{
    DuplicateCollection, Mismatch, duplicate_collections, files_not_in_database,
    merge_duplicate_collections, missing_files, orphaned_images, remove_orphaned_images,
    set_read_status,
};
pub use ops::{
    DisplayMetadata, image_id_for, qhash, remove_book, remove_book_with_progress, replace_book,
    replace_book_with_progress, set_series, tidy_apple_double,
};
pub use report::{CoverFiles, DeviceReport, REPORT_FORMAT};
pub use screen::ASSUMED_SCREEN;
pub use state::{
    BookChange, BookCheck, DeviceRecord, DeviceState, LOG_KEPT, LogEntry, ManagedBook,
    PARALLAX_VERSION, Record, STATE_DIR, Stamp,
};
pub use stats::FinishedEvent;
pub use support::{MAX_WRITE_FIRMWARE_MAJOR, Support};
pub use transfer::{BOOKS_DIR, SentBook, device_file_name};
pub use version::{FirmwareVersion, VersionInfo};
pub use writer::{DeviceWriter, restore};

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

    /// Identity and capabilities. Reads the database to decide what this
    /// Kobo supports ([`KoboDevice::support`]).
    pub fn info(&self) -> DeviceInfo {
        let support = self.support();
        DeviceInfo {
            kind: DeviceKind::Kobo,
            model: self.version.model_name().to_owned(),
            firmware: self.version.firmware.to_string(),
            serial: self.version.serial.clone(),
            mount: self.mount.clone(),
            capabilities: Capabilities {
                native_collections: true,
                series: support.series,
                reading_progress: true,
                reading_time: support.reading_time,
            },
            screen: self.version.screen(&self.mount),
            read_only: support.read_only,
            tested: self.version.is_tested(),
        }
    }
}
