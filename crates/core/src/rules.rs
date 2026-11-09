//! Automatic grouping rules: series, tag or author → collection.
//!
//! Rules only ever add books to collections; they never remove books or
//! delete collections, so they can't undo organising done by hand. Runs
//! the app starts by itself (after a send, on connect) only file books into
//! collections that already exist: new collections are made only when the
//! user applies the rules after seeing the preview, so a broad rule (a
//! collection per tag, at least 1 book) can't fill the Kobo unasked. Books the
//! library knows about use its metadata (which has tags); other books on the
//! device use what Nickel shows (series and author).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use parallax_device_kobo::KoboDevice;
use parallax_library::{Library, Rule, RuleKind};
use serde::Serialize;

use crate::collections::{CollectionChange, apply};

/// A book as far as rules are concerned.
#[derive(Debug, Clone)]
struct Item {
    content_id: String,
    title: String,
    series: Option<String>,
    authors: Vec<String>,
    tags: Vec<String>,
    /// The genre its folder on the device names.
    folder_genre: Option<&'static str>,
    /// Collections it's already in.
    collections: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PlannedCollection {
    pub name: String,
    /// The collection doesn't exist on the device yet.
    pub new: bool,
    pub content_ids: Vec<String>,
    /// Titles of the books being added, for the preview.
    pub titles: Vec<String>,
}

/// What enabled `rules` would add on `device`, largest additions first.
pub fn plan(
    rules: &[Rule],
    library: &Library,
    device: &KoboDevice,
) -> parallax_device_kobo::Result<Vec<PlannedCollection>> {
    let rules: Vec<&Rule> = rules.iter().filter(|r| r.enabled).collect();
    if rules.is_empty() {
        return Ok(Vec::new());
    }
    let db = device.open_db()?;
    let existing: HashSet<String> = db.collections()?.into_iter().map(|c| c.name).collect();
    let memberships = db.memberships()?;
    let items = items(library, device, db.books()?, &memberships);
    Ok(plan_items(&rules, &items, &existing))
}

fn plan_items(
    rules: &[&Rule],
    items: &[Item],
    existing: &HashSet<String>,
) -> Vec<PlannedCollection> {
    // Collection name → books to add (several rules can target one name).
    let mut groups: BTreeMap<String, Vec<&Item>> = BTreeMap::new();
    for rule in rules {
        let mut by_key: HashMap<String, Vec<&Item>> = HashMap::new();
        for item in items {
            for key in keys(rule.kind, item) {
                by_key.entry(key).or_default().push(item);
            }
        }
        for (key, members) in by_key
            .into_iter()
            .filter(|(_, m)| m.len() >= rule.min_books)
        {
            let name = format!("{}{key}", rule.prefix);
            let add = groups.entry(name.clone()).or_default();
            for item in members {
                if !item.collections.contains(&name)
                    && !add.iter().any(|i| i.content_id == item.content_id)
                {
                    add.push(item);
                }
            }
        }
    }
    let mut planned: Vec<PlannedCollection> = groups
        .into_iter()
        .filter(|(_, add)| !add.is_empty())
        .map(|(name, add)| PlannedCollection {
            new: !existing.contains(&name),
            content_ids: add.iter().map(|i| i.content_id.clone()).collect(),
            titles: add.iter().map(|i| i.title.clone()).collect(),
            name,
        })
        .collect();
    planned.sort_by(|a, b| {
        b.content_ids
            .len()
            .cmp(&a.content_ids.len())
            .then_with(|| a.name.cmp(&b.name))
    });
    planned
}

fn keys(kind: RuleKind, item: &Item) -> Vec<String> {
    let clean = |s: &String| {
        let t = s.trim();
        (!t.is_empty()).then(|| t.to_owned())
    };
    match kind {
        RuleKind::Series => item.series.iter().filter_map(clean).collect(),
        // Genre tags under their standard names, so "sci-fi" and "Sci-Fi"
        // share one collection.
        RuleKind::Tag => crate::genres::normalize_tags(&item.tags),
        RuleKind::Author => item.authors.iter().filter_map(clean).collect(),
        RuleKind::Genre => {
            let mut genres: Vec<String> = item
                .tags
                .iter()
                .filter_map(|t| parallax_metadata::genres::canonical(t))
                .map(str::to_owned)
                .collect();
            // A book without genre tags (most books nobody has tagged) gets
            // its folder's genre.
            if genres.is_empty() {
                genres.extend(item.folder_genre.map(str::to_owned));
            }
            genres.dedup();
            genres
        }
    }
}

/// Device books, with library metadata for books the library sent (including
/// ones Nickel hasn't imported yet: their collection rows can be written now).
fn items(
    library: &Library,
    device: &KoboDevice,
    device_books: Vec<parallax_device::DeviceBook>,
    memberships: &HashMap<String, Vec<String>>,
) -> Vec<Item> {
    let copies = library
        .device_copies(&device.version().serial)
        .unwrap_or_default();
    let mut by_id: HashMap<String, Item> = device_books
        .into_iter()
        .map(|b| {
            let item = Item {
                content_id: b.id.clone(),
                title: b.title,
                series: b.series,
                // Nickel joins several authors with ", " in Attribution.
                authors: b.author.map(|a| vec![a]).unwrap_or_default(),
                tags: Vec::new(),
                folder_genre: b.path.as_deref().and_then(crate::genres::folder_genre),
                collections: b.collections,
            };
            (b.id, item)
        })
        .collect();
    for copy in copies {
        if !device.mount().join(&copy.device_path).is_file() {
            continue;
        }
        let Ok(book) = library.book(copy.book_id) else {
            continue;
        };
        // ShelfContent also covers books Nickel hasn't imported yet.
        let collections = memberships
            .get(&copy.content_id)
            .cloned()
            .unwrap_or_default();
        by_id.insert(
            copy.content_id.clone(),
            Item {
                folder_genre: crate::genres::folder_genre(&copy.device_path),
                content_id: copy.content_id,
                title: book.title,
                series: book.series,
                authors: book.authors,
                tags: book.tags,
                collections,
            },
        );
    }
    let mut items: Vec<Item> = by_id.into_values().collect();
    items.sort_by(|a, b| a.title.cmp(&b.title));
    items
}

/// Applies a plan in one backed-up write session. Returns how many books were
/// added to collections.
pub fn apply_plan(
    device: &KoboDevice,
    backup_root: &Path,
    plan: &[PlannedCollection],
) -> parallax_device_kobo::Result<usize> {
    if plan.is_empty() {
        return Ok(0);
    }
    let changes: Vec<CollectionChange> = plan
        .iter()
        .map(|p| CollectionChange::Add {
            name: p.name.clone(),
            content_ids: p.content_ids.clone(),
        })
        .collect();
    apply(device, backup_root, &changes)?;
    Ok(plan.iter().map(|p| p.content_ids.len()).sum())
}

/// Who started a run, which decides whether it may create collections.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    /// The app, after a send or on connect: existing collections only.
    Automatic,
    /// The user, after the preview: everything planned.
    User,
}

