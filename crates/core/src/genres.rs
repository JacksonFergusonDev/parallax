//! Standard genre names across the library and the Kobo.
//!
//! Genres are tags whose name is one of `erc_metadata::genres`' standard
//! names. "Tidy genres" finds tags and Kobo collections that are a genre
//! written another way ("sci-fi", "Non-Fiction", "Thrillers") and renames them
//! to the standard name, merging into it where it already exists. Tags that
//! only mention a genre ("Space Opera") are offered too, but not chosen by
//! default. Folders on the Kobo aren't moved; they're read as genres instead.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use erc_device_kobo::KoboDevice;
use erc_library::Library;
use erc_metadata::genres;
use serde::{Deserialize, Serialize};

use crate::collections::{CollectionChange, apply};

/// The genre a book's folder names, by its standard name:
/// `KEpub/Sci-Fi/x.kepub.epub` → "Science Fiction". `None` for other
/// folders ("To read", "KEpub").
pub fn folder_genre(path: &str) -> Option<&'static str> {
    let mut parts: Vec<&str> = path.split('/').collect();
    parts.pop();
    let folder = *parts.last()?;
    genres::canonical(folder).or_else(|| genres::from_label(folder).map(|g| g.name))
}

/// `tags` with every genre under its standard name, duplicates removed
/// (ignoring case), order kept.
pub fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    tags.iter()
        .map(|t| genres::canonical(t).map_or_else(|| t.trim().to_owned(), str::to_owned))
        .filter(|t| !t.is_empty() && seen.insert(t.to_lowercase()))
        .collect()
}

/// A tag to rename across the library.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TagChange {
    pub from: String,
    pub to: String,
    pub book_ids: Vec<i64>,
    /// The tag is this genre written another way. When false it only
    /// mentions it ("Space Opera"), so it isn't chosen by default.
    pub exact: bool,
}

/// A Kobo collection to rename, or merge into the standard one.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CollectionTidy {
    pub from: String,
    pub to: String,
    pub books: usize,
    /// A collection with the standard name already exists: move the books
    /// into it and delete this one.
    pub merge: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TidyPlan {
    pub tags: Vec<TagChange>,
    pub collections: Vec<CollectionTidy>,
}

/// What would change. Only reads.
pub fn plan(library: &Library, device: Option<&KoboDevice>) -> Result<TidyPlan, String> {
    let mut by_tag: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    for book in library.books().map_err(|e| e.to_string())? {
        for tag in &book.tags {
            by_tag.entry(tag.clone()).or_default().push(book.id);
        }
    }
    let mut tags: Vec<TagChange> = by_tag
        .into_iter()
        .filter_map(|(from, book_ids)| {
            let (to, exact) = match genres::canonical(&from) {
                Some(name) => (name, true),
                None => (genres::classify(&from)?.name, false),
            };
            (to != from).then(|| TagChange {
                from,
                to: to.to_owned(),
                book_ids,
                exact,
            })
        })
        .collect();
    tags.sort_by(|a, b| {
        b.exact
            .cmp(&a.exact)
            .then_with(|| a.to.cmp(&b.to))
            .then_with(|| a.from.to_lowercase().cmp(&b.from.to_lowercase()))
    });

    let mut collections = Vec::new();
    if let Some(device) = device {
        let db = device.open_db().map_err(|e| e.to_string())?;
        let existing = db.collections().map_err(|e| e.to_string())?;
        let names: HashSet<&str> = existing.iter().map(|c| c.name.as_str()).collect();
        // Two variants of one genre: the first is renamed, the second merges.
        let mut claimed: HashSet<&'static str> = HashSet::new();
        for c in &existing {
            let Some(to) = genres::canonical(&c.name).filter(|to| *to != c.name) else {
                continue;
            };
            let merge = names.contains(to) || !claimed.insert(to);
            collections.push(CollectionTidy {
                from: c.name.clone(),
                to: to.to_owned(),
                books: c.book_count,
                merge,
            });
        }
    }
    Ok(TidyPlan { tags, collections })
}

/// The changes the user chose: (from, to) pairs from a [`TidyPlan`].
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TidyChoice {
    pub tags: Vec<(String, String)>,
    pub collections: Vec<(String, String)>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct TidyResult {
    /// Library books whose tags changed.
    pub books: usize,
    /// Kobo collections renamed or merged.
    pub collections: usize,
}

/// Applies the chosen renames: library tags, then the Kobo's collections in
/// one backed-up write session. Library books aren't marked as edited (the
/// Kobo doesn't show tags, so their copies there needn't be updated).
pub fn apply_tidy(
    library: &mut Library,
    device: Option<&KoboDevice>,
    backup_root: &Path,
    choice: &TidyChoice,
) -> Result<TidyResult, String> {
    let mut result = TidyResult::default();
    let renames: HashMap<&str, &str> = choice
        .tags
        .iter()
        .map(|(from, to)| (from.as_str(), to.as_str()))
        .collect();
    if !renames.is_empty() {
        for book in library.books().map_err(|e| e.to_string())? {
            if !book.tags.iter().any(|t| renames.contains_key(t.as_str())) {
                continue;
            }
            let mut seen = HashSet::new();
            let tags: Vec<String> = book
                .tags
                .iter()
                .map(|t| {
                    renames
                        .get(t.as_str())
                        .map_or(t.as_str(), |to| *to)
                        .to_owned()
                })
                .filter(|t| seen.insert(t.to_lowercase()))
                .collect();
            library.retag(book.id, &tags).map_err(|e| e.to_string())?;
            result.books += 1;
        }
    }

    if let Some(device) = device.filter(|_| !choice.collections.is_empty()) {
        let db = device.open_db().map_err(|e| e.to_string())?;
        let existing: HashSet<String> = db
            .collections()
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|c| c.name)
            .collect();
        let mut members: HashMap<String, Vec<String>> = HashMap::new();
        for (content_id, shelves) in db.memberships().map_err(|e| e.to_string())? {
            for shelf in shelves {
                members.entry(shelf).or_default().push(content_id.clone());
            }
        }
        drop(db);
        let mut changes = Vec::new();
        let mut made: HashSet<&str> = HashSet::new();
        for (from, to) in &choice.collections {
            if !existing.contains(from) || from == to {
                continue;
            }
            if existing.contains(to) || made.contains(to.as_str()) {
                changes.push(CollectionChange::Add {
                    name: to.clone(),
                    content_ids: members.get(from).cloned().unwrap_or_default(),
                });
                changes.push(CollectionChange::Delete { name: from.clone() });
            } else {
                changes.push(CollectionChange::Rename {
                    from: from.clone(),
                    to: to.clone(),
                });
                made.insert(to);
            }
            result.collections += 1;
        }
        if !changes.is_empty() {
            apply(device, backup_root, &changes).map_err(|e| e.to_string())?;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_and_tags_use_standard_names() {
        assert_eq!(
            folder_genre("KEpub/Sci-Fi/a.kepub.epub"),
            Some("Science Fiction")
        );
        assert_eq!(folder_genre("Non-Fiction/a.epub"), Some("Nonfiction"));
        assert_eq!(folder_genre("KEpub/a.kepub.epub"), None);
        assert_eq!(folder_genre("a.epub"), None);
        let tags: Vec<String> = [
            "sci-fi",
            "Science Fiction",
            "Space Opera",
            "Non Fiction",
            " ",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            normalize_tags(&tags),
            ["Science Fiction", "Space Opera", "Nonfiction"]
        );
    }
}
