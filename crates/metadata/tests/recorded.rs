//! Lookups against responses recorded from the real services on 2026-11-01
//! (tests/fixtures/metadata). No network.

use std::path::Path;
use std::sync::Mutex;

use parallax_metadata::{
    CachedFetcher, Confidence, Fetch, FetchError, Options, Query, Source, lookup,
};

/// Serves recordings by which book a request is about; logs requests.
struct Recorded {
    log: Mutex<Vec<String>>,
}

impl Recorded {
    fn new() -> Self {
        Self {
            log: Mutex::new(Vec::new()),
        }
    }

    fn hosts(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }
}

impl Fetch for Recorded {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError> {
        let source = match url {
            u if u.contains("wikidata") => "wikidata",
            u if u.contains("openlibrary") => "openlibrary",
            u if u.contains("itunes") => "apple",
            u => return Err(FetchError::Http(format!("unexpected {u}"))),
        };
        let text: String = params.iter().map(|(_, v)| *v).collect::<Vec<_>>().join(" ");
        let book = if text.contains("COUNT(?m)") {
            "sizes-color"
        } else if text.contains("Color of Magic") {
            "color"
        } else if text.contains("Red Seas") {
            "redseas"
        } else if text.contains("Endurance") {
            "endurance"
        } else {
            return Err(FetchError::Http(format!("no recording for {text}")));
        };
        self.log.lock().unwrap().push(format!("{source}-{book}"));
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures/metadata")
            .join(format!("{source}-{book}.json"));
        std::fs::read_to_string(&path).map_err(|e| FetchError::Http(e.to_string()))
    }
}

fn query(title: &str, author: &str) -> Query {
    Query {
        title: title.into(),
        authors: vec![author.into()],
        isbn: None,
    }
}

fn genres(l: &parallax_metadata::Lookup) -> Vec<&str> {
    l.genres.iter().map(|g| g.name.as_str()).collect()
}

#[test]
fn discworld_book_gets_the_main_series_and_two_genres() {
    let fetch = Recorded::new();
    let found = lookup(
        &fetch,
        &query("The Color of Magic", "Pratchett, Terry"),
        &Options::default(),
    );
    let series = found.series.as_ref().unwrap();
    // Discworld (63 books) over its sub-series Rincewind (8).
    assert_eq!(
        (
            series.name.as_str(),
            series.index.as_deref(),
            series.source,
            series.confidence
        ),
        ("Discworld", Some("1"), Source::Wikidata, Confidence::High)
    );
    let mut names = genres(&found);
    names.sort_unstable();
    assert_eq!(names, ["Fantasy", "Humor"]);
    assert!(found.problems.is_empty(), "{:?}", found.problems);
    // Apple is only asked when nobody else knew a genre.
    assert!(!fetch.hosts().iter().any(|h| h.starts_with("apple")));
}

#[test]
fn series_number_comes_from_wikidata() {
    let found = lookup(
        &Recorded::new(),
        &query("Red Seas Under Red Skies", "Scott Lynch"),
        &Options::default(),
    );
    let series = found.series.as_ref().unwrap();
    assert_eq!(
        (series.name.as_str(), series.index.as_deref()),
        ("Gentleman Bastard Sequence", Some("2"))
    );
    assert_eq!(genres(&found), ["Fantasy"]);
}

#[test]
fn nonfiction_without_series() {
    let found = lookup(
        &Recorded::new(),
        &query("Endurance", "Alfred Lansing"),
        &Options::default(),
    );
    // Wikidata only knows novels called Endurance by other authors.
    assert_eq!(found.series, None);
    assert_eq!(genres(&found)[0], "Travel & Exploration");
}

#[test]
fn failing_sources_are_reported_not_fatal() {
    struct Down;
    impl Fetch for Down {
        fn get(&self, url: &str, _: &[(&str, &str)]) -> Result<String, FetchError> {
            Err(FetchError::RateLimited {
                host: url.split('/').nth(2).unwrap_or_default().into(),
            })
        }
    }
    let found = lookup(&Down, &query("Dune", "Frank Herbert"), &Options::default());
    assert_eq!(found.series, None);
    assert!(found.genres.is_empty());
    assert_eq!(found.problems.len(), 3, "{:?}", found.problems);
}

#[test]
fn responses_are_cached() {
    struct Offline;
    impl Fetch for Offline {
        fn get(&self, _: &str, _: &[(&str, &str)]) -> Result<String, FetchError> {
            Err(FetchError::Http("offline".into()))
        }
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cache.sqlite");
    let q = query("Red Seas Under Red Skies", "Scott Lynch");
    let first = CachedFetcher::open(Recorded::new(), &path).unwrap();
    let a = lookup(&first, &q, &Options::default());
    // A second run is answered entirely from the cache.
    let second = CachedFetcher::open(Offline, &path).unwrap();
    assert_eq!(lookup(&second, &q, &Options::default()), a);
}

/// Hits the real services: `cargo test -p parallax-metadata -- --ignored`.
/// Titles with bracketed notes found nothing on Wikidata before they were
/// searched without them.
#[test]
#[ignore = "hits the real services"]
fn live_bracketed_titles_find_their_series() {
    let fetch = parallax_metadata::HttpFetcher::new().unwrap();
    for (title, number) in [
        ("We Are Legion (We Are Bob)", "1"),
        ("For We Are Many", "2"),
        ("All These Worlds (Bobiverse Book 3)", "3"),
    ] {
        let found = lookup(
            &fetch,
            &query(title, "Dennis E. Taylor"),
            &Options::default(),
        );
        eprintln!("{title}: {:?} {:?}", found.series, genres(&found));
        let series = found.series.expect(title);
        assert_eq!(series.index.as_deref(), Some(number), "{title}");
    }
}
