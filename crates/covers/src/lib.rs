//! Finding better covers online: Amazon (by ASIN or ISBN-10), Apple Books,
//! Indigo, Open Library and Google Books. Providers are queried in parallel and any
//! that fail are skipped, so one slow or broken service never blocks the rest.
//! Response parsing is kept in pure functions so it can be tested offline.
//! Results from a title search are kept only when they are this book (same
//! title and first author's surname), so a store that lacks it can't fill the
//! picker with whatever else matched a word.

use std::io::Read;
use std::time::Duration;

use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("{0}")]
    NotAnImage(String),
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Default)]
pub struct CoverQuery {
    pub title: String,
    pub author: Option<String>,
    pub isbn: Option<String>,
    /// Amazon's id, e.g. calibre's `mobi-asin` from a Kindle file.
    pub asin: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub source: &'static str,
    /// Largest image the provider offers.
    pub url: String,
    /// A smaller image for the picker grid.
    pub preview_url: String,
}

/// A cover with the book the provider says it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Listing {
    pub title: String,
    pub authors: Vec<String>,
    pub candidate: Candidate,
}

/// Images larger than this are refused when downloading.
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const USER_AGENT: &str = concat!("E-Reader-Companion/", env!("CARGO_PKG_VERSION"));

fn client() -> Result<Client> {
    Ok(Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(12))
        .build()?)
}

type Provider = fn(&Client, &CoverQuery) -> Result<Vec<Candidate>>;

/// Queries every provider in parallel; Amazon and Apple first, since their
/// images are usually the largest.
pub fn search(query: &CoverQuery) -> Vec<Candidate> {
    let Ok(client) = client() else {
        return Vec::new();
    };
    let providers: [Provider; 5] = [amazon, apple_books, indigo, open_library, google_books];
    let results: Vec<Vec<Candidate>> = std::thread::scope(|s| {
        let handles: Vec<_> = providers
            .iter()
            .map(|provider| s.spawn(|| provider(&client, query).unwrap_or_default()))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let mut seen = std::collections::HashSet::new();
    results
        .into_iter()
        .flatten()
        .filter(|c| seen.insert(c.url.clone()))
        .collect()
}

/// Downloads a chosen cover, refusing non-images and oversized responses.
pub fn download(url: &str) -> Result<Vec<u8>> {
    let response = client()?.get(url).send()?.error_for_status()?;
    let is_image = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|ct| ct.starts_with("image/"));
    if !is_image {
        return Err(Error::NotAnImage(format!("{url} did not return an image")));
    }
    let mut data = Vec::new();
    response
        .take(MAX_IMAGE_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| Error::NotAnImage(e.to_string()))?;
    if data.len() as u64 > MAX_IMAGE_BYTES {
        return Err(Error::NotAnImage("image is too large".into()));
    }
    Ok(data)
}

fn get_json(client: &Client, url: &str, params: &[(&str, &str)]) -> Result<Value> {
    Ok(client
        .get(url)
        .query(params)
        .send()?
        .error_for_status()?
        .json()?)
}

fn search_terms(q: &CoverQuery) -> String {
    let title = erc_metadata::search_title(&q.title);
    match &q.author {
        Some(author) => format!("{title} {author}"),
        None => title,
    }
}

/// The candidates whose listing is this book.
pub fn this_book(q: &CoverQuery, listings: Vec<Listing>) -> Vec<Candidate> {
    let query = erc_metadata::Query {
        title: q.title.clone(),
        authors: q.author.iter().cloned().collect(),
        isbn: None,
    };
    listings
        .into_iter()
        .filter(|l| {
            let hit = erc_metadata::Hit {
                title: l.title.clone(),
                authors: l.authors.clone(),
                ..erc_metadata::Hit::default()
            };
            erc_metadata::match_confidence(&query, &hit).is_some()
        })
        .map(|l| l.candidate)
        .collect()
}

/// Amazon serves a book's original cover image by its ASIN, which for a
/// printed book is its ISBN-10. No search: without either there's nothing to
/// ask. An unknown id returns a 43-byte GIF instead of an error, so each one
/// is checked first.
fn amazon(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let mut found = Vec::new();
    for id in amazon_ids(q) {
        let url = format!("https://m.media-amazon.com/images/P/{id}.01._SCRM_.jpg");
        let response = client.head(&url).send()?;
        let is_cover = response.status().is_success()
            && response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .is_some_and(|v| v.as_bytes().starts_with(b"image/jpeg"))
            && response.content_length().is_none_or(|n| n > 1000);
        if is_cover {
            found.push(Candidate {
                source: "Amazon",
                preview_url: format!("https://m.media-amazon.com/images/P/{id}.01.LZZZZZZZ.jpg"),
                url,
            });
        }
    }
    Ok(found)
}

/// The ids Amazon might know the book by: its ASIN, then its ISBN as an
/// ISBN-10.
pub fn amazon_ids(q: &CoverQuery) -> Vec<String> {
    let asin = q
        .asin
        .as_deref()
        .map(|a| a.trim().to_ascii_uppercase())
        .filter(|a| a.len() == 10 && a.chars().all(|c| c.is_ascii_alphanumeric()));
    let isbn10 = q.isbn.as_deref().and_then(isbn10);
    let mut ids: Vec<String> = asin.into_iter().chain(isbn10).collect();
    ids.dedup();
    ids
}

/// An ISBN as ISBN-10, or `None` for ISBN-13s outside the 978 range (which
/// have no ISBN-10) and anything that isn't an ISBN.
pub fn isbn10(isbn: &str) -> Option<String> {
    let digits: String = isbn
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    match digits.len() {
        10 => Some(digits),
        13 if digits.starts_with("978") && digits.chars().all(|c| c.is_ascii_digit()) => {
            let core = &digits[3..12];
            let sum: u32 = core
                .chars()
                .zip((2..=10).rev())
                .map(|(c, w)| c.to_digit(10).unwrap_or(0) * w)
                .sum();
            let check = (11 - sum % 11) % 11;
            let check = if check == 10 {
                'X'
            } else {
                char::from_digit(check, 10)?
            };
            Some(format!("{core}{check}"))
        }
        _ => None,
    }
}

/// Indigo's Shopify storefront search. Its results have no author field, so
/// the author goes into the search and results are matched on title.
fn indigo(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let terms = match &q.isbn {
        Some(isbn) => isbn.clone(),
        None => search_terms(q),
    };
    let json = get_json(
        client,
        "https://www.indigo.ca/search/suggest.json",
        &[
            ("q", terms.as_str()),
            ("resources[type]", "product"),
            ("resources[limit]", "6"),
        ],
    )?;
    let listings = parse_indigo(&json);
    Ok(if q.isbn.is_some() {
        listings.into_iter().map(|l| l.candidate).collect()
    } else {
        let by_title = CoverQuery {
            author: None,
            ..q.clone()
        };
        this_book(&by_title, listings)
    })
}

pub fn parse_indigo(json: &Value) -> Vec<Listing> {
    json.pointer("/resources/results/products")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|p| {
            let image = p.get("image")?.as_str()?;
            let url = match image.strip_prefix("//") {
                Some(rest) => format!("https://{rest}"),
                None => image.to_owned(),
            };
            let sep = if url.contains('?') { '&' } else { '?' };
            Some(Listing {
                title: text(p, "title"),
                authors: Vec::new(),
                candidate: Candidate {
                    source: "Indigo",
                    preview_url: format!("{url}{sep}width=400"),
                    url,
                },
            })
        })
        .collect()
}

