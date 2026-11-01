//! Types shared by every e-reader backend. Backends (Kobo now, Kindle later)
//! describe what they can do through [`Capabilities`] so the UI can tell
//! on-device changes apart from local-only organisation without checking the
//! device type.

use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    Kobo,
}

/// What a connected device supports natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Capabilities {
    /// Collections the user sees on the device itself.
    pub native_collections: bool,
    /// Series name and number shown on the device.
    pub series: bool,
    /// Read status, percentage and last-read date.
    pub reading_progress: bool,
    /// Accumulated reading time per book.
    pub reading_time: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub kind: DeviceKind,
    /// Human-readable model, e.g. "Kobo Libra 2".
    pub model: String,
    pub firmware: String,
    pub serial: String,
    /// Where the device's storage is mounted on this computer.
    pub mount: PathBuf,
    pub capabilities: Capabilities,
    /// Why the app won't change this device (an untested model, firmware or
    /// database); `None` when it will. Reading still works.
    pub read_only: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadStatus {
    Unread,
    Reading,
    Finished,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceBook {
    /// Stable device-side identifier (Kobo: `ContentID`).
    pub id: String,
    pub title: String,
    pub author: Option<String>,
    /// Path relative to the mount point for sideloaded books; `None` for
    /// store books whose files the user can't manage directly.
    pub path: Option<String>,
    pub sideloaded: bool,
    pub series: Option<String>,
    pub series_number: Option<String>,
    pub read_status: ReadStatus,
    /// 0–100.
    pub percent_read: u8,
    pub last_read: Option<DateTime<Utc>>,
    pub reading_time_secs: Option<u64>,
    /// When the book arrived on the device.
    pub added: Option<DateTime<Utc>>,
    pub file_size: Option<u64>,
    pub collections: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Collection {
    pub name: String,
    pub book_count: usize,
}
