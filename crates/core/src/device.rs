//! Library-aware changes to books already on a device.

use std::path::Path;

use erc_device_kobo::{KoboDevice, remove_book};
use erc_library::Library;
use serde::Serialize;

/// A library book whose copy is on the connected device.
#[derive(Debug, Clone, Serialize)]
pub struct OnDevice {
    pub book_id: i64,
    pub device_path: String,
    pub content_id: String,
    pub sent_at: String,
    /// The book was edited (metadata or cover) after it was sent.
    pub stale: bool,
}

pub fn books_on_device(
    library: &Library,
    device: &KoboDevice,
) -> erc_library::Result<Vec<OnDevice>> {
    let copies = library.device_copies(&device.version().serial)?;
    Ok(copies
        .into_iter()
        .filter(|c| device.mount().join(&c.device_path).is_file())
        .filter_map(|c| {
            let book = library.book(c.book_id).ok()?;
            Some(OnDevice {
                stale: book.modified_at > c.sent_at,
                book_id: c.book_id,
                device_path: c.device_path,
                content_id: c.content_id,
                sent_at: c.sent_at,
            })
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct RemoveOutcome {
    pub content_id: String,
    pub result: Result<(), String>,
}

/// Removes books from the device in one backed-up write session and forgets
/// the library's record of them. Fails as a whole only if the session can't
/// start (e.g. the database isn't safe to write); otherwise per book.
pub fn remove_from_device(
    library: Option<&Library>,
    device: &KoboDevice,
    backup_root: &Path,
    content_ids: &[String],
) -> erc_device_kobo::Result<Vec<RemoveOutcome>> {
    let serial = device.version().serial.clone();
    let copies = library
        .and_then(|l| l.device_copies(&serial).ok())
        .unwrap_or_default();
    let mut writer = device.open_writer(backup_root)?;
    let outcomes = content_ids
        .iter()
        .map(|content_id| {
            let result = remove_book(&mut writer, content_id).map_err(|e| e.to_string());
            if result.is_ok()
                && let (Some(lib), Some(copy)) =
                    (library, copies.iter().find(|c| &c.content_id == content_id))
            {
                let _ = lib.remove_device_copy(copy.book_id, &serial);
            }
            RemoveOutcome {
                content_id: content_id.clone(),
                result,
            }
        })
        .collect();
    writer.close()?;
    Ok(outcomes)
}
