//! Collection management and the follow-ups to sending: writing collection
//! memberships for just-sent books, applying series once Nickel has imported
//! them, and turning device folders into collections.

use std::collections::BTreeMap;
use std::path::Path;

use erc_device::DeviceBook;
use erc_device_kobo::{
    BOOKS_DIR, KoboDevice, add_to_collection, create_collection, delete_collection,
    remove_from_collection, rename_collection, set_series,
};
use erc_library::Library;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CollectionChange {
    Create {
        name: String,
    },
    Rename {
        from: String,
        to: String,
    },
    Delete {
        name: String,
    },
    Add {
        name: String,
        content_ids: Vec<String>,
    },
    Remove {
        name: String,
        content_ids: Vec<String>,
    },
}

/// Applies changes in order, in one backed-up write session and one
/// transaction: either all of them land or none do.
pub fn apply(
    device: &KoboDevice,
    backup_root: &Path,
    changes: &[CollectionChange],
) -> erc_device_kobo::Result<()> {
    let mut writer = device.open_writer(backup_root)?;
    let result = writer.transaction(|tx| {
        for change in changes {
            match change {
                CollectionChange::Create { name } => create_collection(tx, name)?,
                CollectionChange::Rename { from, to } => rename_collection(tx, from, to)?,
                CollectionChange::Delete { name } => delete_collection(tx, name)?,
                CollectionChange::Add { name, content_ids } => {
                    add_to_collection(tx, name, content_ids)?;
                }
                CollectionChange::Remove { name, content_ids } => {
                    remove_from_collection(tx, name, content_ids)?;
                }
            }
        }
        Ok(())
    });
    writer.close()?;
    result
}

/// Books grouped by the folder they sit in on the device
/// (`KEpub/<Folder>/…`, or `<Folder>/…` outside `KEpub`).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FolderGroup {
    pub folder: String,
    pub content_ids: Vec<String>,
}

pub fn folder_groups(books: &[DeviceBook]) -> Vec<FolderGroup> {
    let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for book in books {
        let Some(path) = &book.path else { continue };
        let mut parts: Vec<&str> = path.split('/').collect();
        parts.pop(); // file name
        if parts.first() == Some(&BOOKS_DIR) {
            parts.remove(0);
        }
        if let Some(folder) = parts.first().filter(|f| !f.is_empty()) {
            groups
                .entry((*folder).to_owned())
                .or_default()
                .push(book.id.clone());
        }
    }
    groups
        .into_iter()
        .map(|(folder, content_ids)| FolderGroup {
            folder,
            content_ids,
        })
        .collect()
}

/// Library books on this device whose series Nickel doesn't show yet
/// (Nickel ignores series in sideloaded EPUBs, and the database row only
/// exists after import, so series are applied on the next connection).
#[derive(Debug, Clone, Serialize)]
pub struct PendingSeries {
    pub content_id: String,
    pub series: String,
    pub series_index: Option<String>,
}

pub fn pending_series(
    library: &Library,
    device: &KoboDevice,
) -> erc_device_kobo::Result<Vec<PendingSeries>> {
    let copies = library
        .device_copies(&device.version().serial)
        .unwrap_or_default();
    if copies.is_empty() {
        return Ok(Vec::new());
    }
    let shown: BTreeMap<String, (Option<String>, Option<String>)> = device
        .open_db()?
        .books()?
        .into_iter()
        .map(|b| (b.id, (b.series, b.series_number)))
        .collect();
    Ok(copies
        .into_iter()
        .filter_map(|copy| {
            let book = library.book(copy.book_id).ok()?;
            let series = book.series?;
            let (on_device, number) = shown.get(&copy.content_id)?; // not imported yet
            let index = book.series_index;
            let same = on_device.as_deref() == Some(series.as_str())
                && number.as_deref().map(normalise_index) == index.as_deref().map(normalise_index);
            (!same).then_some(PendingSeries {
                content_id: copy.content_id,
                series,
                series_index: index,
            })
        })
        .collect())
}

/// "1.0" and "1" are the same position.
fn normalise_index(s: &str) -> String {
    s.parse::<f64>()
        .map_or_else(|_| s.trim().to_owned(), |n| n.to_string())
}

/// Writes pending series to the device. No write session is opened when
/// nothing is pending. Returns how many books were updated.
pub fn reconcile_series(
    library: &Library,
    device: &KoboDevice,
    backup_root: &Path,
) -> erc_device_kobo::Result<usize> {
    let pending = pending_series(library, device)?;
    if pending.is_empty() {
        return Ok(0);
    }
    let mut writer = device.open_writer(backup_root)?;
    let updated = writer.transaction(|tx| {
        let mut n = 0;
        for p in &pending {
            if set_series(
                tx,
                &p.content_id,
                Some(&p.series),
                p.series_index.as_deref(),
            )? {
                n += 1;
            }
        }
        Ok(n)
    });
    writer.close()?;
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(path: &str) -> DeviceBook {
        DeviceBook {
            id: format!("file:///mnt/onboard/{path}"),
            title: path.into(),
            author: None,
            path: Some(path.into()),
            sideloaded: true,
            series: None,
            series_number: None,
            read_status: erc_device::ReadStatus::Unread,
            percent_read: 0,
            last_read: None,
            reading_time_secs: None,
            added: None,
            file_size: None,
            collections: vec![],
        }
    }

    #[test]
    fn groups_by_folder() {
        let books = [
            book("KEpub/Fantasy/a.kepub.epub"),
            book("KEpub/Fantasy/Sub/b.kepub.epub"),
            book("KEpub/Horror/c.kepub.epub"),
            book("KEpub/loose.kepub.epub"),
            book("Comics/d.epub"),
            book("root.epub"),
        ];
        let groups = folder_groups(&books);
        let summary: Vec<(&str, usize)> = groups
            .iter()
            .map(|g| (g.folder.as_str(), g.content_ids.len()))
            .collect();
        assert_eq!(summary, [("Comics", 1), ("Fantasy", 2), ("Horror", 1)]);
    }

    #[test]
    fn index_normalisation() {
        assert_eq!(normalise_index("1.0"), normalise_index("1"));
        assert_ne!(normalise_index("1.5"), normalise_index("1"));
    }
}
