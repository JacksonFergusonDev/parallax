//! Looking over books just added to the library: what's worth fixing while
//! they're fresh. Only what the app can do something about, or the user
//! should know before relying on the book, is reported; notes such as
//! embedded fonts stay in "Check library".

use std::path::Path;

use parallax_library::{Book, BookFiles};
use serde::Serialize;

use crate::duplicates::LibraryGroup;
use crate::enhance::has_genre;
use crate::health::{Check, check_files};

/// Library setting: "off" stops the review after adding books.
pub const REVIEW_ON_ADD: &str = "review_on_add";
/// More books than this are offered for review rather than opening it and
/// looking them up unasked.
pub const AUTO_REVIEW_LIMIT: usize = 25;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Issue {
    /// The file is unusable or has parts missing.
    Damaged,
    Drm,
    NoGenre,
    NoAuthor,
    TitleFromFile,
    NoCover,
    SmallCover,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReviewFinding {
    pub issue: Issue,
    pub book_id: i64,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Review {
    pub findings: Vec<ReviewFinding>,
    /// Probable duplicates involving these books.
    pub duplicates: Vec<LibraryGroup>,
    /// Too many books to review without being asked.
    pub bulk: bool,
}

/// Reviews `books`. Takes copies so callers needn't hold the library while
/// the files are read.
pub fn review(books: &[Book], files: &[BookFiles], duplicates: Vec<LibraryGroup>) -> Review {
    let mut findings: Vec<ReviewFinding> = check_files(books, files, |_, _| {})
        .into_iter()
        .filter_map(|f| {
            let issue = match f.check {
                Check::Unreadable | Check::MissingFiles | Check::DamagedPages => Issue::Damaged,
                Check::Drm => Issue::Drm,
                Check::NoCover => Issue::NoCover,
                Check::SmallCover => Issue::SmallCover,
                Check::NoAuthor => Issue::NoAuthor,
                _ => return None,
            };
            Some(ReviewFinding {
                issue,
                book_id: *f.book_ids.first()?,
                detail: f.detail,
            })
        })
        .collect();
    for book in books {
        let mut add = |issue| {
            findings.push(ReviewFinding {
                issue,
                book_id: book.id,
                detail: String::new(),
            });
        };
        if !has_genre(book) {
            add(Issue::NoGenre);
        }
        if title_from_file(&book.title, &book.original_name) {
            add(Issue::TitleFromFile);
        }
    }
    findings.sort_by_key(|f| (f.issue, f.book_id));
    let ids: Vec<i64> = books.iter().map(|b| b.id).collect();
    Review {
        findings,
        duplicates: duplicates
            .into_iter()
            .filter(|g| g.members.iter().any(|m| ids.contains(&m.book_id)))
            .collect(),
        bulk: books.len() > AUTO_REVIEW_LIMIT,
    }
}

/// Whether a title is really a file name, as when a book names no title of
/// its own. "Dune" from `Dune.epub` is fine; `the_stand-v2` isn't.
fn title_from_file(title: &str, file_name: &str) -> bool {
    let title = title.trim();
    let lower = title.to_lowercase();
    let extension = [
        ".epub", ".kepub", ".mobi", ".azw", ".azw3", ".kfx", ".prc", ".pdf",
    ]
    .iter()
    .any(|e| lower.ends_with(e));
    let placeholder = ["unknown", "untitled", "no title"].contains(&lower.as_str());
    let stem = Path::new(file_name)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let hyphenated = !title.contains(' ') && title.matches('-').count() >= 2;
    extension
        || placeholder
        || title.contains('_')
        || (hyphenated && title.eq_ignore_ascii_case(stem))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_are_told_from_titles() {
        for (title, file) in [
            ("the_stand", "the_stand.epub"),
            ("the-stand-v2", "the-stand-v2.epub"),
            ("Book.epub", "whatever.epub"),
            ("Book.EPUB", "whatever.epub"),
            ("Unknown", "a.epub"),
            ("Foo_Bar Baz", "x.epub"),
        ] {
            assert!(title_from_file(title, file), "{title}");
        }
        for (title, file) in [
            ("Dune", "Dune.epub"),
            ("The Stand", "The Stand.epub"),
            ("Catch-22", "Catch-22.epub"),
            ("Slaughterhouse-Five", "Slaughterhouse-Five.epub"),
            ("Twenty-One Stories", "Twenty-One Stories.epub"),
        ] {
            assert!(!title_from_file(title, file), "{title}");
        }
    }
}
