//! Finding Kindles: as a drive under `/Volumes`, or over MTP.
//!
//! calibre recognises a drive-mode Kindle by its USB ids (vendor 0x1949) and
//! an MTP one by its MTP interface (`devices/kindle/driver.py`,
//! `devices/mtp/driver.py`). Here a drive counts as a Kindle when it has the
//! Kindle's `documents` and `system` folders and is named Kindle or an
//! Amazon USB device is connected. Its serial number comes from USB.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

use nusb::MaybeFuture;
use parallax_device::Transport;

use crate::mtp::{self, MtpStorage};
use crate::storage::{DriveStorage, Storage};
use crate::{AMAZON_VENDOR_ID, Error, KindleDevice, MTP_PREFIX, Result, models};

/// An Amazon device on USB.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbKindle {
    pub serial: Option<String>,
    pub product_id: u16,
    pub product: Option<String>,
}

/// Amazon devices on USB, without opening any of them.
pub fn usb_kindles() -> Vec<UsbKindle> {
    let Ok(devices) = nusb::list_devices().wait() else {
        return Vec::new();
    };
    devices
        .filter(|d| d.vendor_id() == AMAZON_VENDOR_ID)
        .map(|d| UsbKindle {
            serial: d.serial_number().map(str::to_owned),
            product_id: d.product_id(),
            product: d.product_string().map(str::to_owned),
        })
        .collect()
}

/// Whether `root` looks like a Kindle's storage. `amazon_on_usb` says an
/// Amazon USB device is connected, for drives not named Kindle.
pub fn is_kindle_drive(root: &Path, amazon_on_usb: bool) -> bool {
    let named = root
        .file_name()
        .is_some_and(|n| n.to_string_lossy().to_lowercase().contains("kindle"));
    root.join("documents").is_dir() && root.join("system").is_dir() && (named || amazon_on_usb)
}

/// The firmware a drive-mode Kindle reports, when it says.
fn drive_firmware(storage: &DriveStorage) -> String {
    storage
        .read("system/version.txt")
        .ok()
        .and_then(|v| {
            String::from_utf8_lossy(&v)
                .lines()
                .next()
                .map(|l| l.trim().to_owned())
        })
        .unwrap_or_default()
}

/// Kindles mounted directly under `volumes_root` (normally `/Volumes`).
pub fn drives(volumes_root: &Path) -> Vec<KindleDevice> {
    let Ok(entries) = fs::read_dir(volumes_root) else {
        return Vec::new();
    };
    let usb = usb_kindles();
    let mtp_serials: HashSet<String> = mtp::candidates().into_iter().map(|c| c.serial).collect();
    // A drive-mode Kindle is the Amazon device that isn't offering MTP.
    let drive_usb: Vec<&UsbKindle> = usb
        .iter()
        .filter(|u| u.serial.as_ref().is_none_or(|s| !mtp_serials.contains(s)))
        .collect();
    let mut roots: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| is_kindle_drive(p, !drive_usb.is_empty()))
        .collect();
    roots.sort();
    let only_usb = (roots.len() == 1 && drive_usb.len() == 1).then(|| drive_usb[0]);
    roots
        .into_iter()
        .map(|root| {
            let storage = DriveStorage::new(&root);
            let serial = only_usb.and_then(|u| u.serial.clone()).unwrap_or_else(|| {
                // No USB serial to go on: name it after the volume, which
                // is stable for one Kindle.
                format!(
                    "kindle-{}",
                    root.file_name().unwrap_or_default().to_string_lossy()
                )
            });
            let firmware = drive_firmware(&storage);
            KindleDevice::new(
                Arc::new(storage),
                root.to_string_lossy().into_owned(),
                serial,
                firmware,
                Transport::Drive,
                only_usb.and_then(|u| u.product.as_deref()),
            )
        })
        .collect()
}

/// Opens a drive-mode Kindle at `root`, as [`drives`] found it.
pub fn open_drive(root: &Path) -> Result<KindleDevice> {
    if !is_kindle_drive(root, true) {
        return Err(Error::NotAKindle(root.display().to_string()));
    }
    drives(root.parent().unwrap_or(root))
        .into_iter()
        .find(|k| Path::new(k.key()) == root)
        .ok_or_else(|| Error::NotAKindle(root.display().to_string()))
}

/// Why an MTP Kindle couldn't be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unavailable {
    pub serial: String,
    pub product: Option<String>,
    pub reason: String,
}

/// MTP connections to Kindles. MTP allows one connection per device, so the
/// app keeps them here while the Kindles stay plugged in.
#[derive(Default)]
pub struct Connections {
    open: Mutex<HashMap<String, Arc<MtpStorage>>>,
    /// Ejected but still plugged in: not reopened until unplugged.
    ejected: Mutex<HashSet<String>>,
}

/// The message for a Kindle another program is holding.
pub const BUSY: &str = "Another app is using your Kindle. Quit Amazon's Send to Kindle, Android File Transfer or OpenMTP, then unplug your Kindle and plug it in again.";
/// The same when macOS's photo import service holds it and Parallax may not stop it.
pub const BUSY_CAMERA: &str = "macOS's photo import service is using your Kindle. Turn on Settings › Kindle › Free your Kindle from photo import, or quit other apps that use your Kindle, then plug it in again.";

