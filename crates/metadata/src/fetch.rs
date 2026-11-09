//! Getting responses: over HTTP with per-host pacing, through a SQLite cache
//! so re-runs are quick and work offline, or (in tests) from recordings.

use std::collections::HashMap;
use std::path::Path;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as Age, Utc};
use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::{RETRY_AFTER, USER_AGENT as USER_AGENT_HEADER};
use rusqlite::{Connection, OptionalExtension, params};
use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("{0}")]
    Http(String),
    #[error("{host} returned HTTP {status}")]
    Status { host: String, status: u16 },
    #[error("{host} is limiting requests; try again later")]
    RateLimited { host: String },
}

/// One GET returning the body as text.
pub trait Fetch: Sync {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError>;

    fn get_authenticated(
        &self,
        url: &str,
        params: &[(&str, &str)],
        token: &str,
    ) -> Result<String, FetchError> {
        let _ = (url, params, token);
        Err(FetchError::Http(
            "authenticated requests are unavailable".into(),
        ))
    }
}

/// Sent to every service.
pub const USER_AGENT: &str = concat!(
    "E-Reader-Companion/",
    env!("CARGO_PKG_VERSION"),
    " (personal e-book manager)"
);

/// Sent only to services whose terms ask for a way to reach the developer:
/// Open Library allows 3 requests a second with it, and Wikimedia may block
/// clients without it.
const CONTACT_USER_AGENT: &str = concat!(
    "E-Reader-Companion/",
    env!("CARGO_PKG_VERSION"),
    " (personal e-book manager; jackson.ferguson0@gmail.com)"
);

/// The User-Agent for a request to `url`.
pub fn user_agent(url: &str) -> &'static str {
    let host = host_of(url);
    if host == "query.wikidata.org"
        || host == "openlibrary.org"
        || host.ends_with(".openlibrary.org")
    {
        CONTACT_USER_AGENT
    } else {
        USER_AGENT
    }
}

fn host_of(url: &str) -> &str {
    url.split("://")
        .nth(1)
        .and_then(|r| r.split('/').next())
        .unwrap_or_default()
}

/// Minimum gap between requests to one host, following each service's
/// published limits (Apple's search API allows about 20 a minute).
fn min_interval(host: &str) -> Duration {
    match host {
        "itunes.apple.com" | "metron.cloud" => Duration::from_millis(3100),
        "query.wikidata.org" => Duration::from_millis(500),
        "openlibrary.org" => Duration::from_millis(350),
        "mangainfo.com" => Duration::from_millis(1100),
        _ => Duration::from_millis(100),
    }
}

/// How long a host is left alone when it limits us without saying for how long.
const DEFAULT_PAUSE: Duration = Duration::from_secs(60);
/// The longest a host is left alone, whatever it asks for.
const MAX_PAUSE: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy)]
struct Slot {
    /// When the host may next be asked.
    next: Instant,
    /// Requests fail straight away until then.
    paused_until: Option<Instant>,
}

/// When each host may next be asked.
#[derive(Debug, Default)]
struct Pacer {
    hosts: HashMap<String, Slot>,
}

impl Pacer {
    /// Takes the host's next turn: how long to wait for it, or `None` while
    /// the host is limiting us.
    fn reserve(&mut self, host: &str, now: Instant) -> Option<Duration> {
        let slot = self.hosts.entry(host.to_owned()).or_insert(Slot {
            next: now,
            paused_until: None,
        });
        if slot.paused_until.is_some_and(|until| until > now) {
            return None;
        }
        let at = slot.next.max(now);
        slot.next = at + min_interval(host);
        Some(at - now)
    }

    fn pause(&mut self, host: &str, now: Instant, wait: Duration) {
        let until = now + wait.min(MAX_PAUSE);
        self.hosts
            .entry(host.to_owned())
            .and_modify(|s| s.paused_until = Some(until))
            .or_insert(Slot {
                next: now,
                paused_until: Some(until),
            });
    }
}

/// Shared by everything in the process, because the limits are per client:
/// metadata lookups and cover searches both spend Apple's 20 a minute.
static PACER: LazyLock<Mutex<Pacer>> = LazyLock::new(Mutex::default);

