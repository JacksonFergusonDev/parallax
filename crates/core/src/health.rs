//! "Check library": problems worth fixing before books go to a reader, from
//! the Quality Check calibre plugin (kiwidude68), limited to checks that need
//! only the library and the EPUB files.

use std::collections::BTreeMap;

use erc_epub::inspect;
use erc_library::{Book, Library};
use serde::Serialize;

/// Covers shorter than this look soft on current Kobos (the Libra 2 screen
/// is 1680 px tall); the cover picker uses the same line.
pub const SMALL_COVER_HEIGHT: u32 = 1200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Check {
    Unreadable,
    Drm,
    MissingFiles,
    BrokenLinks,
    NoCover,
    SmallCover,
    NoAuthor,
    SeriesWithoutNumber,
    SeriesGap,
    Unmanifested,
    /// Left out of copies sent to devices automatically.
    Clutter,
    /// Not a problem; some readers want to know.
    EmbeddedFonts,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub check: Check,
    pub book_ids: Vec<i64>,
    pub detail: String,
}

/// Runs every check; `on_progress(done, total)` after each book's file.
pub fn check_library(
    library: &Library,
    mut on_progress: impl FnMut(usize, usize),
) -> erc_library::Result<Vec<Finding>> {
    let books = library.books()?;
    let mut out = Vec::new();
    let one = |check, id, detail: String| Finding {
        check,
        book_ids: vec![id],
        detail,
    };
    for (i, book) in books.iter().enumerate() {
        match inspect(&library.book_file(book.id)) {
            Err(e) => out.push(one(Check::Unreadable, book.id, e.to_string())),
            Ok(found) => {
                let list = |v: &[String]| {
                    let shown: Vec<&str> = v.iter().take(3).map(String::as_str).collect();
                    let more = v.len().saturating_sub(3);
                    if more > 0 {
                        format!("{} and {more} more", shown.join(", "))
                    } else {
                        shown.join(", ")
                    }
                };
                if found.drm {
                    out.push(one(Check::Drm, book.id, "protected by DRM".into()));
                }
                if !found.missing_files.is_empty() {
                    out.push(one(
                        Check::MissingFiles,
                        book.id,
                        list(&found.missing_files),
                    ));
                }
                if !found.broken_links.is_empty() {
                    let mut links: Vec<String> = Vec::new();
                    for (_, target) in &found.broken_links {
                        if !links.contains(target) {
                            links.push(target.clone());
                        }
                    }
                    out.push(one(Check::BrokenLinks, book.id, list(&links)));
                }
                if !found.unmanifested.is_empty() {
                    out.push(one(Check::Unmanifested, book.id, list(&found.unmanifested)));
                }
                if !found.clutter.is_empty() {
                    out.push(one(Check::Clutter, book.id, list(&found.clutter)));
                }
                if !found.fonts.is_empty() {
                    out.push(one(Check::EmbeddedFonts, book.id, list(&found.fonts)));
                }
            }
        }
        match library.cover_size(book.id) {
            None => out.push(one(Check::NoCover, book.id, String::new())),
            Some((w, h)) if h < SMALL_COVER_HEIGHT => {
                out.push(one(Check::SmallCover, book.id, format!("{w}×{h}")));
            }
            Some(_) => {}
        }
        if book.authors.is_empty()
            || book
                .authors
                .iter()
                .all(|a| a.trim().eq_ignore_ascii_case("unknown"))
        {
            out.push(one(Check::NoAuthor, book.id, String::new()));
        }
        if book.series.is_some() && book.series_index.is_none() {
            out.push(one(
                Check::SeriesWithoutNumber,
                book.id,
                book.series.clone().unwrap_or_default(),
            ));
        }
        on_progress(i + 1, books.len());
    }
    out.extend(series_gaps(&books));
    out.sort_by_key(|f| f.check);
    Ok(out)
}

/// Whole-numbered series with numbers missing between the first and last
/// one the library has ("Discworld: #3 missing").
pub fn series_gaps(books: &[Book]) -> Vec<Finding> {
    let mut series: BTreeMap<String, (String, Vec<(u32, i64)>)> = BTreeMap::new();
    for b in books {
        let (Some(name), Some(index)) = (&b.series, &b.series_index) else {
            continue;
        };
        let Ok(n) = index.trim().parse::<f64>() else {
            continue;
        };
        if n.fract() != 0.0 || !(1.0..=10_000.0).contains(&n) {
            continue;
        }
        series
            .entry(name.to_lowercase())
            .or_insert_with(|| (name.clone(), Vec::new()))
            .1
            .push((n as u32, b.id));
    }
    series
        .into_values()
        .filter_map(|(name, mut members)| {
            members.sort();
            let have: Vec<u32> = members.iter().map(|m| m.0).collect();
            let (first, last) = (*have.first()?, *have.last()?);
            let missing: Vec<String> = (first..=last)
                .filter(|n| !have.contains(n))
                .map(|n| format!("#{n}"))
                .collect();
            (!missing.is_empty()).then(|| Finding {
                check: Check::SeriesGap,
                book_ids: members.iter().map(|m| m.1).collect(),
                detail: format!("{name}: {} missing", missing.join(", ")),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn book(id: i64, series: &str, index: &str) -> Book {
        Book {
            id,
            title: format!("B{id}"),
            authors: vec!["A".into()],
            contributors: None,
            series: Some(series.into()),
            series_index: Some(index.into()),
            tags: vec![],
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            original_name: "b.epub".into(),
            file_size: 1,
            has_cover: true,
            custom_cover: false,
            cover_upscaled: false,
            added_at: String::new(),
            modified_at: String::new(),
        }
    }

    #[test]
    fn finds_gaps_in_whole_numbered_series() {
        let books = [
            book(1, "Discworld", "1"),
            book(2, "discworld", "4.0"),
            book(3, "Discworld", "2"),
            book(4, "Novellas", "1.5"),
            book(5, "Novellas", "3"),
            book(6, "Complete", "1"),
            book(7, "Complete", "2"),
        ];
        let gaps = series_gaps(&books);
        assert_eq!(gaps.len(), 1);
        assert_eq!(gaps[0].detail, "Discworld: #3 missing");
        assert_eq!(gaps[0].book_ids, [1, 3, 2]);
    }
}
