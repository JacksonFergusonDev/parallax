//! Filling in a book's series and genres from public sources (PLAN §7.4).
//!
//! Sources, each optional and independent (a failing one is noted and
//! skipped):
//! - Wikidata (SPARQL): series and number, genres;
//! - Open Library search: subjects;
//! - Google Books, with the user's API key: categories;
//! - Apple Books search: genres, asked only when the others found none
//!   because it allows about 20 requests a minute.
//!
//! Results are matched to the book by ISBN, or by title (subtitles,
//! "(Series #1)" and spelling variants like Color/Colour tolerated) plus the
//! first author's surname. Response parsing is in pure functions tested
//! against recorded responses; nothing here touches a library or device.

mod fetch;
pub mod genres;

use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;

pub use fetch::{CachedFetcher, Fetch, FetchError, HttpFetcher};

#[derive(Debug, Clone, Default)]
pub struct Query {
    pub title: String,
    pub authors: Vec<String>,
    pub isbn: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Options {
    /// Google Books is only asked with a key; without one its shared quota
    /// is usually exhausted.
    pub google_books_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Wikidata,
    OpenLibrary,
    GoogleBooks,
    AppleBooks,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Wikidata => "Wikidata",
            Source::OpenLibrary => "Open Library",
            Source::GoogleBooks => "Google Books",
            Source::AppleBooks => "Apple Books",
        }
    }
}

/// How sure the match to the book is: low-confidence results are shown but
/// not selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Low,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SeriesFound {
    pub name: String,
    pub index: Option<String>,
    pub source: Source,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GenreFound {
    pub name: String,
    /// Sources that suggested it.
    pub sources: Vec<Source>,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Lookup {
    pub series: Option<SeriesFound>,
    /// Best first; at most two specific genres, or one broad one.
    pub genres: Vec<GenreFound>,
    /// Sources that failed, with why.
    pub problems: Vec<String>,
}

/// One result from a source, before matching.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Hit {
    pub title: String,
    pub authors: Vec<String>,
    pub isbns: Vec<String>,
    /// (Wikidata id, name, number) of each series it's in.
    pub series: Vec<(String, String, Option<String>)>,
    pub subjects: Vec<String>,
}

/// Asks the sources about one book.
pub fn lookup(fetch: &dyn Fetch, query: &Query, options: &Options) -> Lookup {
    let mut out = Lookup::default();
    let ask = |source: Source| -> Result<Vec<Hit>, FetchError> {
        match source {
            Source::Wikidata => wikidata(fetch, query),
            Source::OpenLibrary => open_library(fetch, query),
            Source::GoogleBooks => match &options.google_books_key {
                Some(key) => google_books(fetch, query, key),
                None => Ok(Vec::new()),
            },
            Source::AppleBooks => apple_books(fetch, query),
        }
    };
    let first = [Source::Wikidata, Source::OpenLibrary, Source::GoogleBooks];
    let results: Vec<(Source, Result<Vec<Hit>, FetchError>)> = std::thread::scope(|s| {
        let handles: Vec<_> = first
            .iter()
            .map(|&source| (source, s.spawn(move || ask(source))))
            .collect();
        handles
            .into_iter()
            .map(|(source, h)| {
                let result = h
                    .join()
                    .unwrap_or_else(|_| Err(FetchError::Http("lookup failed".into())));
                (source, result)
            })
            .collect()
    });
    let mut matched: Vec<(Source, Confidence, Hit)> = Vec::new();
    for (source, result) in results {
        absorb(query, source, result, &mut matched, &mut out);
    }
    if !has_specific_genre(&matched) {
        let result = ask(Source::AppleBooks);
        absorb(query, Source::AppleBooks, result, &mut matched, &mut out);
    }

    out.series = pick_series(fetch, &matched);
    out.genres = pick_genres(&matched);
    out
}

/// Keeps the hits that are this book; notes a failed source.
fn absorb(
    query: &Query,
    source: Source,
    result: Result<Vec<Hit>, FetchError>,
    matched: &mut Vec<(Source, Confidence, Hit)>,
    out: &mut Lookup,
) {
    match result {
        Ok(hits) => matched.extend(
            hits.into_iter()
                .filter_map(|h| match_confidence(query, &h).map(|c| (source, c, h))),
        ),
        Err(e) => out.problems.push(format!("{}: {e}", source.label())),
    }
}

