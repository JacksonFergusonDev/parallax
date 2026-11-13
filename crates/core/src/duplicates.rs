//! Finding books that are probably the same, in the library or on a device,
//! and suggesting which copy to keep.
//!
//! Books are grouped when they share an identical file, an ISBN, or a title
//! and author once both are normalised (case, punctuation, leading articles,
//! subtitles and trailing "(Series, Book 1)" ignored; authors compared by
//! surname). Suggestions are conservative: a copy with reading progress, a
//! store copy, or the only copy that's on a device is never suggested for
//! removal. Groups the user marked "not duplicates" stay hidden until a new
//! copy joins them.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::Path;

use parallax_device::{DeviceBook, ReadStatus};
use parallax_device_kobo::KoboDevice;
use parallax_library::{Book, Library, normalize_authors};
use serde::Serialize;
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    SameFile,
    SameIsbn,
    SameTitleAndAuthor,
}

/// What the engine needs to know about each book.
#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub title: String,
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    /// SHA-256 of the file, when worth computing.
    pub file_hash: Option<String>,
}

/// Indices into the candidates, with why they were grouped.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    pub members: Vec<usize>,
    pub reasons: Vec<Reason>,
}

/// Groups candidates that share any key (connected components, so A~B and
/// B~C put all three together). Groups are ordered by their first member.
pub fn group(candidates: &[Candidate]) -> Vec<Group> {
    fn find(parent: &mut [usize], i: usize) -> usize {
        let mut root = i;
        while parent[root] != root {
            root = parent[root];
        }
        parent[i] = root;
        root
    }
    let mut parent: Vec<usize> = (0..candidates.len()).collect();
    // (reason, key) → first candidate with it.
    let mut first: HashMap<(Reason, String), usize> = HashMap::new();
    let mut links: Vec<(usize, usize, Reason)> = Vec::new();
    for (i, c) in candidates.iter().enumerate() {
        let keys = [
            (Reason::SameFile, c.file_hash.clone()),
            (Reason::SameIsbn, c.isbn.as_deref().map(normalise_isbn)),
            (
                Reason::SameTitleAndAuthor,
                title_author_key(&c.title, &c.authors),
            ),
        ];
        for (reason, key) in keys {
            let Some(key) = key.filter(|k| !k.is_empty()) else {
                continue;
            };
            match first.get(&(reason, key.clone())) {
                Some(&j) => {
                    links.push((j, i, reason));
                    let (a, b) = (find(&mut parent, j), find(&mut parent, i));
                    parent[b.max(a)] = a.min(b);
                }
                None => {
                    first.insert((reason, key), i);
                }
            }
        }
    }
    let mut groups: HashMap<usize, (BTreeSet<usize>, BTreeSet<Reason>)> = HashMap::new();
    for (a, b, reason) in links {
        let root = find(&mut parent, a);
        let entry = groups.entry(root).or_default();
        entry.0.extend([a, b]);
        entry.1.insert(reason);
    }
    let mut out: Vec<Group> = groups
        .into_values()
        .map(|(members, reasons)| Group {
            members: members.into_iter().collect(),
            reasons: reasons.into_iter().collect(),
        })
        .collect();
    out.sort_by_key(|g| g.members[0]);
    out
}

pub(crate) fn normalise_isbn(isbn: &str) -> String {
    isbn.chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_uppercase)
        .collect()
}

fn alnum(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// "The Color of Magic: A Discworld Novel (Discworld, #1)" and
/// "Color of Magic, The" by "Pratchett, Terry" share a key.
pub(crate) fn title_author_key(title: &str, authors: &[String]) -> Option<String> {
    let mut t = title.trim();
    if let Some(i) = t.find(['(', '[']).filter(|&i| i > 0) {
        t = &t[..i];
    }
    if let Some((main, _)) = t.split_once(": ").filter(|(m, _)| !m.trim().is_empty()) {
        t = main;
    }
    let lower = t.trim().to_lowercase();
    let mut t = lower.as_str();
    for article in ["the ", "a ", "an "] {
        t = t.strip_prefix(article).unwrap_or(t);
    }
    for article in [", the", ", a", ", an"] {
        t = t.strip_suffix(article).unwrap_or(t);
    }
    let title = alnum(t);
    if title.is_empty() {
        return None;
    }
    let surname = normalize_authors(authors)
        .first()
        .and_then(|a| a.split_whitespace().last().map(alnum))
        .unwrap_or_default();
    Some(format!("{title}|{surname}"))
}

/// The key a set of members is remembered by when dismissed.
pub fn group_key<'a>(ids: impl IntoIterator<Item = &'a str>) -> String {
    let mut ids: Vec<&str> = ids.into_iter().collect();
    ids.sort_unstable();
    ids.join("\n")
}