fn pacer() -> std::sync::MutexGuard<'static, Pacer> {
    PACER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Waits until `url`'s host may be asked. Call before each request.
pub fn wait_turn(url: &str) -> Result<(), FetchError> {
    let host = host_of(url);
    let wait = pacer().reserve(host, Instant::now());
    match wait {
        Some(wait) => {
            std::thread::sleep(wait);
            Ok(())
        }
        None => Err(FetchError::RateLimited {
            host: host.to_owned(),
        }),
    }
}

/// Whether a response means the host wants fewer requests. Apple answers
/// 403 rather than 429.
fn is_limiting(host: &str, status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS
        || (host == "itunes.apple.com" && status == StatusCode::FORBIDDEN)
}

/// A `Retry-After` value in seconds; the date form falls back to the default.
fn pause_length(retry_after: Option<&str>) -> Duration {
    retry_after
        .and_then(|v| v.trim().parse().ok())
        .map_or(DEFAULT_PAUSE, Duration::from_secs)
}

/// Call with each response: an error when the host is limiting us, and it's
/// then left alone for as long as it asked.
pub fn check_limited(url: &str, response: &Response) -> Result<(), FetchError> {
    let host = host_of(url);
    if !is_limiting(host, response.status()) {
        return Ok(());
    }
    let retry_after = response
        .headers()
        .get(RETRY_AFTER)
        .and_then(|v| v.to_str().ok());
    pacer().pause(host, Instant::now(), pause_length(retry_after));
    Err(FetchError::RateLimited {
        host: host.to_owned(),
    })
}

pub struct HttpFetcher {
    client: Client,
}

impl HttpFetcher {
    pub fn new() -> Result<Self, FetchError> {
        Ok(Self {
            client: Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .map_err(|e| FetchError::Http(e.to_string()))?,
        })
    }
}

impl HttpFetcher {
    fn request(
        &self,
        url: &str,
        params: &[(&str, &str)],
        token: Option<&str>,
    ) -> Result<String, FetchError> {
        wait_turn(url)?;
        let mut request = self
            .client
            .get(url)
            .query(params)
            .header(USER_AGENT_HEADER, user_agent(url));
        request = request.header(
            "Accept",
            match host_of(url) {
                "query.wikidata.org" => "application/sparql-results+json",
                _ => "application/json",
            },
        );
        if let Some(token) = token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .map_err(|e| FetchError::Http(e.to_string()))?;
        check_limited(url, &response)?;
        if !response.status().is_success() {
            return Err(FetchError::Status {
                host: host_of(url).into(),
                status: response.status().as_u16(),
            });
        }
        response.text().map_err(|e| FetchError::Http(e.to_string()))
    }
}

impl Fetch for HttpFetcher {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError> {
        self.request(url, params, None)
    }

    fn get_authenticated(
        &self,
        url: &str,
        params: &[(&str, &str)],
        token: &str,
    ) -> Result<String, FetchError> {
        self.request(url, params, Some(token))
    }
}

/// How long a cached response is used.
const CACHE_DAYS: i64 = 30;

