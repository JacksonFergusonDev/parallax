//! Collections on older Kindle firmware, read from `system/collections.json`
//! as calibre does (`devices/kindle/driver.py`, `kindle_update_booklist`).
//! The file is a cache the Kindle regenerates, so it's read-only. Firmware 5
//! and later keep collections in a database USB can't reach, so this is
//! usually absent.

use std::collections::{BTreeMap, BTreeSet};

use crate::storage::Storage;

pub const COLLECTIONS_FILE: &str = "system/collections.json";

/// Collection names for each book path ("documents/…"), when the file exists.
pub fn read(storage: &dyn Storage) -> BTreeMap<String, BTreeSet<String>> {
    let Ok(data) = storage.read(COLLECTIONS_FILE) else {
        return BTreeMap::new();
    };
    parse(&data)
}

/// Keyed by the SHA-1 of `/mnt/us/<path>`, which [`for_path`] looks up.
pub fn parse(data: &[u8]) -> BTreeMap<String, BTreeSet<String>> {
    let Ok(serde_json::Value::Object(map)) = serde_json::from_slice(data) else {
        return BTreeMap::new();
    };
    let mut by_hash: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (name, value) in map {
        let collection = name.split('@').next().unwrap_or(&name).to_owned();
        let items = value.get("items").and_then(|i| i.as_array());
        for item in items.into_iter().flatten().filter_map(|i| i.as_str()) {
            // The hash may carry a prefix; calibre uses the last 40 characters.
            let start = item.len().saturating_sub(40);
            by_hash
                .entry(item[start..].to_ascii_lowercase())
                .or_default()
                .insert(collection.clone());
        }
    }
    by_hash
}

/// A book's collections from [`parse`]'s result.
pub fn for_path(by_hash: &BTreeMap<String, BTreeSet<String>>, path: &str) -> Vec<String> {
    let hash = sha1_smol::Sha1::from(format!("/mnt/us/{}", path.trim_matches('/')))
        .digest()
        .to_string();
    by_hash
        .get(&hash)
        .map_or_default(|names| names.iter().cloned().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_collections() {
        let hash = sha1_smol::Sha1::from("/mnt/us/documents/Book.azw3")
            .digest()
            .to_string();
        let json = format!(
            r##"{{"Stoics@en-US": {{"items": ["*{hash}", "#B000FA5ZEG^EBOK"], "lastAccess": 1}}, "Bad": 3}}"##
        );
        let parsed = parse(json.as_bytes());
        assert_eq!(for_path(&parsed, "documents/Book.azw3"), ["Stoics"]);
        assert!(for_path(&parsed, "documents/Other.azw3").is_empty());
        assert!(parse(b"not json").is_empty());
    }
}