fn has_specific_genre(matched: &[(Source, Confidence, Hit)]) -> bool {
    matched
        .iter()
        .flat_map(|(_, _, h)| &h.subjects)
        .filter_map(|s| genres::classify(s))
        .any(|g| !g.broad)
}

/// The series with a book number from the best match; ties (a book in
/// "Discworld" and its sub-series "Rincewind") go to the larger series.
fn pick_series(fetch: &dyn Fetch, matched: &[(Source, Confidence, Hit)]) -> Option<SeriesFound> {
    let confidence = matched
        .iter()
        .filter(|(_, _, h)| !h.series.is_empty())
        .map(|(_, c, _)| *c)
        .max()?;
    let mut options: Vec<(String, String, Option<String>, Source)> = Vec::new();
    for (source, c, hit) in matched {
        if *c != confidence {
            continue;
        }
        for (id, name, index) in &hit.series {
            if !options.iter().any(|o| &o.0 == id) {
                options.push((id.clone(), name.clone(), index.clone(), *source));
            }
        }
    }
    let numbered: Vec<_> = options.iter().filter(|o| o.2.is_some()).collect();
    let pool = if numbered.is_empty() {
        options.iter().collect()
    } else {
        numbered
    };
    let best = if pool.len() > 1 {
        let ids: Vec<&str> = pool.iter().map(|o| o.0.as_str()).collect();
        let sizes = wikidata_series_sizes(fetch, &ids).unwrap_or_default();
        pool.into_iter()
            .max_by_key(|o| sizes.get(&o.0).copied().unwrap_or(0))?
    } else {
        pool.into_iter().next()?
    };
    Some(SeriesFound {
        name: best.1.clone(),
        index: best.2.clone(),
        source: best.3,
        // Without a number it's often a publisher's line ("Dover Thrift
        // Editions") rather than a story series.
        confidence: if best.2.is_some() {
            confidence
        } else {
            Confidence::Low
        },
    })
}

/// Genres suggested by the most sources: up to two specific ones, else the
/// broad Fiction / Nonfiction.
fn pick_genres(matched: &[(Source, Confidence, Hit)]) -> Vec<GenreFound> {
    // Genre → (sources, best confidence, subjects pointing to it).
    let mut votes: BTreeMap<&'static str, (Vec<Source>, Confidence, usize)> = BTreeMap::new();
    for (source, confidence, hit) in matched {
        for subject in &hit.subjects {
            let Some(genre) = genres::classify(subject) else {
                continue;
            };
            let entry = votes
                .entry(genre.name)
                .or_insert((Vec::new(), Confidence::Low, 0));
            if !entry.0.contains(source) {
                entry.0.push(*source);
            }
            entry.1 = entry.1.max(*confidence);
            entry.2 += 1;
        }
    }
    let rank = |broad: bool| {
        let mut list = votes
            .iter()
            .filter(|(name, _)| genres::by_name(name).is_some_and(|g| g.broad == broad))
            .map(|(name, (sources, confidence, mentions))| {
                let order = genres::GENRES.iter().position(|g| g.name == *name);
                let rank = (
                    std::cmp::Reverse(sources.len()),
                    std::cmp::Reverse(*mentions),
                    order,
                );
                let found = GenreFound {
                    name: (*name).to_owned(),
                    sources: sources.clone(),
                    confidence: *confidence,
                };
                (rank, found)
            })
            .collect::<Vec<_>>();
        // Most sources, then most subjects; the genre list's order breaks ties.
        list.sort_by_key(|(rank, _)| *rank);
        list.into_iter().map(|(_, g)| g).collect::<Vec<_>>()
    };
    let specific = rank(false);
    if specific.is_empty() {
        rank(true).into_iter().take(1).collect()
    } else {
        let top = specific[0].sources.len();
        // A runner-up needs as many sources as the leader, or two.
        specific
            .into_iter()
            .enumerate()
            .filter(|(i, g)| *i == 0 || g.sources.len() >= top.min(2))
            .map(|(_, g)| g)
            .take(2)
            .collect()
    }
}

// ---- Matching ----