fn apple_books(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let json = get_json(
        client,
        "https://itunes.apple.com/search",
        &[
            ("term", &search_terms(q)),
            ("media", "ebook"),
            ("entity", "ebook"),
            ("limit", "8"),
        ],
    )?;
    Ok(this_book(q, parse_apple_books(&json)))
}

/// Apple's artwork URLs end in a size such as `100x100bb.jpg`; asking for a
/// bigger size returns up to the original resolution.
pub fn parse_apple_books(json: &Value) -> Vec<Listing> {
    results(json, "results")
        .filter_map(|r| {
            let u = r.get("artworkUrl100")?.as_str()?;
            u.contains("100x100").then(|| Listing {
                title: text(r, "trackName"),
                authors: text(r, "artistName")
                    .split(" & ")
                    .map(str::to_owned)
                    .collect(),
                candidate: Candidate {
                    source: "Apple Books",
                    url: u.replace("100x100", "3000x3000"),
                    preview_url: u.replace("100x100", "400x400"),
                },
            })
        })
        .collect()
}

fn open_library(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let title = erc_metadata::search_title(&q.title);
    let mut params = vec![
        ("title", title.as_str()),
        ("limit", "8"),
        ("fields", "cover_i,title,author_name"),
    ];
    if let Some(author) = &q.author {
        params.push(("author", author));
    }
    let json = get_json(client, "https://openlibrary.org/search.json", &params)?;
    let mut found = Vec::new();
    if let Some(isbn) = &q.isbn {
        // `default=false` makes a missing cover a 404 instead of a blank image.
        found.push(Candidate {
            source: "Open Library",
            url: format!("https://covers.openlibrary.org/b/isbn/{isbn}-L.jpg?default=false"),
            preview_url: format!(
                "https://covers.openlibrary.org/b/isbn/{isbn}-M.jpg?default=false"
            ),
        });
    }
    found.extend(this_book(q, parse_open_library(&json)));
    Ok(found)
}

