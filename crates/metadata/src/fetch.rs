//! Getting responses: over HTTP with per-host pacing, through a SQLite cache
//! so re-runs are quick and work offline, or (in tests) from recordings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{Duration as Age, Utc};
use reqwest::blocking::Client;
use rusqlite::{Connection, OptionalExtension, params};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    Http(String),
    #[error("{host} is limiting requests; try again later")]
    RateLimited { host: String },
}

/// One GET returning the body as text.
pub trait Fetch: Sync {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError>;
}

const USER_AGENT: &str = concat!(
    "E-Reader-Companion/",
    env!("CARGO_PKG_VERSION"),
    " (personal e-book manager)"
);

/// Minimum gap between requests to one host, following each service's
/// published limits (Apple's search API allows about 20 a minute).
fn min_interval(host: &str) -> Duration {
    match host {
        "itunes.apple.com" => Duration::from_millis(3100),
        "query.wikidata.org" => Duration::from_millis(500),
        "openlibrary.org" => Duration::from_millis(350),
        _ => Duration::from_millis(100),
    }
}

pub struct HttpFetcher {
    client: Client,
    /// When each host may next be asked.
    next: Mutex<HashMap<String, Instant>>,
}

impl HttpFetcher {
    pub fn new() -> Result<Self, FetchError> {
        Ok(Self {
            client: Client::builder()
                .user_agent(USER_AGENT)
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|e| FetchError::Http(e.to_string()))?,
            next: Mutex::new(HashMap::new()),
        })
    }

    fn wait_turn(&self, host: &str) {
        let wait = {
            let mut next = self
                .next
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let now = Instant::now();
            let at = next.get(host).copied().unwrap_or(now).max(now);
            next.insert(host.to_owned(), at + min_interval(host));
            at - now
        };
        std::thread::sleep(wait);
    }
}

impl Fetch for HttpFetcher {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError> {
        let host = url
            .split("://")
            .nth(1)
            .and_then(|r| r.split('/').next())
            .unwrap_or_default()
            .to_owned();
        self.wait_turn(&host);
        let mut request = self.client.get(url).query(params);
        if host == "query.wikidata.org" {
            request = request.header("Accept", "application/sparql-results+json");
        }
        let response = request
            .send()
            .map_err(|e| FetchError::Http(e.to_string()))?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(FetchError::RateLimited { host });
        }
        response
            .error_for_status()
            .and_then(reqwest::blocking::Response::text)
            .map_err(|e| FetchError::Http(e.to_string()))
    }
}

/// How long a cached response is used.
const CACHE_DAYS: i64 = 30;

/// Caches successful responses in a SQLite file; failures aren't cached.
pub struct CachedFetcher<F> {
    inner: F,
    db: Mutex<Connection>,
}

impl<F: Fetch> CachedFetcher<F> {
    pub fn open(inner: F, path: &Path) -> rusqlite::Result<Self> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "CREATE TABLE IF NOT EXISTS responses (
                key        TEXT PRIMARY KEY,
                body       TEXT NOT NULL,
                fetched_at TEXT NOT NULL
            );",
        )?;
        Ok(Self {
            inner,
            db: Mutex::new(db),
        })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<F: Fetch> Fetch for CachedFetcher<F> {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError> {
        let mut key = url.to_owned();
        for (k, v) in params {
            key.push_str(&format!("\n{k}={v}"));
        }
        let oldest = (Utc::now() - Age::days(CACHE_DAYS)).to_rfc3339();
        let cached: Option<String> = self
            .db()
            .query_row(
                "SELECT body FROM responses WHERE key = ?1 AND fetched_at > ?2",
                params![key, oldest],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if let Some(body) = cached {
            return Ok(body);
        }
        let body = self.inner.get(url, params)?;
        let _ = self.db().execute(
            "INSERT OR REPLACE INTO responses (key, body, fetched_at) VALUES (?1, ?2, ?3)",
            params![key, body, Utc::now().to_rfc3339()],
        );
        Ok(body)
    }
}