fn alnum(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Ways a title might be written: whole, before and after a colon, all
/// without leading/trailing articles and bracketed series notes.
pub fn title_keys(title: &str) -> Vec<String> {
    let base = match title.find(['(', '[']) {
        Some(i) if i > 0 => &title[..i],
        _ => title,
    };
    let mut parts = vec![base.to_owned()];
    if let Some((before, after)) = base.split_once([':', ';']) {
        parts.push(before.to_owned());
        parts.push(after.to_owned());
    }
    parts
        .iter()
        .map(|p| {
            let lower = p.trim().to_lowercase();
            let mut t = lower.as_str();
            for article in ["the ", "a ", "an "] {
                t = t.strip_prefix(article).unwrap_or(t);
            }
            for article in [", the", ", a", ", an"] {
                t = t.strip_suffix(article).unwrap_or(t);
            }
            alnum(t)
        })
        .filter(|k| !k.is_empty())
        .collect()
}

/// The title to search for: without bracketed notes such as "(We Are Bob)"
/// or "(Bobiverse Book 3)", which make searches miss the book.
pub fn search_title(title: &str) -> String {
    let mut out = String::new();
    let mut depth = 0usize;
    for c in title.chars() {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    let cleaned = out
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace(" :", ":")
        .replace(" ;", ";")
        .replace(" ,", ",");
    let cleaned = cleaned.trim_end_matches([':', ';', ',', '-', ' ']);
    if cleaned.is_empty() {
        title.trim().to_owned()
    } else {
        cleaned.to_owned()
    }
}

/// Lowercase surname of the first author, after flipping "Last, First".
pub fn surname(author: &str) -> String {
    let name = match author.split_once(',') {
        Some((last, first)) if !first.trim().is_empty() => format!("{} {last}", first.trim()),
        _ => author.to_owned(),
    };
    name.split_whitespace()
        .last()
        .map(alnum)
        .unwrap_or_default()
}

fn similarity(a: &str, b: &str) -> f64 {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let longest = a.len().max(b.len());
    if longest == 0 {
        return 0.0;
    }
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for i in 1..=a.len() {
        let mut prev = row[0];
        row[0] = i;
        for j in 1..=b.len() {
            let cur = row[j];
            row[j] = (prev + usize::from(a[i - 1] != b[j - 1]))
                .min(row[j] + 1)
                .min(row[j - 1] + 1);
            prev = cur;
        }
    }
    1.0 - row[b.len()] as f64 / longest as f64
}

fn isbn_key(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(char::to_uppercase)
        .collect()
}

/// `None` when the hit isn't this book.
pub fn match_confidence(query: &Query, hit: &Hit) -> Option<Confidence> {
    if let Some(isbn) = query.isbn.as_deref().map(isbn_key)
        && hit.isbns.iter().any(|i| isbn_key(i) == isbn)
    {
        return Some(Confidence::High);
    }
    if let Some(author) = query.authors.first() {
        let wanted = surname(author);
        if !wanted.is_empty() && !hit.authors.iter().any(|a| surname(a) == wanted) {
            return None;
        }
    }
    let best = title_keys(&query.title)
        .iter()
        .flat_map(|q| {
            title_keys(&hit.title)
                .into_iter()
                .map(move |h| similarity(q, &h))
        })
        .fold(0.0, f64::max);
    if best >= 0.9 {
        Some(Confidence::High)
    } else if best >= 0.75 {
        Some(Confidence::Low)
    } else {
        None
    }
}

// ---- Sources ----

const WIKIDATA: &str = "https://query.wikidata.org/sparql";

fn sparql_string(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn wikidata(fetch: &dyn Fetch, q: &Query) -> Result<Vec<Hit>, FetchError> {
    let query = format!(
        r#"SELECT ?item ?itemLabel ?authorLabel ?series ?seriesLabel ?ordinal ?genreLabel WHERE {{
  SERVICE wikibase:mwapi {{
    bd:serviceParam wikibase:endpoint "www.wikidata.org"; wikibase:api "EntitySearch"; mwapi:search "{}"; mwapi:language "en".
    ?item wikibase:apiOutputItem mwapi:item.
  }}
  ?item wdt:P50 ?author.
  OPTIONAL {{ ?item p:P179 ?st. ?st ps:P179 ?series. OPTIONAL {{ ?st pq:P1545 ?ordinal }} }}
  OPTIONAL {{ ?item wdt:P136 ?genre }}
  SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en". }}
}} LIMIT 60"#,
        sparql_string(&search_title(&q.title))
    );
    let body = fetch.get(WIKIDATA, &[("query", &query), ("format", "json")])?;
    Ok(parse_wikidata(&json(&body)?))
}

/// One hit per Wikidata item, with all its authors, series and genres.
pub fn parse_wikidata(json: &Value) -> Vec<Hit> {
    let mut items: BTreeMap<String, Hit> = BTreeMap::new();
    let value = |b: &Value, k: &str| b.get(k)?.get("value")?.as_str().map(str::to_owned);
    for b in rows(json, &["results", "bindings"]) {
        let Some(item) = value(b, "item") else {
            continue;
        };
        let hit = items.entry(item).or_default();
        if let Some(title) = value(b, "itemLabel") {
            hit.title = title;
        }
        push_unique(&mut hit.authors, value(b, "authorLabel"));
        push_unique(&mut hit.subjects, value(b, "genreLabel"));
        if let (Some(id), Some(name)) = (value(b, "series"), value(b, "seriesLabel"))
            && !hit.series.iter().any(|s| s.0 == id)
        {
            hit.series.push((id, name, value(b, "ordinal")));
        }
    }
    items.into_values().collect()
}

fn wikidata_series_sizes(
    fetch: &dyn Fetch,
    ids: &[&str],
) -> Result<BTreeMap<String, u32>, FetchError> {
    let values: Vec<String> = ids
        .iter()
        .filter_map(|id| id.rsplit('/').next())
        .filter(|q| q.starts_with('Q') && q[1..].chars().all(|c| c.is_ascii_digit()))
        .map(|q| format!("wd:{q}"))
        .collect();
    // The label service works around a query-engine error on this shape.
    let query = format!(
        r#"SELECT ?series ?seriesLabel (COUNT(?m) AS ?size) WHERE {{ VALUES ?series {{ {} }} OPTIONAL {{ ?m wdt:P179 ?series }} SERVICE wikibase:label {{ bd:serviceParam wikibase:language "en". }} }} GROUP BY ?series ?seriesLabel"#,
        values.join(" ")
    );
    let body = fetch.get(WIKIDATA, &[("query", &query), ("format", "json")])?;
    Ok(parse_series_sizes(&json(&body)?))
}

pub fn parse_series_sizes(json: &Value) -> BTreeMap<String, u32> {
    rows(json, &["results", "bindings"])
        .filter_map(|b| {
            let id = b.get("series")?.get("value")?.as_str()?.to_owned();
            let size = b.get("size")?.get("value")?.as_str()?.parse().ok()?;
            Some((id, size))
        })
        .collect()
}

fn terms(q: &Query) -> String {
    match q.authors.first() {
        Some(author) => format!("{} {author}", search_title(&q.title)),
        None => search_title(&q.title),
    }
}

fn open_library(fetch: &dyn Fetch, q: &Query) -> Result<Vec<Hit>, FetchError> {
    let search = match &q.isbn {
        Some(isbn) => format!("isbn:{}", isbn_key(isbn)),
        None => terms(q),
    };
    let body = fetch.get(
        "https://openlibrary.org/search.json",
        &[
            ("q", &search),
            (
                "fields",
                "key,title,author_name,subject,isbn,first_publish_year,language",
            ),
            ("limit", "5"),
        ],
    )?;
    Ok(parse_open_library(&json(&body)?))
}

pub fn parse_open_library(json: &Value) -> Vec<Hit> {
    rows(json, &["docs"])
        .map(|d| Hit {
            title: str_at(d, "title"),
            authors: strings(d, "author_name"),
            isbns: strings(d, "isbn"),
            subjects: strings(d, "subject"),
            ..Hit::default()
        })
        .collect()
}

fn google_books(fetch: &dyn Fetch, q: &Query, key: &str) -> Result<Vec<Hit>, FetchError> {
    let search = match &q.isbn {
        Some(isbn) => format!("isbn:{}", isbn_key(isbn)),
        None => match q.authors.first() {
            Some(a) => format!("intitle:{} inauthor:{}", search_title(&q.title), surname(a)),
            None => format!("intitle:{}", search_title(&q.title)),
        },
    };
    let body = fetch.get(
        "https://www.googleapis.com/books/v1/volumes",
        &[("q", &search), ("maxResults", "5"), ("key", key)],
    )?;
    Ok(parse_google_books(&json(&body)?))
}

pub fn parse_google_books(json: &Value) -> Vec<Hit> {
    rows(json, &["items"])
        .filter_map(|i| i.get("volumeInfo"))
        .map(|v| Hit {
            title: str_at(v, "title"),
            authors: strings(v, "authors"),
            isbns: v
                .get("industryIdentifiers")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(|id| id.get("identifier")?.as_str().map(str::to_owned))
                .collect(),
            subjects: strings(v, "categories"),
            ..Hit::default()
        })
        .collect()
}

fn apple_books(fetch: &dyn Fetch, q: &Query) -> Result<Vec<Hit>, FetchError> {
    let body = fetch.get(
        "https://itunes.apple.com/search",
        &[
            ("term", &terms(q)),
            ("media", "ebook"),
            ("entity", "ebook"),
            ("limit", "5"),
        ],
    )?;
    Ok(parse_apple_books(&json(&body)?))
}

pub fn parse_apple_books(json: &Value) -> Vec<Hit> {
    rows(json, &["results"])
        .map(|r| Hit {
            title: str_at(r, "trackName"),
            authors: r
                .get("artistName")
                .and_then(Value::as_str)
                .map(|a| a.split(" & ").map(str::to_owned).collect())
                .unwrap_or_default(),
            subjects: strings(r, "genres"),
            ..Hit::default()
        })
        .collect()
}

// ---- JSON helpers ----

fn json(body: &str) -> Result<Value, FetchError> {
    serde_json::from_str(body).map_err(|e| FetchError::Http(format!("unexpected response: {e}")))
}

fn rows<'a>(json: &'a Value, path: &[&str]) -> impl Iterator<Item = &'a Value> {
    path.iter()
        .try_fold(json, |v, k| v.get(k))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
}