pub fn parse_open_library(json: &Value) -> Vec<Listing> {
    results(json, "docs")
        .filter_map(|d| {
            let id = d.get("cover_i")?.as_i64()?;
            Some(Listing {
                title: text(d, "title"),
                authors: texts(d, "author_name"),
                candidate: Candidate {
                    source: "Open Library",
                    url: format!("https://covers.openlibrary.org/b/id/{id}-L.jpg"),
                    preview_url: format!("https://covers.openlibrary.org/b/id/{id}-M.jpg"),
                },
            })
        })
        .collect()
}

fn google_books(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let mut terms = format!("intitle:{}", erc_metadata::search_title(&q.title));
    if let Some(author) = &q.author {
        terms.push_str(&format!(" inauthor:{author}"));
    }
    if let Some(isbn) = &q.isbn {
        terms = format!("isbn:{isbn}");
    }
    let json = get_json(
        client,
        "https://www.googleapis.com/books/v1/volumes",
        &[("q", &terms), ("maxResults", "8"), ("printType", "books")],
    )?;
    let listings = parse_google_books(&json);
    // An ISBN search already names the book.
    Ok(if q.isbn.is_some() {
        listings.into_iter().map(|l| l.candidate).collect()
    } else {
        this_book(q, listings)
    })
}

/// Google only exposes small thumbnails through the public API.
pub fn parse_google_books(json: &Value) -> Vec<Listing> {
    results(json, "items")
        .filter_map(|i| {
            let info = i.get("volumeInfo")?;
            let links = info.get("imageLinks")?;
            let url = links
                .get("thumbnail")
                .or_else(|| links.get("smallThumbnail"))?
                .as_str()?
                .replacen("http://", "https://", 1)
                .replace("&edge=curl", "");
            Some(Listing {
                title: text(info, "title"),
                authors: texts(info, "authors"),
                candidate: Candidate {
                    source: "Google Books",
                    preview_url: url.clone(),
                    url,
                },
            })
        })
        .collect()
}