/// Plans and applies enabled rules; opens no write session if there's
/// nothing to do.
pub fn run(
    library: &Library,
    device: &KoboDevice,
    backup_root: &Path,
    trigger: Trigger,
) -> parallax_device_kobo::Result<usize> {
    let rules = library.rules().unwrap_or_default();
    let mut planned = plan(&rules, library, device)?;
    if trigger == Trigger::Automatic {
        planned.retain(|p| !p.new);
    }
    apply_plan(device, backup_root, &planned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(
        id: &str,
        series: Option<&str>,
        authors: &[&str],
        tags: &[&str],
        collections: &[&str],
    ) -> Item {
        Item {
            content_id: id.into(),
            title: id.into(),
            series: series.map(Into::into),
            authors: authors.iter().map(ToString::to_string).collect(),
            tags: tags.iter().map(ToString::to_string).collect(),
            folder_genre: None,
            collections: collections.iter().map(ToString::to_string).collect(),
        }
    }

    fn rule(kind: RuleKind, min_books: usize, prefix: &str) -> Rule {
        Rule {
            id: 1,
            kind,
            enabled: true,
            min_books,
            prefix: prefix.into(),
        }
    }

    #[test]
    fn plans_additions_only_above_threshold() {
        let items = [
            item(
                "lies",
                Some("Gentleman Bastard"),
                &["Scott Lynch"],
                &["Fantasy"],
                &[],
            ),
            item(
                "red",
                Some("Gentleman Bastard"),
                &["Scott Lynch"],
                &["Fantasy"],
                &["Gentleman Bastard"],
            ),
            item("dune", Some("Dune"), &["Frank Herbert"], &["Sci-Fi"], &[]),
            item("emma", None, &["Jane Austen"], &[], &[]),
        ];
        let series = rule(RuleKind::Series, 2, "");
        let tags = rule(RuleKind::Tag, 1, "#");
        let existing: HashSet<String> = ["Gentleman Bastard".to_owned()].into();
        let plan = plan_items(&[&series, &tags], &items, &existing);
        let summary: Vec<(&str, bool, Vec<&str>)> = plan
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.new,
                    p.content_ids.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        assert_eq!(
            summary,
            [
                ("#Fantasy", true, vec!["lies", "red"]),
                // The "Sci-Fi" tag under the genre's standard name.
                ("#Science Fiction", true, vec!["dune"]),
                // Dune has 1 book, below min 2; "red" is already in its series.
                ("Gentleman Bastard", false, vec!["lies"]),
            ]
        );
    }

    #[test]
    fn genre_rule_uses_standard_names_and_folders() {
        let mut from_folder = item("folder", None, &["A"], &[], &[]);
        from_folder.folder_genre = Some("Science Fiction");
        let mut tagged_in_other_folder =
            item("tagged", None, &["B"], &["sci-fi", "Space Opera"], &[]);
        tagged_in_other_folder.folder_genre = Some("Fantasy");
        let items = [
            from_folder,
            tagged_in_other_folder,
            item("non", None, &["C"], &["Non-Fiction"], &[]),
            item("plain", None, &["D"], &["Favourites"], &[]),
        ];
        let genres = rule(RuleKind::Genre, 1, "");
        let plan = plan_items(&[&genres], &items, &HashSet::new());
        let summary: Vec<(&str, Vec<&str>)> = plan
            .iter()
            .map(|p| {
                (
                    p.name.as_str(),
                    p.content_ids.iter().map(String::as_str).collect(),
                )
            })
            .collect();
        // Genre tags win over the folder; non-genre tags make nothing.
        assert_eq!(
            summary,
            [
                ("Science Fiction", vec!["folder", "tagged"]),
                ("Nonfiction", vec!["non"]),
            ]
        );
    }

    #[test]
    fn nothing_to_do_when_everything_is_in_place() {
        let items = [
            item("a", None, &["Ann"], &[], &["Ann"]),
            item("b", None, &["Ann"], &[], &["Ann"]),
        ];
        let authors = rule(RuleKind::Author, 2, "");
        assert!(plan_items(&[&authors], &items, &HashSet::new()).is_empty());
    }
}
