//! Kindles that connect over MTP (the Scribe since firmware 5.16.3, 2024 and
//! later Paperwhites, the Colorsoft) through mtp-rs, a pure-Rust MTP client.
//!
//! MTP addresses objects by handle, not path, so paths are resolved one folder
//! at a time, as calibre's MTP driver does (`devices/mtp/driver.py`,
//! `ensure_parent`, `find_path`). mtp-rs is async but needs no particular
//! runtime; each call here blocks on it so the rest of the crate stays sync.
//! One connection per device: keep a single [`MtpStorage`] open while the
//! Kindle is connected.

use std::future::Future;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use futures_lite::future::{self, block_on};
use mtp_rs::{MtpDevice, NewObjectInfo, ObjectHandle, ObjectInfo, Storage as MtpStore};

use crate::storage::{Entry, Progress, Storage, split};
use crate::{AMAZON_VENDOR_ID, Error, Result};

/// Upload chunk size.
const CHUNK: usize = 512 * 1024;

/// A Kindle on USB that offers MTP, found without opening it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MtpCandidate {
    pub serial: String,
    pub product_id: u16,
    pub product: Option<String>,
}

/// Kindles offering MTP. Listing doesn't open them, so it never competes
/// with another app for the device.
pub fn candidates() -> Vec<MtpCandidate> {
    MtpDevice::list_devices()
        .unwrap_or_default()
        .into_iter()
        .filter(|d| d.vendor_id == AMAZON_VENDOR_ID)
        .filter_map(|d| {
            Some(MtpCandidate {
                serial: d.serial_number?,
                product_id: d.product_id,
                product: d.product,
            })
        })
        .collect()
}

/// What the device says about itself once open.
#[derive(Debug, Clone, Default)]
pub struct MtpIdentity {
    pub manufacturer: String,
    pub model: String,
    pub serial: String,
    /// Usually the firmware version.
    pub device_version: String,
}

struct Inner {
    device: MtpDevice,
    store: MtpStore,
}

/// An open MTP connection to a Kindle's internal storage.
pub struct MtpStorage {
    inner: Mutex<Inner>,
    identity: MtpIdentity,
}

impl std::fmt::Debug for MtpStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MtpStorage")
            .field("identity", &self.identity)
            .finish_non_exhaustive()
    }
}

fn mtp(e: impl Into<mtp_rs::Error>) -> Error {
    let e = e.into();
    if e.is_exclusive_access() {
        Error::MtpBusy
    } else {
        Error::Mtp(e.to_string())
    }
}

impl MtpStorage {
    /// Opens the Kindle with this USB serial number.
    pub fn open(serial: &str) -> Result<Self> {
        let device = block_on(MtpDevice::builder().open_by_serial(serial)).map_err(mtp)?;
        Self::from_device(device)
    }

    /// Wraps a device that's already open (a virtual one in tests).
    pub fn from_device(device: MtpDevice) -> Result<Self> {
        let info = device.device_info();
        let identity = MtpIdentity {
            manufacturer: info.manufacturer.clone(),
            model: info.model.clone(),
            serial: info.serial_number.clone(),
            device_version: info.device_version.clone(),
        };
        let store = block_on(device.storages())
            .map_err(mtp)?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Mtp("the Kindle reported no storage".to_owned()))?;
        Ok(Self {
            inner: Mutex::new(Inner { device, store }),
            identity,
        })
    }

    pub fn identity(&self) -> &MtpIdentity {
        &self.identity
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Closes the session so the Kindle can be unplugged cleanly.
    pub fn close(self) -> Result<()> {
        let inner = self
            .inner
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        block_on(inner.device.close()).map_err(mtp)
    }
}

impl Inner {
    fn children(&self, parent: Option<ObjectHandle>) -> Result<Vec<ObjectInfo>> {
        block_on(self.store.list_objects(parent)).map_err(mtp)
    }

    fn child(&self, parent: Option<ObjectHandle>, name: &str) -> Result<Option<ObjectInfo>> {
        Ok(self
            .children(parent)?
            .into_iter()
            .find(|o| o.filename == name))
    }

