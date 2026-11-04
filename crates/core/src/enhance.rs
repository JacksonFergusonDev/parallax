//! "Enhance and organise": proposing series and genres for library books,
//! and applying the ones the user accepts.
//!
//! Proposals only fill gaps: a series when the book has none (or its number
//! when only that is missing) and genres the book isn't already tagged with.
//! Local clues come first: the series Nickel shows on a connected Kobo and
//! the Kobo folder a book sits in ("KEpub/Fantasy/…"). Then the online
//! sources in `erc-metadata`. Genres always use the standard names
//! ("Science Fiction", never "Sci-Fi"), so every source fills the same tag
//! and collections made from them don't split.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use erc_device::DeviceBook;
use erc_device_kobo::KoboDevice;
use erc_library::{Book, Library};
use erc_metadata::genres;

use crate::genres::folder_genre;
use erc_metadata::{CachedFetcher, Confidence, Fetch, HttpFetcher, Lookup, Options, Query};
use serde::{Deserialize, Serialize};

/// Library setting holding the user's Google Books API key.
pub const GOOGLE_BOOKS_KEY: &str = "google_books_key";
/// Lookups run for this many books at once; each service is paced anyway.
const WORKERS: usize = 4;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProposedSeries {
    pub name: String,
    pub index: Option<String>,
    /// Where it came from: "Kobo", "Wikidata"…
    pub source: String,
    /// Selected by default.
    pub confident: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProposedGenre {
    pub name: String,
    pub sources: Vec<String>,
    pub confident: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Proposal {
    pub book_id: i64,
    pub series: Option<ProposedSeries>,
    pub genres: Vec<ProposedGenre>,
    /// Sources that couldn't be asked.
    pub problems: Vec<String>,
}

impl Proposal {
    pub fn is_empty(&self) -> bool {
        self.series.is_none() && self.genres.is_empty()
    }
}

/// What the user accepted for one book.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Accepted {
    pub book_id: i64,
    pub series: Option<(String, Option<String>)>,
    pub genres: Vec<String>,
}

/// What a connected Kobo knows about library books, by library id.
#[derive(Debug, Default)]
pub struct DeviceClues {
    /// (series, number) Nickel shows.
    series: HashMap<i64, (String, Option<String>)>,
    /// The genre the book's folder names, by its standard name.
    folder: HashMap<i64, &'static str>,
}

impl DeviceClues {
    pub fn read(library: &Library, device: &KoboDevice) -> erc_device_kobo::Result<Self> {
        let books = device.open_db()?.books()?;
        let copies = library
            .device_copies(&device.version().serial)
            .unwrap_or_default();
        let by_content: HashMap<&str, i64> = copies
            .iter()
            .map(|c| (c.content_id.as_str(), c.book_id))
            .collect();
        Ok(Self::from_books(&books, &by_content))
    }

    fn from_books(books: &[DeviceBook], library_ids: &HashMap<&str, i64>) -> Self {
        let mut clues = Self::default();
        for book in books {
            let folder = book.path.as_deref().and_then(folder_genre);
            let Some(&id) = library_ids.get(book.id.as_str()) else {
                continue;
            };
            if let Some(series) = &book.series {
                clues
                    .series
                    .insert(id, (series.clone(), book.series_number.clone()));
            }
            if let Some(genre) = folder {
                clues.folder.insert(id, genre);
            }
        }
        clues
    }
}

/// The standard genre of a tag, or the tag itself, for comparing.
fn genre_key(tag: &str) -> String {
    genres::from_label(tag).map_or_else(|| tag.to_lowercase(), |g| g.name.to_lowercase())
}

/// The online fetcher, caching responses beside the library for 30 days.
pub fn fetcher(library: &Library) -> Result<CachedFetcher<HttpFetcher>, String> {
    let http = HttpFetcher::new().map_err(|e| e.to_string())?;
    CachedFetcher::open(http, &library.root().join("metadata-cache.sqlite"))
        .map_err(|e| e.to_string())
}

/// Builds the proposal for one book from local clues and a lookup.
pub fn proposal(book: &Book, clues: &DeviceClues, lookup: Option<&Lookup>) -> Proposal {
    let mut out = Proposal {
        book_id: book.id,
        series: None,
        genres: Vec::new(),
        problems: lookup.map(|l| l.problems.clone()).unwrap_or_default(),
    };

    let online_series = lookup
        .and_then(|l| l.series.as_ref())
        .map(|s| ProposedSeries {
            name: s.name.clone(),
            index: s.index.clone(),
            source: s.source.label().to_owned(),
            confident: s.confidence == Confidence::High,
        });
    let kobo_series = clues
        .series
        .get(&book.id)
        .map(|(name, index)| ProposedSeries {
            name: name.clone(),
            index: index.clone(),
            source: "Kobo".to_owned(),
            confident: true,
        });
    out.series = match &book.series {
        None => kobo_series.or(online_series),
        // Same series, number missing: propose just the number.
        Some(current) if book.series_index.is_none() => kobo_series
            .into_iter()
            .chain(online_series)
            .find(|s| s.name.eq_ignore_ascii_case(current) && s.index.is_some()),
        Some(_) => None,
    };

    let mut have: Vec<String> = book.tags.iter().map(|t| genre_key(t)).collect();
    let mut add = |name: String, sources: Vec<String>, confident: bool| {
        let key = genre_key(&name);
        if !have.contains(&key) {
            have.push(key);
            out.genres.push(ProposedGenre {
                name,
                sources,
                confident,
            });
        }
    };
    if let Some(folder) = clues.folder.get(&book.id) {
        add((*folder).to_owned(), vec!["Kobo folder".to_owned()], true);
    }
    for g in lookup.map(|l| l.genres.as_slice()).unwrap_or_default() {
        add(
            g.name.clone(),
            g.sources.iter().map(|s| s.label().to_owned()).collect(),
            g.confidence == Confidence::High,
        );
    }
    out
}

/// Whether a book still needs looking up online: it lacks a series or any
/// genre tag.
fn needs_lookup(book: &Book) -> bool {
    book.series.is_none()
        || book.series_index.is_none()
        || !book.tags.iter().any(|t| genres::from_label(t).is_some())
}

/// Lookup options from the library's settings.
pub fn options(library: &Library) -> Options {
    Options {
        google_books_key: library.setting(GOOGLE_BOOKS_KEY).ok().flatten(),
    }
}

/// Proposals for `books`, looked up a few at a time. Takes copies of the
/// books so callers needn't hold the library while the network is slow.
/// `on_progress(done, total, title)` is called as each book finishes.
pub fn propose(
    books: &[Book],
    options: &Options,
    fetch: &dyn Fetch,
    clues: &DeviceClues,
    on_progress: impl Fn(usize, usize, &str) + Sync,
) -> Vec<Proposal> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Proposal>>> = Mutex::new(vec![None; books.len()]);
    std::thread::scope(|s| {
        for _ in 0..WORKERS.min(books.len()) {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some(book) = books.get(i) else {
                        break;
                    };
                    let lookup = needs_lookup(book).then(|| {
                        erc_metadata::lookup(
                            fetch,
                            &Query {
                                title: book.title.clone(),
                                authors: book.authors.clone(),
                                isbn: book.isbn().map(str::to_owned),
                            },
                            options,
                        )
                    });
                    let p = proposal(book, clues, lookup.as_ref());
                    results.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(p);
                    let n = done.fetch_add(1, Ordering::SeqCst) + 1;
                    on_progress(n, books.len(), &book.title);
                }
            });
        }
    });
    results
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .flatten()
        .collect()
}