impl Connections {
    fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Opens newly connected MTP Kindles and forgets unplugged ones. When
    /// `stop_camera_daemon` is set and macOS's `ptpcamerad` has claimed a
    /// Kindle, stops it so Parallax can open the Kindle.
    pub fn refresh(&self, stop_camera_daemon: bool) -> (Vec<KindleDevice>, Vec<Unavailable>) {
        let candidates = mtp::candidates();
        let present: HashSet<&str> = candidates.iter().map(|c| c.serial.as_str()).collect();
        Self::lock(&self.open).retain(|serial, _| present.contains(serial.as_str()));
        Self::lock(&self.ejected).retain(|serial| present.contains(serial.as_str()));
        let mut devices = Vec::new();
        let mut unavailable = Vec::new();
        for candidate in &candidates {
            if Self::lock(&self.ejected).contains(&candidate.serial) {
                continue;
            }
            let existing = Self::lock(&self.open).get(&candidate.serial).cloned();
            let storage = match existing {
                Some(storage) => storage,
                None => match open_mtp(&candidate.serial, stop_camera_daemon) {
                    Ok(storage) => {
                        let storage = Arc::new(storage);
                        Self::lock(&self.open).insert(candidate.serial.clone(), storage.clone());
                        storage
                    }
                    Err(e) => {
                        unavailable.push(Unavailable {
                            serial: candidate.serial.clone(),
                            product: candidate.product.clone(),
                            reason: match e {
                                Error::MtpBusy
                                    if stop_camera_daemon || !cfg!(target_os = "macos") =>
                                {
                                    BUSY.to_owned()
                                }
                                Error::MtpBusy => BUSY_CAMERA.to_owned(),
                                other => other.to_string(),
                            },
                        });
                        continue;
                    }
                },
            };
            devices.push(mtp_device(storage));
        }
        (devices, unavailable)
    }

    /// The open Kindle with this key (`mtp:<serial>`), without touching USB.
    pub fn get(&self, key: &str) -> Option<KindleDevice> {
        let serial = key.strip_prefix(MTP_PREFIX)?;
        Self::lock(&self.open).get(serial).cloned().map(mtp_device)
    }

    /// Closes the connection so the Kindle can be unplugged, and leaves it
    /// closed until it is.
    pub fn eject(&self, key: &str) -> Result<()> {
        let serial = key
            .strip_prefix(MTP_PREFIX)
            .ok_or_else(|| Error::NotAKindle(key.to_owned()))?;
        Self::lock(&self.ejected).insert(serial.to_owned());
        if let Some(storage) = Self::lock(&self.open).remove(serial) {
            // Other holders finish their operation first; the session closes
            // when the last one lets go.
            if let Ok(storage) = Arc::try_unwrap(storage) {
                storage.close()?;
            }
        }
        Ok(())
    }
}

fn mtp_device(storage: Arc<MtpStorage>) -> KindleDevice {
    let identity = storage.identity().clone();
    let product = (!identity.model.is_empty()).then(|| identity.model.clone());
    KindleDevice::new(
        storage,
        format!("{MTP_PREFIX}{}", identity.serial),
        identity.serial,
        identity.device_version,
        Transport::Mtp,
        product.as_deref(),
    )
}

fn open_mtp(serial: &str, stop_camera_daemon: bool) -> Result<MtpStorage> {
    match MtpStorage::open(serial) {
        Err(Error::MtpBusy) if stop_camera_daemon && cfg!(target_os = "macos") => {
            // ptpcamerad claims MTP devices as they connect, and launchd
            // restarts it on demand, so stop it and open straight away; mtp-rs
            // documents the same workaround.
            for _ in 0..10 {
                stop_ptpcamerad();
                std::thread::sleep(std::time::Duration::from_millis(150));
                match MtpStorage::open(serial) {
                    Err(Error::MtpBusy) => {}
                    other => return other,
                }
            }
            Err(Error::MtpBusy)
        }
        other => other,
    }
}

/// Stops macOS's photo import daemon, which runs as the user. It restarts the
/// next time something needs it.
fn stop_ptpcamerad() {
    let _ = std::process::Command::new("/usr/bin/pkill")
        .args(["-x", "ptpcamerad"])
        .status();
}

/// The model to show for a Kindle, from its serial or what USB calls it.
pub fn model_for(serial: &str, product: Option<&str>) -> (models::Model, String) {
    if let Some(model) = models::from_serial(serial) {
        return (model, model.name.to_owned());
    }
    let name = product
        .filter(|p| p.to_lowercase().contains("kindle"))
        .unwrap_or("Kindle")
        .to_owned();
    (models::UNKNOWN, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_kindle_drives() {
        let dir = tempfile::tempdir().unwrap();
        let kindle = dir.path().join("Kindle");
        fs::create_dir_all(kindle.join("documents")).unwrap();
        fs::create_dir_all(kindle.join("system")).unwrap();
        let other = dir.path().join("BACKUP");
        fs::create_dir_all(other.join("documents")).unwrap();
        fs::create_dir_all(other.join("system")).unwrap();
        let usb = dir.path().join("USB");
        fs::create_dir_all(usb.join("documents")).unwrap();

        assert!(is_kindle_drive(&kindle, false));
        assert!(!is_kindle_drive(&other, false));
        assert!(is_kindle_drive(&other, true));
        assert!(!is_kindle_drive(&usb, true));
    }

    #[test]
    fn models_fall_back_to_usb_name() {
        assert_eq!(model_for("G003H20000000000", None).1, "Kindle Colorsoft");
        assert_eq!(
            model_for("XYZ", Some("Kindle Paperwhite Signature Edition")).1,
            "Kindle Paperwhite Signature Edition"
        );
        assert_eq!(model_for("XYZ", Some("MTP Device")).1, "Kindle");
    }
}