    /// The folder at `dir`: `Ok(None)` is the root, `Err(NotFound)` missing.
    fn folder(&self, dir: &str) -> Result<Option<ObjectHandle>> {
        let mut handle = None;
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            match self.child(handle, part)? {
                Some(o) if o.is_folder() => handle = Some(o.handle),
                _ => return Err(Error::NotFound(dir.to_owned())),
            }
        }
        Ok(handle)
    }

    fn ensure_folder(&self, dir: &str) -> Result<Option<ObjectHandle>> {
        let mut handle = None;
        for part in dir.split('/').filter(|p| !p.is_empty()) {
            handle = Some(match self.child(handle, part)? {
                Some(o) if o.is_folder() => o.handle,
                Some(_) => return Err(Error::NotFound(dir.to_owned())),
                None => block_on(self.store.create_folder(handle, part)).map_err(mtp)?,
            });
        }
        Ok(handle)
    }

    fn file(&self, path: &str) -> Result<Option<ObjectInfo>> {
        let (dir, name) = split(path);
        match self.folder(dir) {
            Ok(parent) => self.child(parent, name),
            Err(Error::NotFound(_)) => Ok(None),
            Err(e) => Err(e),
        }
    }

    /// Deletes an object, emptying folders first: MTP refuses to delete a
    /// folder that has anything in it.
    fn delete(&self, object: &ObjectInfo) -> Result<()> {
        if object.is_folder() {
            for child in self.children(Some(object.handle))? {
                self.delete(&child)?;
            }
        }
        block_on(self.store.delete(object.handle)).map_err(mtp)
    }

    fn upload(
        &self,
        parent: Option<ObjectHandle>,
        name: &str,
        data: &[u8],
        progress: Progress,
    ) -> Result<ObjectHandle> {
        let total = data.len() as u64;
        let sent = Arc::new(AtomicU64::new(0));
        let counter = sent.clone();
        let chunks: Vec<std::io::Result<bytes::Bytes>> = data
            .chunks(CHUNK)
            .map(|c| Ok(bytes::Bytes::copy_from_slice(c)))
            .collect();
        let mut upload = std::pin::pin!(self.store.upload_with_progress(
            parent,
            NewObjectInfo::file(name, total),
            futures_lite::stream::iter(chunks),
            move |p: mtp_rs::Progress| {
                counter.store(p.bytes_transferred, Ordering::Relaxed);
                ControlFlow::Continue(())
            },
        ));
        // The progress callback must be Send and this one isn't, so report
        // from here each time the upload wakes.
        let result = block_on(future::poll_fn(|cx| {
            let poll = upload.as_mut().poll(cx);
            progress(sent.load(Ordering::Relaxed).min(total), total);
            poll
        }));
        match result {
            Ok(handle) => {
                progress(total, total);
                Ok(handle)
            }
            Err(e) => {
                if let Some(partial) = e.partial {
                    let _ = block_on(self.store.delete(partial));
                }
                Err(mtp(e))
            }
        }
    }
}

impl Storage for MtpStorage {
    fn list(&self, dir: &str) -> Result<Vec<Entry>> {
        let inner = self.lock();
        let parent = match inner.folder(dir) {
            Ok(parent) => parent,
            Err(Error::NotFound(_)) => return Ok(Vec::new()),
            Err(e) => return Err(e),
        };
        let mut entries: Vec<Entry> = inner
            .children(parent)?
            .into_iter()
            .map(|o| Entry {
                is_dir: o.is_folder(),
                size: o.size,
                modified: o.modified.and_then(|t| t.to_unix_seconds()),
                name: o.filename,
            })
            .collect();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(entries)
    }

    fn size(&self, path: &str) -> Result<Option<u64>> {
        Ok(self
            .lock()
            .file(path)?
            .filter(|o| !o.is_folder())
            .map(|o| o.size))
    }

    fn is_dir(&self, path: &str) -> Result<bool> {
        Ok(self.lock().file(path)?.is_some_and(|o| o.is_folder()))
    }

    fn read(&self, path: &str) -> Result<Vec<u8>> {
        let inner = self.lock();
        let object = inner
            .file(path)?
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        block_on(inner.store.download_to_vec(object.handle)).map_err(mtp)
    }

    fn read_prefix(&self, path: &str, len: u64) -> Result<Vec<u8>> {
        let inner = self.lock();
        let object = inner
            .file(path)?
            .ok_or_else(|| Error::NotFound(path.to_owned()))?;
        let len = len.min(object.size);
        let len = u32::try_from(len).unwrap_or(u32::MAX);
        if len == 0 {
            return Ok(Vec::new());
        }
        block_on(inner.store.read_range(object.handle, 0, len)).map_err(mtp)
    }

    fn write(&self, path: &str, data: &[u8], progress: Progress) -> Result<()> {
        let inner = self.lock();
        let (dir, name) = split(path);
        let parent = inner.ensure_folder(dir)?;
        let existing = inner.child(parent, name)?;
        let Some(existing) = existing else {
            inner.upload(parent, name, data, progress)?;
            return Ok(());
        };
        // MTP can't replace in place. Upload beside the old file and swap
        // them by rename when the device can, so an interrupted write keeps
        // the old copy; otherwise delete first, as calibre does.
        if inner.device.supports_rename() {
            let part = format!(".{name}.parallax-part");
            if let Some(stale) = inner.child(parent, &part)? {
                inner.delete(&stale)?;
            }
            let handle = inner.upload(parent, &part, data, progress)?;
            inner.delete(&existing)?;
            block_on(inner.store.rename(handle, name)).map_err(mtp)
        } else {
            inner.delete(&existing)?;
            inner.upload(parent, name, data, progress).map(|_| ())
        }
    }

    fn remove(&self, path: &str) -> Result<()> {
        let inner = self.lock();
        match inner.file(path)? {
            Some(object) => inner.delete(&object),
            None => Ok(()),
        }
    }

    fn create_dir_all(&self, path: &str) -> Result<()> {
        self.lock().ensure_folder(path).map(|_| ())
    }

    fn free_space(&self) -> Option<u64> {
        let mut inner = self.lock();
        block_on(inner.store.refresh()).ok()?;
        Some(inner.store.info().free_space)
    }
}
