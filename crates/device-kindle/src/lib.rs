//! Kindle e-reader backend.
//!
//! Kindles before 2024 appear as a drive; the Scribe (firmware 5.16.3 and
//! later), 2024 and later Paperwhites and the Colorsoft use MTP instead. Both
//! go through [`storage::Storage`], so everything else works the same on
//! either. Much of this is ported from calibre's Kindle and MTP drivers,
//! cited where it's used.
//!
//! Unlike a Kobo, a Kindle has no database Parallax can reach: collections
//! and reading status live in `cc.db` on the system partition. So this
//! backend only adds and removes files, and reads progress and annotations
//! from sidecar files.

pub mod apnx;
pub mod books;
pub mod clippings;
pub mod collections;
pub mod detect;
pub mod krds;
pub mod mobi;
pub mod models;
pub mod mtp;
pub mod storage;
pub mod thumbnails;
pub mod transfer;

use std::path::PathBuf;
use std::sync::Arc;

use parallax_device::{Capabilities, DeviceInfo, DeviceKind, Transport};

pub use books::{KindleBook, MetaCache};
pub use clippings::Clipping;
pub use detect::{Connections, Unavailable};
pub use storage::Storage;
pub use transfer::{Outgoing, Sent};

/// Amazon's USB vendor id.
pub const AMAZON_VENDOR_ID: u16 = 0x1949;
/// Where Kindles keep books.
pub const DOCUMENTS_DIR: &str = "documents";
/// The highlights and notes file.
pub const CLIPPINGS_FILE: &str = "documents/My Clippings.txt";
/// Prefix of the key that stands in for an MTP Kindle's mount point.
pub const MTP_PREFIX: &str = "mtp:";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0} isn't on the Kindle")]
    NotFound(String),
    #[error("{0} isn't a connected Kindle")]
    NotAKindle(String),
    #[error("{}", detect::BUSY)]
    MtpBusy,
    #[error("the Kindle didn't respond as expected: {0}")]
    Mtp(String),
    #[error("the Kindle's files changed while Parallax was reading them; try again")]
    MtpStale,
    #[error("couldn't make a cover thumbnail: {0}")]
    Image(String),
    #[error("not enough space on the Kindle: needs {needed} bytes, {free} free")]
    NoSpace { needed: u64, free: u64 },
}

pub type Result<T> = std::result::Result<T, Error>;

/// A connected Kindle.
#[derive(Clone)]
pub struct KindleDevice {
    storage: Arc<dyn Storage>,
    key: String,
    serial: String,
    model: models::Model,
    model_name: String,
    firmware: String,
    transport: Transport,
}

impl std::fmt::Debug for KindleDevice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KindleDevice")
            .field("key", &self.key)
            .field("model", &self.model_name)
            .field("transport", &self.transport)
            .finish_non_exhaustive()
    }
}

impl KindleDevice {
    /// `key` is the mount point, or `mtp:<serial>` for an MTP Kindle.
    /// `product` is what USB or MTP calls the device.
    pub fn new(
        storage: Arc<dyn Storage>,
        key: String,
        serial: String,
        firmware: String,
        transport: Transport,
        product: Option<&str>,
    ) -> Self {
        let (model, model_name) = detect::model_for(&serial, product);
        Self {
            storage,
            key,
            serial,
            model,
            model_name,
            firmware,
            transport,
        }
    }

    pub fn storage(&self) -> &dyn Storage {
        self.storage.as_ref()
    }

    /// The mount point, or `mtp:<serial>`.
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn serial(&self) -> &str {
        &self.serial
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn transport(&self) -> Transport {
        self.transport
    }

    pub fn info(&self) -> DeviceInfo {
        DeviceInfo {
            kind: DeviceKind::Kindle,
            model: self.model_name.clone(),
            firmware: self.firmware.clone(),
            serial: self.serial.clone(),
            mount: PathBuf::from(&self.key),
            transport: self.transport,
            capabilities: Capabilities {
                native_collections: false,
                series: false,
                reading_progress: true,
                reading_time: true,
            },
            screen: self.model.screen(),
            read_only: None,
            tested: models::TESTED_MODELS.contains(&self.model_name.as_str()),
            unavailable: None,
        }
    }

    /// Every book in `documents/`.
    pub fn books(&self, cache: &mut MetaCache) -> Result<Vec<KindleBook>> {
        books::scan(self.storage(), cache)
    }

    /// Entries in `documents/My Clippings.txt`.
    pub fn clippings(&self) -> Result<Vec<Clipping>> {
        match self.storage.read(CLIPPINGS_FILE) {
            Ok(data) => Ok(clippings::parse(&String::from_utf8_lossy(&data))),
            Err(Error::NotFound(_)) => Ok(Vec::new()),
            Err(Error::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                Ok(Vec::new())
            }
            Err(e) => Err(e),
        }
    }

    /// Puts back cover thumbnails Amazon replaced. Returns how many.
    pub fn restore_thumbnails(&self) -> Result<usize> {
        thumbnails::restore(self.storage())
    }

    pub fn send(&self, book: &Outgoing, progress: storage::Progress) -> Result<Sent> {
        transfer::send(self.storage(), book, progress)
    }

    pub fn remove(&self, path: &str) -> Result<()> {
        transfer::remove(self.storage(), path)
    }

    /// Reads a whole file, such as a book to add to the library.
    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        self.storage.read(path)
    }

    /// The cached thumbnail Parallax or calibre wrote for a book, or the
    /// Kindle's own, as JPEG.
    pub fn thumbnail(&self, book: &KindleBook) -> Option<Vec<u8>> {
        let name =
            thumbnails::recorded(self.storage(), &book.path).or_else(|| book.thumbnail_name())?;
        self.storage
            .read(&storage::join(&[thumbnails::THUMBNAILS_DIR, &name]))
            .or_else(|_| {
                self.storage
                    .read(&storage::join(&[thumbnails::CACHE_DIR, &name]))
            })
            .ok()
    }

    /// Unmounts a drive-mode Kindle. MTP Kindles are ejected through
    /// [`Connections::eject`].
    pub fn eject(&self) -> Result<()> {
        if self.transport == Transport::Mtp {
            return Ok(());
        }
        let status = std::process::Command::new("/usr/sbin/diskutil")
            .arg("eject")
            .arg(&self.key)
            .status()
            .map_err(|source| Error::Io {
                path: PathBuf::from(&self.key),
                source,
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::Io {
                path: PathBuf::from(&self.key),
                source: std::io::Error::other(
                    "diskutil couldn't eject it; close any files open on it",
                ),
            })
        }
    }
}
