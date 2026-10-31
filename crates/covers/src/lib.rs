//! Finding better covers online. Providers are queried in parallel and any
//! that fail are skipped, so one slow or broken service never blocks the rest.
//! Response parsing is kept in pure functions so it can be tested offline.

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
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    pub source: &'static str,
    /// Largest image the provider offers.
    pub url: String,
    /// A smaller image for the picker grid.
    pub preview_url: String,
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

/// Queries every provider in parallel; Apple first, since its images are
/// usually the largest.
pub fn search(query: &CoverQuery) -> Vec<Candidate> {
    let Ok(client) = client() else {
        return Vec::new();
    };
    let providers: [Provider; 3] = [apple_books, open_library, google_books];
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
    match &q.author {
        Some(author) => format!("{} {author}", q.title),
        None => q.title.clone(),
    }
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
    Ok(parse_apple_books(&json))
}

/// Apple's artwork URLs end in a size such as `100x100bb.jpg`; asking for a
/// bigger size returns up to the original resolution.
pub fn parse_apple_books(json: &Value) -> Vec<Candidate> {
    results(json, "results")
        .filter_map(|r| r.get("artworkUrl100")?.as_str())
        .filter(|u| u.contains("100x100"))
        .map(|u| Candidate {
            source: "Apple Books",
            url: u.replace("100x100", "3000x3000"),
            preview_url: u.replace("100x100", "400x400"),
        })
        .collect()
}

fn open_library(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let mut params = vec![
        ("title", q.title.as_str()),
        ("limit", "8"),
        ("fields", "cover_i"),
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
    found.extend(parse_open_library(&json));
    Ok(found)
}

pub fn parse_open_library(json: &Value) -> Vec<Candidate> {
    results(json, "docs")
        .filter_map(|d| d.get("cover_i")?.as_i64())
        .map(|id| Candidate {
            source: "Open Library",
            url: format!("https://covers.openlibrary.org/b/id/{id}-L.jpg"),
            preview_url: format!("https://covers.openlibrary.org/b/id/{id}-M.jpg"),
        })
        .collect()
}

fn google_books(client: &Client, q: &CoverQuery) -> Result<Vec<Candidate>> {
    let mut terms = format!("intitle:{}", q.title);
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
    Ok(parse_google_books(&json))
}

/// Google only exposes small thumbnails through the public API.
pub fn parse_google_books(json: &Value) -> Vec<Candidate> {
    results(json, "items")
        .filter_map(|i| {
            let links = i.get("volumeInfo")?.get("imageLinks")?;
            links
                .get("thumbnail")
                .or_else(|| links.get("smallThumbnail"))?
                .as_str()
        })
        .map(|u| {
            let url = u
                .replacen("http://", "https://", 1)
                .replace("&edge=curl", "");
            Candidate {
                source: "Google Books",
                preview_url: url.clone(),
                url,
            }
        })
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
        assert_eq!(
            parse_apple_books(&j),
            [Candidate {
                source: "Apple Books",
                url: "https://is1-ssl.mzstatic.com/image/thumb/a/b/3000x3000bb.jpg".into(),
                preview_url: "https://is1-ssl.mzstatic.com/image/thumb/a/b/400x400bb.jpg".into(),
            }]
        );
    }

    #[test]
    fn parses_open_library() {
        let j = json!({"docs": [{"cover_i": 12345}, {"title": "no cover"}]});
        let c = parse_open_library(&j);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].url, "https://covers.openlibrary.org/b/id/12345-L.jpg");
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
            c[0].url,
            "https://books.google.com/books/content?id=x&zoom=1&source=gbs_api"
        );
    }

    #[test]
    fn tolerates_unexpected_json() {
        assert!(parse_apple_books(&json!({"results": "nope"})).is_empty());
        assert!(parse_open_library(&json!(null)).is_empty());
        assert!(parse_google_books(&json!({"items": [1, 2]})).is_empty());
    }

    /// Hits the real services: `cargo test -p erc-covers -- --ignored`.
    #[test]
    #[ignore]
    fn live_search_finds_covers() {
        let found = search(&CoverQuery {
            title: "Dune".into(),
            author: Some("Frank Herbert".into()),
            isbn: None,
        });
        for c in &found {
            eprintln!("{:<13} {}", c.source, c.url);
        }
        assert!(!found.is_empty());
        let image = download(&found[0].url).unwrap();
        assert!(image.len() > 10_000);
    }
}