/// Applies accepted changes; returns the updated books.
pub fn apply(library: &mut Library, accepted: &[Accepted]) -> erc_library::Result<Vec<Book>> {
    let mut updated = Vec::new();
    for a in accepted {
        if a.series.is_none() && a.genres.is_empty() {
            continue;
        }
        let book = library.book(a.book_id)?;
        let mut edit = book.edit();
        if let Some((name, index)) = &a.series {
            edit.series = Some(name.clone());
            edit.series_index = index.clone();
        }
        for g in &a.genres {
            if !edit.tags.iter().any(|t| t.eq_ignore_ascii_case(g)) {
                edit.tags.push(g.clone());
            }
        }
        updated.push(library.update(a.book_id, &edit)?);
    }
    Ok(updated)
}

#[cfg(test)]
mod tests {
    use erc_metadata::{GenreFound, SeriesFound, Source};

    use super::*;

    fn book(id: i64, series: Option<&str>, index: Option<&str>, tags: &[&str]) -> Book {
        Book {
            id,
            title: "T".into(),
            authors: vec!["A".into()],
            contributors: None,
            series: series.map(Into::into),
            series_index: index.map(Into::into),
            tags: tags.iter().map(|t| t.to_string()).collect(),
            language: None,
            publisher: None,
            description: None,
            identifiers: vec![],
            original_name: "t.epub".into(),
            file_size: 1,
            has_cover: false,
            custom_cover: false,
            added_at: String::new(),
            modified_at: String::new(),
        }
    }

