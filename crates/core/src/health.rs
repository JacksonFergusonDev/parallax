//! "Check library": problems worth fixing before books go to a reader, from
//! the Quality Check calibre plugin (kiwidude68), limited to checks that need
//! only the library and the EPUB files.

use std::collections::BTreeMap;
use std::sync::Mutex;

use std::path::Path;

use erc_comic::{Cbz, SkipReason};
use erc_epub::{Inspection, inspect};
use erc_library::{Book, BookFiles, CoverQuality, Library};
use rayon::prelude::*;
use serde::Serialize;

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
    /// Comic pages that aren't readable images.
    DamagedPages,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Finding {
    pub check: Check,
    pub book_ids: Vec<i64>,
    pub detail: String,
}

/// Runs every check. Book files are read across the CPU's cores;
/// `on_progress(done, total)` follows each one, in the order they finish.
/// Findings come out in the same order however the reads were scheduled.
pub fn check_library(
    library: &Library,
    on_progress: impl FnMut(usize, usize) + Send,
) -> erc_library::Result<Vec<Finding>> {
    let books = library.books()?;
    let files: Vec<BookFiles> = books.iter().map(|b| library.book_files(b.id)).collect();
    let mut out = check_files(&books, &files, on_progress);
    out.extend(series_gaps(&books));
    out.sort_by_key(|f| f.check);
    Ok(out)
}

/// The checks on each of `books`, whose files are `files`, leaving out those
/// that compare books. Needs no library, so a caller can let go of it first.
pub fn check_files(
    books: &[Book],
    files: &[BookFiles],
    on_progress: impl FnMut(usize, usize) + Send,
) -> Vec<Finding> {
    let progress = Mutex::new((0, on_progress));
    let reads: Vec<FileRead> = files
        .par_iter()
        .map(|file| {
            let read = if is_comic(&file.book) {
                comic_read(&file.book, file.cover_size())
            } else {
                FileRead {
                    inspection: inspect(&file.book).map_err(|e| e.to_string()),
                    cover_size: file.cover_size(),
                    damaged_pages: Vec::new(),
                }
            };
            let mut progress = progress
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            progress.0 += 1;
            let done = progress.0;
            (progress.1)(done, books.len());
            read
        })
        .collect();

    let mut out = Vec::new();
    for (book, read) in books.iter().zip(reads) {
        book_findings(book, read, &mut out);
    }
    out
}

/// What the checks need from a book's files.
struct FileRead {
    inspection: Result<Inspection, String>,
    cover_size: Option<(u32, u32)>,
    /// Comic pages whose size can't be read from their headers.
    damaged_pages: Vec<String>,
}

fn is_comic(path: &Path) -> bool {
    path.extension().is_some_and(|e| e == "cbz")
}

/// A comic's checks in the EPUB checks' terms: hidden files are clutter and
/// other files that aren't pages are extra files.
fn comic_read(path: &Path, cover_size: Option<(u32, u32)>) -> FileRead {
    let mut read = FileRead {
        inspection: Err(String::new()),
        cover_size,
        damaged_pages: Vec::new(),
    };
    let mut comic = match Cbz::open(path) {
        Ok(c) => c,
        Err(e) => {
            read.inspection = Err(e.to_string());
            return read;
        }
    };
    let mut inspection = Inspection::default();
    for skipped in comic.skipped() {
        match skipped.reason {
            SkipReason::Hidden => inspection.clutter.push(skipped.name.clone()),
            _ => inspection.unmanifested.push(skipped.name.clone()),
        }
    }
    for i in 0..comic.pages().len() {
        if !matches!(comic.page_dimensions(i), Ok(Some(_))) {
            read.damaged_pages.push(comic.pages()[i].name.clone());
        }
    }
    read.inspection = Ok(inspection);
    read
}

fn book_findings(book: &Book, read: FileRead, out: &mut Vec<Finding>) {
    let one = |check, detail: String| Finding {
        check,
        book_ids: vec![book.id],
        detail,
    };
    match read.inspection {
        Err(e) => out.push(one(Check::Unreadable, e)),
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
                out.push(one(Check::Drm, "protected by DRM".into()));
            }
            if !found.missing_files.is_empty() {
                out.push(one(Check::MissingFiles, list(&found.missing_files)));
            }
            if !found.broken_links.is_empty() {
                let mut links: Vec<String> = Vec::new();
                for (_, target) in &found.broken_links {
                    if !links.contains(target) {
                        links.push(target.clone());
                    }
                }
                out.push(one(Check::BrokenLinks, list(&links)));
            }
            if !found.unmanifested.is_empty() {
                out.push(one(Check::Unmanifested, list(&found.unmanifested)));
            }
            if !found.clutter.is_empty() {
                out.push(one(Check::Clutter, list(&found.clutter)));
            }
            if !found.fonts.is_empty() {
                out.push(one(Check::EmbeddedFonts, list(&found.fonts)));
            }
            if !read.damaged_pages.is_empty() {
                out.push(one(Check::DamagedPages, list(&read.damaged_pages)));
            }
        }
    }
    match read.cover_size {
        None => out.push(one(Check::NoCover, String::new())),
        Some((w, h)) if CoverQuality::of_height(h) == CoverQuality::Low => {
            out.push(one(Check::SmallCover, format!("{w}×{h}")));
        }
        Some(_) => {}
    }
    if book.authors.is_empty()
        || book
            .authors
            .iter()
            .all(|a| a.trim().eq_ignore_ascii_case("unknown"))
    {
        out.push(one(Check::NoAuthor, String::new()));
    }
    if book.series.is_some() && book.series_index.is_none() {
        out.push(one(
            Check::SeriesWithoutNumber,
            book.series.clone().unwrap_or_default(),
        ));
    }
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
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a whole number in 1..=10000"
        )]
        series
            .entry(name.to_lowercase())
            .or_insert_with(|| (name.clone(), Vec::new()))
            .1
            .push((n as u32, b.id));
    }
    series
        .into_values()
        .filter_map(|(name, mut members)| {
            members.sort_unstable();
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
            published: None,
            description: None,
            identifiers: vec![],
            words: None,
            pages: None,
            pages_estimated: true,
            original_name: "b.epub".into(),
            file_size: 1,
            format: erc_library::Format::Epub,
            kind: erc_library::Kind::Book,
            direction: erc_library::Direction::Ltr,
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