/// Scope names for dismissed groups.
pub const LIBRARY_SCOPE: &str = "library";
pub fn device_scope(serial: &str) -> String {
    format!("device:{serial}")
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryMember {
    pub book_id: i64,
    /// Suggested for removal.
    pub remove: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LibraryGroup {
    pub key: String,
    pub reasons: Vec<Reason>,
    pub members: Vec<LibraryMember>,
}

/// Probable duplicates among library books, minus dismissed groups.
pub fn library_duplicates(library: &Library) -> parallax_library::Result<Vec<LibraryGroup>> {
    let books = library.books()?;
    let on_device = library.books_on_any_device()?;
    let dismissed = library.dismissed_duplicates(LIBRARY_SCOPE)?;
    let candidates: Vec<Candidate> = books
        .iter()
        .map(|b| Candidate {
            title: b.title.clone(),
            authors: b.authors.clone(),
            isbn: b.isbn().map(str::to_owned),
            // Identical files are refused at import.
            file_hash: None,
        })
        .collect();
    Ok(group(&candidates)
        .into_iter()
        .filter_map(|g| {
            let members: Vec<&Book> = g.members.iter().map(|&i| &books[i]).collect();
            let ids: Vec<String> = members.iter().map(|b| b.id.to_string()).collect();
            let key = group_key(ids.iter().map(String::as_str));
            if dismissed.contains(&key) {
                return None;
            }
            let keep = members
                .iter()
                .max_by_key(|b| {
                    (
                        on_device.contains(&b.id),
                        b.custom_cover,
                        b.has_cover,
                        usize::from(b.series.is_some()) + b.tags.len(),
                        b.modified_at.clone(),
                    )
                })
                .map(|b| b.id);
            Some(LibraryGroup {
                key,
                reasons: g.reasons,
                members: members
                    .iter()
                    .map(|b| LibraryMember {
                        book_id: b.id,
                        // A copy on a device is linked to it; removing it
                        // would orphan that link, so it's never suggested.
                        remove: Some(b.id) != keep && !on_device.contains(&b.id),
                    })
                    .collect(),
            })
        })
        .collect())
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceMember {
    pub content_id: String,
    /// "KEPUB", "EPUB", "PDF"… or "Kobo store".
    pub format: String,
    pub size: Option<u64>,
    /// Parallax can remove it (sideloaded).
    pub removable: bool,
    /// Suggested for removal.
    pub remove: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeviceGroup {
    pub key: String,
    pub reasons: Vec<Reason>,
    pub members: Vec<DeviceMember>,
}

/// Probable duplicates on a device, minus groups dismissed for it. Only
/// reads the device; files are hashed only when their sizes match.
pub fn device_duplicates(
    library: &Library,
    device: &KoboDevice,
) -> parallax_device_kobo::Result<Vec<DeviceGroup>> {
    let books = device.open_db()?.books()?;
    let dismissed = library
        .dismissed_duplicates(&device_scope(&device.version().serial))
        .unwrap_or_default();
    let sizes: Vec<Option<u64>> = books
        .iter()
        .map(|b| {
            b.path
                .as_ref()
                .and_then(|p| fs::metadata(device.mount().join(p)).ok())
                .map(|m| m.len())
        })
        .collect();
    let mut seen = HashMap::new();
    for size in sizes.iter().flatten() {
        *seen.entry(*size).or_insert(0) += 1;
    }
    let candidates: Vec<Candidate> = books
        .iter()
        .zip(&sizes)
        .map(|(b, size)| Candidate {
            title: b.title.clone(),
            authors: b.author.iter().cloned().collect(),
            isbn: None,
            file_hash: size
                .filter(|s| seen.get(s).copied().unwrap_or(0) > 1)
                .and_then(|_| hash(&device.mount().join(b.path.as_deref()?))),
        })
        .collect();
    Ok(group(&candidates)
        .into_iter()
        .filter_map(|g| {
            let members: Vec<(&DeviceBook, Option<u64>)> =
                g.members.iter().map(|&i| (&books[i], sizes[i])).collect();
            let key = group_key(members.iter().map(|(b, _)| b.id.as_str()));
            if dismissed.contains(&key) {
                return None;
            }
            let keep = members
                .iter()
                .max_by_key(|(b, size)| {
                    (progress(b), !b.sideloaded, format_of(b) == "KEPUB", *size)
                })
                .map(|(b, _)| b.id.clone());
            Some(DeviceGroup {
                key,
                reasons: g.reasons,
                members: members
                    .iter()
                    .map(|(b, size)| DeviceMember {
                        content_id: b.id.clone(),
                        format: format_of(b),
                        size: *size,
                        removable: b.sideloaded,
                        remove: b.sideloaded && Some(&b.id) != keep.as_ref() && progress(b) == 0,
                    })
                    .collect(),
            })
        })
        .collect())
}

/// 0 unread, 1–100 percent read, 101 finished.
fn progress(b: &DeviceBook) -> u8 {
    match b.read_status {
        ReadStatus::Unread => 0,
        ReadStatus::Reading => b.percent_read.clamp(1, 100),
        ReadStatus::Finished => 101,
    }
}

#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "already lowercased"
)]
fn format_of(b: &DeviceBook) -> String {
    let Some(path) = b.path.as_deref().filter(|_| b.sideloaded) else {
        return "Kobo store".into();
    };
    let lower = path.to_lowercase();
    if lower.ends_with(".kepub.epub") || lower.ends_with(".kepub") {
        "KEPUB".into()
    } else {
        lower.rsplit('.').next().unwrap_or_default().to_uppercase()
    }
}

fn hash(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(
        hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// A candidate in more than one group would be a bug.
    fn disjoint(groups: &[Group]) -> bool {
        let mut seen = HashSet::new();
        groups
            .iter()
            .flat_map(|g| &g.members)
            .all(|m| seen.insert(*m))
    }

    fn c(title: &str, author: &str) -> Candidate {
        Candidate {
            title: title.into(),
            authors: if author.is_empty() {
                vec![]
            } else {
                vec![author.into()]
            },
            ..Default::default()
        }
    }

    #[test]
    fn titles_and_authors_are_normalised() {
        let books = [
            c("The Color of Magic", "Terry Pratchett"),
            c("Color of Magic, The", "Pratchett, Terry"),
            c("The Colour of Magic", "Terry Pratchett"),
            c("Dune (Dune Chronicles, Book 1)", "Frank Herbert"),
            c("Dune: Deluxe Edition", "Herbert, Frank"),
            c("Dune Messiah", "Frank Herbert"),
            c("Dune", "Brian Herbert"),
        ];
        let groups = group(&books);
        assert_eq!(
            groups,
            [
                Group {
                    members: vec![0, 1],
                    reasons: vec![Reason::SameTitleAndAuthor]
                },
                // Brian and Frank share a surname: grouped for review, never
                // removed without the user choosing.
                Group {
                    members: vec![3, 4, 6],
                    reasons: vec![Reason::SameTitleAndAuthor]
                },
            ]
        );
    }

    #[test]
    fn keys_chain_into_one_group() {
        let mut a = c("Mistborn", "Brandon Sanderson");
        a.isbn = Some("978-0-7653-1178-8".into());
        let mut b = c("The Final Empire", "Brandon Sanderson");
        b.isbn = Some("9780765311788".into());
        let mut d = c("Final Empire", "B. Sanderson");
        d.file_hash = Some("abc".into());
        let mut e = c("Something else", "Someone");
        e.file_hash = Some("abc".into());
        let groups = group(&[a, b, d, e]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].members, [0, 1, 2, 3]);
        assert_eq!(
            groups[0].reasons,
            [
                Reason::SameFile,
                Reason::SameIsbn,
                Reason::SameTitleAndAuthor
            ]
        );
        assert!(disjoint(&groups));
    }

    #[test]
    fn empty_titles_never_match() {
        assert!(group(&[c("", "A"), c("", "A"), c("()", "A"), c("()", "A")]).is_empty());
    }

    #[test]
    fn group_keys_ignore_order() {
        assert_eq!(group_key(["b", "a"]), group_key(["a", "b"]));
    }
}