    fn device_book(id: &str, path: &str, series: Option<&str>) -> DeviceBook {
        DeviceBook {
            id: id.into(),
            title: "T".into(),
            author: None,
            path: Some(path.into()),
            sideloaded: true,
            series: series.map(Into::into),
            series_number: series.map(|_| "3".into()),
            read_status: erc_device::ReadStatus::Unread,
            percent_read: 0,
            last_read: None,
            reading_time_secs: None,
            added: None,
            file_size: None,
            collections: vec![],
        }
    }

    fn lookup(series: Option<(&str, &str)>, genres: &[(&str, Confidence)]) -> Lookup {
        Lookup {
            series: series.map(|(name, index)| SeriesFound {
                name: name.into(),
                index: Some(index.into()),
                source: Source::Wikidata,
                confidence: Confidence::High,
            }),
            genres: genres
                .iter()
                .map(|(name, confidence)| GenreFound {
                    name: (*name).into(),
                    sources: vec![Source::OpenLibrary],
                    confidence: *confidence,
                })
                .collect(),
            problems: vec![],
        }
    }

    #[test]
    fn local_clues_win_and_genres_use_standard_names() {
        let devices = [
            device_book("a", "KEpub/Sci-Fi/a.kepub.epub", Some("Expanse")),
            device_book("b", "KEpub/Fantasy/b.kepub.epub", None),
            device_book("c", "KEpub/To read/c.kepub.epub", None),
        ];
        let ids: HashMap<&str, i64> = [("a", 1), ("b", 2), ("c", 3)].into();
        let clues = DeviceClues::from_books(&devices, &ids);

        let p = proposal(
            &book(1, None, None, &[]),
            &clues,
            Some(&lookup(
                Some(("Other", "9")),
                &[("Science Fiction", Confidence::High)],
            )),
        );
        assert_eq!(
            p.series,
            Some(ProposedSeries {
                name: "Expanse".into(),
                index: Some("3".into()),
                source: "Kobo".into(),
                confident: true
            })
        );
        // The "Sci-Fi" folder and the online genre are one genre, by its
        // standard name.
        let names: Vec<&str> = p.genres.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["Science Fiction"]);

        // A non-genre folder ("To read") isn't proposed.
        let p = proposal(
            &book(3, None, None, &[]),
            &clues,
            Some(&lookup(None, &[("Fantasy", Confidence::Low)])),
        );
        assert_eq!(p.series, None);
        assert_eq!(
            p.genres,
            [ProposedGenre {
                name: "Fantasy".into(),
                sources: vec!["Open Library".into()],
                confident: false
            }]
        );
    }

    #[test]
    fn only_gaps_are_filled() {
        let clues = DeviceClues::default();
        let found = lookup(
            Some(("Dune", "2")),
            &[("Science Fiction", Confidence::High)],
        );
        // Already has a series and a genre tag under another name.
        let full = book(1, Some("Dune"), Some("2"), &["sci-fi"]);
        assert!(!needs_lookup(&full));
        assert!(proposal(&full, &clues, Some(&found)).is_empty());
        // Same series without a number: only the number is proposed.
        let p = proposal(&book(2, Some("dune"), None, &[]), &clues, Some(&found));
        assert_eq!(p.series.unwrap().index.as_deref(), Some("2"));
        // A different series is never proposed over an existing one.
        let p = proposal(&book(3, Some("Other"), None, &[]), &clues, Some(&found));
        assert_eq!(p.series, None);
    }
}