fn str_at(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn strings(v: &Value, key: &str) -> Vec<String> {
    v.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|s| s.as_str().map(str::to_owned))
        .collect()
}

fn push_unique(list: &mut Vec<String>, value: Option<String>) {
    if let Some(v) = value
        && !list.contains(&v)
    {
        list.push(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(title: &str, author: &str) -> Query {
        Query {
            title: title.into(),
            authors: vec![author.into()],
            isbn: None,
        }
    }

    fn hit(title: &str, author: &str) -> Hit {
        Hit {
            title: title.into(),
            authors: vec![author.into()],
            ..Hit::default()
        }
    }

    #[test]
    fn matching_tolerates_spelling_and_subtitles() {
        let q = query("The Color of Magic", "Terry Pratchett");
        assert_eq!(
            match_confidence(&q, &hit("The Colour of Magic", "Terry Pratchett")),
            Some(Confidence::High)
        );
        let q = query("The Final Empire", "Sanderson, Brandon");
        assert_eq!(
            match_confidence(&q, &hit("Mistborn: The Final Empire", "Brandon Sanderson")),
            Some(Confidence::High)
        );
        let q = query("Red Seas Under Red Skies", "Scott Lynch");
        assert_eq!(
            match_confidence(
                &q,
                &hit(
                    "Red Seas Under Red Skies (Gentlemen Bastards #2)",
                    "Scott Lynch"
                )
            ),
            Some(Confidence::High)
        );
    }

    #[test]
    fn matching_rejects_other_authors_and_titles() {
        let q = query("Endurance", "Alfred Lansing");
        assert_eq!(
            match_confidence(&q, &hit("Endurance", "Stephen Baxter")),
            None
        );
        assert_eq!(match_confidence(&q, &hit("Endure", "Alfred Lansing")), None);
        let q = Query {
            isbn: Some("978-0-06-102071-2".into()),
            ..query("Something", "Someone")
        };
        let mut by_isbn = hit("Entirely different", "Other Person");
        by_isbn.isbns = vec!["9780061020712".into()];
        assert_eq!(match_confidence(&q, &by_isbn), Some(Confidence::High));
    }

    #[test]
    fn search_titles_drop_bracketed_notes() {
        assert_eq!(search_title("We Are Legion (We Are Bob)"), "We Are Legion");
        assert_eq!(
            search_title("All These Worlds (Bobiverse Book 3)"),
            "All These Worlds"
        );
        assert_eq!(search_title("Dune [Illustrated]: Deluxe"), "Dune: Deluxe");
        assert_eq!(search_title("A Clockwork Orange"), "A Clockwork Orange");
        assert_eq!(search_title("(Untitled)"), "(Untitled)");
        let q = query("We Are Legion (We Are Bob)", "Dennis E. Taylor");
        assert_eq!(
            match_confidence(&q, &hit("We Are Legion", "Dennis E. Taylor")),
            Some(Confidence::High)
        );
    }

    #[test]
    fn similarity_is_edit_distance_based() {
        assert!((similarity("colorofmagic", "colourofmagic") - 12.0 / 13.0).abs() < 1e-9);
        assert_eq!(similarity("", ""), 0.0);
    }
}