/// Caches successful responses in a SQLite file; failures aren't cached.
pub struct CachedFetcher<F> {
    inner: F,
    db: Mutex<Connection>,
    manga: Mutex<HashMap<String, (DateTime<Utc>, String)>>,
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
            manga: Mutex::default(),
        })
    }

    fn db(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.db
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl<F: Fetch> CachedFetcher<F> {
    fn request(
        &self,
        url: &str,
        params: &[(&str, &str)],
        token: Option<&str>,
    ) -> Result<String, FetchError> {
        let mut key = url.to_owned();
        if let Some(token) = token {
            use std::fmt::Write as _;
            key.push_str("\naccount=");
            for byte in Sha256::digest(token.as_bytes()) {
                let _ = write!(key, "{byte:02x}");
            }
        }
        for (k, v) in params {
            key.push_str(&format!("\n{k}={v}"));
        }
        let now = Utc::now();
        if host_of(url) == "mangainfo.com" {
            // Mangainfo permits brief caching. Keep responses in memory for
            // this batch, respecting HTTP max-age and the UTC day boundary.
            let oldest = (now - Age::minutes(5))
                .max(now.date_naive().and_time(chrono::NaiveTime::MIN).and_utc());
            let mut manga = self
                .manga
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            manga.retain(|_, (fetched, _)| *fetched > oldest);
            if let Some((_, body)) = manga.get(&key) {
                return Ok(body.clone());
            }
            let body = self.inner.get(url, params)?;
            manga.insert(key, (Utc::now(), body.clone()));
            return Ok(body);
        }
        let oldest = (now - Age::days(CACHE_DAYS)).to_rfc3339();
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
        let body = match token {
            Some(token) => self.inner.get_authenticated(url, params, token)?,
            None => self.inner.get(url, params)?,
        };
        let _ = self.db().execute(
            "INSERT OR REPLACE INTO responses (key, body, fetched_at) VALUES (?1, ?2, ?3)",
            params![key, body, Utc::now().to_rfc3339()],
        );
        Ok(body)
    }
}

impl<F: Fetch> Fetch for CachedFetcher<F> {
    fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError> {
        self.request(url, params, None)
    }

    fn get_authenticated(
        &self,
        url: &str,
        params: &[(&str, &str)],
        token: &str,
    ) -> Result<String, FetchError> {
        self.request(url, params, Some(token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_services_that_ask_get_the_contact_address() {
        for url in [
            "https://openlibrary.org/search.json",
            "https://covers.openlibrary.org/b/id/1-L.jpg",
            "https://query.wikidata.org/sparql",
        ] {
            assert!(user_agent(url).contains('@'), "{url}");
        }
        for url in [
            "https://itunes.apple.com/search",
            "https://www.googleapis.com/books/v1/volumes",
            "https://m.media-amazon.com/images/P/1.jpg",
            "https://www.indigo.ca/search/suggest.json",
            "https://notopenlibrary.org/",
        ] {
            assert!(!user_agent(url).contains('@'), "{url}");
        }
    }

    #[test]
    fn turns_are_spaced_per_host() {
        let mut pacer = Pacer::default();
        let now = Instant::now();
        let gap = min_interval("itunes.apple.com");
        assert_eq!(pacer.reserve("itunes.apple.com", now), Some(Duration::ZERO));
        assert_eq!(pacer.reserve("itunes.apple.com", now), Some(gap));
        assert_eq!(pacer.reserve("itunes.apple.com", now), Some(gap * 2));
        assert_eq!(pacer.reserve("openlibrary.org", now), Some(Duration::ZERO));
        assert_eq!(
            pacer.reserve("itunes.apple.com", now + gap * 3),
            Some(Duration::ZERO)
        );
    }

    #[test]
    fn a_paused_host_refuses_turns_until_the_pause_ends() {
        let mut pacer = Pacer::default();
        let now = Instant::now();
        pacer.pause("query.wikidata.org", now, Duration::from_secs(30));
        let later = |s| now + Duration::from_secs(s);
        assert_eq!(pacer.reserve("query.wikidata.org", later(29)), None);
        assert!(pacer.reserve("openlibrary.org", later(29)).is_some());
        assert!(pacer.reserve("query.wikidata.org", later(30)).is_some());
    }

    #[test]
    fn a_pause_is_capped() {
        let mut pacer = Pacer::default();
        let now = Instant::now();
        pacer.pause("a.example", now, Duration::from_hours(24));
        assert!(pacer.reserve("a.example", now + MAX_PAUSE).is_some());
    }

    #[test]
    fn apple_limits_with_403_and_everyone_with_429() {
        assert!(is_limiting("itunes.apple.com", StatusCode::FORBIDDEN));
        assert!(is_limiting(
            "openlibrary.org",
            StatusCode::TOO_MANY_REQUESTS
        ));
        assert!(!is_limiting("openlibrary.org", StatusCode::FORBIDDEN));
        assert!(!is_limiting("itunes.apple.com", StatusCode::NOT_FOUND));
    }

    #[test]
    fn retry_after_seconds_set_the_pause() {
        assert_eq!(pause_length(Some("120")), Duration::from_secs(120));
        assert_eq!(pause_length(Some(" 5 ")), Duration::from_secs(5));
        assert_eq!(
            pause_length(Some("Wed, 21 Oct 2026 07:28:00 GMT")),
            DEFAULT_PAUSE
        );
        assert_eq!(pause_length(None), DEFAULT_PAUSE);
    }
}