fn text(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn texts(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn results<'a>(json: &'a Value, key: &str) -> impl Iterator<Item = &'a Value> {
    json.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_apple_books() {
        let j = json!({"results": [
            {"artworkUrl100": "https://is1-ssl.mzstatic.com/image/thumb/a/b/100x100bb.jpg"},
            {"trackName": "no artwork"}
        ]});
        let found = parse_apple_books(&j);
        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].candidate,
            Candidate {
                source: "Apple Books",
                url: "https://is1-ssl.mzstatic.com/image/thumb/a/b/3000x3000bb.jpg".into(),
                preview_url: "https://is1-ssl.mzstatic.com/image/thumb/a/b/400x400bb.jpg".into(),
            }
        );
    }

    #[test]
    fn parses_open_library() {
        let j = json!({"docs": [{"cover_i": 12345}, {"title": "no cover"}]});
        let c = parse_open_library(&j);
        assert_eq!(c.len(), 1);
        assert_eq!(
            c[0].candidate.url,
            "https://covers.openlibrary.org/b/id/12345-L.jpg"
        );
    }

    #[test]
    fn parses_google_books() {
        let j = json!({"items": [
            {"volumeInfo": {"imageLinks": {"thumbnail": "http://books.google.com/books/content?id=x&zoom=1&edge=curl&source=gbs_api"}}},
            {"volumeInfo": {}}
        ]});
        let c = parse_google_books(&j);
        assert_eq!(c.len(), 1);
        assert_eq!(
            c[0].candidate.url,
            "https://books.google.com/books/content?id=x&zoom=1&source=gbs_api"
        );
    }

    /// Apple lacks the English ebook (an Amazon exclusive), so a search
    /// returns other books that share a word, and a translation.
    #[test]
    fn keeps_only_this_book() {
        let j = json!({"results": [
            {"trackName": "We are Fat and We are Legion", "artistName": "Benjamin Duffy",
             "artworkUrl100": "https://x/1/100x100bb.jpg"},
            {"trackName": "Nous sommes Bob, T1 : Nous sommes Légion (Nous sommes Bob)",
             "artistName": "Dennis E. Taylor & Sébastien Baert", "artworkUrl100": "https://x/2/100x100bb.jpg"},
            {"trackName": "We Are Legion (We Are Bob)", "artistName": "Dennis E. Taylor",
             "artworkUrl100": "https://x/3/100x100bb.jpg"},
        ]});
        let q = CoverQuery {
            title: "We Are Legion (We Are Bob)".into(),
            author: Some("Dennis E. Taylor".into()),
            ..CoverQuery::default()
        };
        let kept = this_book(&q, parse_apple_books(&j));
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].url, "https://x/3/3000x3000bb.jpg");
        assert_eq!(search_terms(&q), "We Are Legion Dennis E. Taylor");
    }

    #[test]
    fn tolerates_unexpected_json() {
        assert!(parse_apple_books(&json!({"results": "nope"})).is_empty());
        assert!(parse_open_library(&json!(null)).is_empty());
        assert!(parse_google_books(&json!({"items": [1, 2]})).is_empty());
    }

    #[test]
    fn amazon_ids_from_asin_and_isbn() {
        assert_eq!(isbn10("978-0-441-17271-9").as_deref(), Some("0441172717"));
        assert_eq!(isbn10("9780306406157").as_deref(), Some("0306406152"));
        assert_eq!(isbn10("9781234567897").as_deref(), Some("123456789X"));
        assert_eq!(isbn10("979-10-90636-07-1"), None);
        assert_eq!(isbn10("0-441-17271-7").as_deref(), Some("0441172717"));
        assert_eq!(isbn10("not an isbn"), None);
        let q = CoverQuery {
            asin: Some("b0736185zl".into()),
            isbn: Some("9780441172719".into()),
            ..CoverQuery::default()
        };
        assert_eq!(amazon_ids(&q), ["B0736185ZL", "0441172717"]);
        let q = CoverQuery {
            asin: Some("0441172717".into()),
            isbn: Some("9780441172719".into()),
            ..CoverQuery::default()
        };
        assert_eq!(amazon_ids(&q), ["0441172717"]);
        assert!(amazon_ids(&CoverQuery::default()).is_empty());
    }

    #[test]
    fn parses_indigo_and_matches_on_title() {
        let j = json!({"resources": {"results": {"products": [
            {"title": "We Are Legion (We Are Bob): Bobiverse: Book 1",
             "image": "https://cdn.shopify.com/s/files/1/a.jpg?v=1"},
            {"title": "Wij zijn legio – wij zijn Bob", "image": "//cdn.shopify.com/b.jpg"},
            {"title": "No image"},
        ]}}});
        let found = parse_indigo(&j);
        assert_eq!(found.len(), 2);
        assert_eq!(found[1].candidate.url, "https://cdn.shopify.com/b.jpg");
        assert_eq!(
            found[0].candidate.preview_url,
            "https://cdn.shopify.com/s/files/1/a.jpg?v=1&width=400"
        );
        let q = CoverQuery {
            title: "We Are Legion (We Are Bob)".into(),
            ..CoverQuery::default()
        };
        let kept = this_book(&q, found);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].url, "https://cdn.shopify.com/s/files/1/a.jpg?v=1");
    }

    /// Hits the real services: `cargo test -p erc-covers -- --ignored`.
    #[test]
    #[ignore = "hits the real services"]
    fn live_search_finds_covers() {
        let bob = search(&CoverQuery {
            title: "We Are Legion (We Are Bob)".into(),
            author: Some("Dennis E. Taylor".into()),
            ..CoverQuery::default()
        });
        assert!(bob.iter().any(|c| c.source == "Indigo"));
        let worlds = search(&CoverQuery {
            title: "All These Worlds (Bobiverse Book 3)".into(),
            author: Some("Dennis E. Taylor".into()),
            asin: Some("B0736185ZL".into()),
            ..CoverQuery::default()
        });
        assert_eq!(worlds[0].source, "Amazon");
        assert!(download(&worlds[0].url).unwrap().len() > 100_000);
        let unknown = search(&CoverQuery {
            title: "zzqx nothing".into(),
            asin: Some("B000000000".into()),
            ..CoverQuery::default()
        });
        assert!(unknown.iter().all(|c| c.source != "Amazon"));
        let found = search(&CoverQuery {
            title: "Dune".into(),
            author: Some("Frank Herbert".into()),
            ..CoverQuery::default()
        });
        for c in &found {
            eprintln!("{:<13} {}", c.source, c.url);
        }
        assert!(!found.is_empty());
        let image = download(&found[0].url).unwrap();
        assert!(image.len() > 10_000);
    }
}
