//! Manga series and editions from Mangainfo, and individual issues from Metron.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{
    Confidence, Fetch, FetchError, Hit, Lookup, SeriesFound, Source, alnum, isbn_key, pick_genres,
};

const MANGAINFO: &str = "https://mangainfo.com/api/v1";
const METRON: &str = "https://metron.cloud/api/issue/";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Catalog {
    Mangainfo,
    Metron,
}

impl Catalog {
    pub fn label(self) -> &'static str {
        self.source().label()
    }

    pub fn scheme(self) -> &'static str {
        match self {
            Self::Mangainfo => "mangainfo-url",
            Self::Metron => "metron-url",
        }
    }

    fn source(self) -> Source {
        match self {
            Self::Mangainfo => Source::Mangainfo,
            Self::Metron => Source::Metron,
        }
    }

    /// Accept only links to this provider's public records.
    pub fn allows_url(self, url: &str) -> bool {
        let Ok(url) = reqwest::Url::parse(url) else {
            return false;
        };
        let (host, path) = match self {
            Self::Mangainfo => ("mangainfo.com", "/series/"),
            Self::Metron => ("metron.cloud", "/issue/"),
        };
        url.scheme() == "https"
            && url.host_str() == Some(host)
            && url.path().starts_with(path)
            && url.username().is_empty()
            && url.password().is_none()
            && url.port().is_none()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRecord {
    pub catalog: Catalog,
    pub url: String,
    pub issue_id: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Credit {
    pub name: String,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IssueChoice {
    pub id: u64,
    pub title: String,
}

#[derive(Debug, thiserror::Error)]
pub enum MatchError {
    #[error(transparent)]
    Fetch(#[from] FetchError),
    #[error("multiple Metron issues match")]
    Ambiguous(Vec<IssueChoice>),
}

#[derive(Debug, Clone)]
pub struct Match {
    pub record: CatalogRecord,
    pub series: String,
    pub index: Option<String>,
    pub authors: Vec<String>,
    pub contributors: Vec<Credit>,
    pub description: Option<String>,
    pub publisher: Option<String>,
    pub genres: Vec<String>,
    pub confidence: Confidence,
}

impl Match {
    pub fn lookup(&self) -> Lookup {
        let source = self.record.catalog.source();
        Lookup {
            series: Some(SeriesFound {
                name: self.series.clone(),
                index: self.index.clone(),
                source,
                confidence: self.confidence,
            }),
            genres: pick_genres(&[(
                source,
                self.confidence,
                Hit {
                    subjects: self.genres.clone(),
                    ..Hit::default()
                },
            )]),
            ..Lookup::default()
        }
    }
}

#[derive(Debug, Default)]
pub struct Query<'a> {
    pub series: &'a str,
    pub authors: &'a [String],
    pub isbn: Option<&'a str>,
    pub volume: Option<&'a str>,
    pub issue: Option<&'a str>,
    pub run: Option<&'a str>,
    pub year: Option<&'a str>,
    pub publisher: Option<&'a str>,
    pub mangainfo_url: Option<&'a str>,
    pub metron_id: Option<u64>,
    pub comic_vine_id: Option<u64>,
    pub upc: Option<&'a str>,
}

fn parse(body: &str) -> Result<Value, FetchError> {
    super::json(body)
}

fn get_json(body: Result<String, FetchError>) -> Result<Value, FetchError> {
    match body {
        Ok(body) => parse(&body),
        Err(FetchError::Status { status: 404, .. }) => Ok(Value::Null),
        Err(error) => Err(error),
    }
}

fn string(value: &Value) -> Option<&str> {
    value.as_str().map(str::trim).filter(|s| !s.is_empty())
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map_or(&[], Vec::as_slice)
}

pub fn isbn(value: &str) -> Option<String> {
    let key = isbn_key(value);
    let valid = match key.len() {
        10 => {
            key[..9].bytes().all(|c| c.is_ascii_digit())
                && (key.as_bytes()[9].is_ascii_digit() || key.ends_with('X'))
        }
        13 => {
            (key.starts_with("978") || key.starts_with("979"))
                && key.bytes().all(|c| c.is_ascii_digit())
        }
        _ => false,
    };
    valid.then_some(key)
}

fn mangainfo_slug(value: &str) -> Option<&str> {
    (!value.is_empty()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-'))
    .then_some(value)
}

fn mangainfo_alias_matches(title: &str, record: &Value) -> bool {
    let aliases = &record["titles"];
    [
        &record["title"],
        &aliases["english"],
        &aliases["romaji"],
        &aliases["native"],
    ]
    .into_iter()
    .chain(array(&aliases["other"]))
    .filter_map(string)
    .any(|s| alnum(s) == title)
}

fn name_agrees(authors: &[String], found: &[String]) -> bool {
    authors.is_empty()
        || authors.iter().any(|author| {
            found.iter().any(|name| {
                let wanted = super::surname(author);
                !wanted.is_empty()
                    && (super::surname(name) == wanted
                        || name
                            .split_whitespace()
                            .next()
                            .is_some_and(|first| alnum(first) == wanted))
            })
        })
}

pub fn mangainfo(fetch: &dyn Fetch, query: &Query<'_>) -> Result<Option<Match>, FetchError> {
    let mut edition = Value::Null;
    if let Some(isbn) = query.isbn.and_then(isbn) {
        edition = get_json(fetch.get(&format!("{MANGAINFO}/volumes/{isbn}"), &[]))?;
    }
    let saved = query
        .mangainfo_url
        .filter(|url| Catalog::Mangainfo.allows_url(url))
        .and_then(|url| url.strip_prefix("https://mangainfo.com/series/"))
        .and_then(|path| path.split('/').next())
        .and_then(mangainfo_slug);
    let direct = string(&edition["data"]["series"]["slug"])
        .and_then(mangainfo_slug)
        .or(saved);
    let search;
    let slug = if let Some(slug) = direct {
        slug
    } else {
        let title = alnum(query.series);
        if title.is_empty() {
            return Ok(None);
        }
        search = get_json(fetch.get(&format!("{MANGAINFO}/series"), &[("q", query.series)]))?;
        let mut exact = array(&search["data"])
            .iter()
            .filter(|r| mangainfo_alias_matches(&title, r));
        let candidate = exact
            .next()
            .and_then(|r| string(&r["slug"]))
            .and_then(mangainfo_slug);
        if exact.next().is_some() {
            return Err(FetchError::Http(
                "multiple series share this title. Add an ISBN or a more specific series title"
                    .into(),
            ));
        }
        let Some(slug) = candidate else {
            return Ok(None);
        };
        slug
    };
    let detail = get_json(fetch.get(&format!("{MANGAINFO}/series/{slug}"), &[]))?;
    let series = &detail["data"];
    let (Some(title), Some(url)) = (string(&series["title"]), string(&series["url"])) else {
        return Ok(None);
    };
    if !Catalog::Mangainfo.allows_url(url) {
        return Ok(None);
    }
    let authors: Vec<String> = array(&series["creators"])
        .iter()
        .filter(|creator| {
            matches!(
                creator["role"].as_str(),
                Some("story" | "story_and_art" | "author")
            )
        })
        .filter_map(|creator| string(&creator["name"]).map(str::to_owned))
        .collect();
    if direct.is_none() && !authors.is_empty() && !name_agrees(query.authors, &authors) {
        return Ok(None);
    }
    let volume = &edition["data"];
    let english = &volume["edition"];
    let mut index = match array(&english["volumes"]) {
        [number] => number.as_u64(),
        [] if volume["match"].as_str() == Some("japanese_volume") => {
            volume["volume"]["number"].as_u64()
        }
        _ => None,
    };
    let mut confidence = Confidence::High;
    if volume.is_null()
        && let Some(number) = query.volume.and_then(|value| value.parse::<u64>().ok())
    {
        let volumes = get_json(fetch.get(&format!("{MANGAINFO}/series/{slug}/volumes"), &[]))?;
        if array(&volumes["data"])
            .iter()
            .any(|v| v["number"].as_u64() == Some(number))
        {
            index = Some(number);
        } else {
            confidence = Confidence::Low;
        }
    }
    Ok(Some(Match {
        record: CatalogRecord {
            catalog: Catalog::Mangainfo,
            url: url.into(),
            issue_id: None,
        },
        series: title.into(),
        index: index.map(|number| number.to_string()),
        authors,
        contributors: array(&series["creators"])
            .iter()
            .filter(|creator| creator["role"].as_str() == Some("art"))
            .filter_map(|creator| {
                Some(Credit {
                    name: string(&creator["name"])?.into(),
                    role: "art".into(),
                })
            })
            .collect(),
        description: string(&series["description"]).map(str::to_owned),
        publisher: string(&english["publisher"]).map(str::to_owned),
        genres: array(&series["genres"])
            .iter()
            .filter_map(string)
            .map(str::to_owned)
            .collect(),
        confidence,
    }))
}

fn metron_issue_id(
    fetch: &dyn Fetch,
    token: &str,
    query: &Query<'_>,
    cover_hash: impl FnOnce() -> Option<u64>,
) -> Result<Option<(u64, Confidence)>, MatchError> {
    if let Some(id) = query.metron_id {
        return Ok(Some((id, Confidence::High)));
    }
    let cv = query.comic_vine_id.map(|id| id.to_string());
    let direct = cv
        .as_deref()
        .map(|id| ("cv_id", id))
        .or(query.upc.map(|upc| ("upc", upc)));
    let params = if let Some(exact) = direct {
        vec![exact]
    } else {
        let Some(number) = query.issue else {
            return Ok(None);
        };
        if query.series.trim().is_empty() {
            return Ok(None);
        }
        let mut params = vec![("series_name", query.series), ("number", number)];
        params.extend(query.run.map(|run| ("series_volume", run)));
        params.extend(
            query
                .publisher
                .map(|publisher| ("publisher_name", publisher)),
        );
        params
    };
    let response = get_json(fetch.get_authenticated(METRON, &params, token))?;
    let same_series = |value: &&Value| {
        direct.is_some()
            || string(&value["series"]["name"])
                .is_some_and(|name| alnum(name) == alnum(query.series))
    };
    let mut matches: Vec<_> = array(&response["results"])
        .iter()
        .filter(same_series)
        .collect();
    let complete = response["next"].is_null();
    if complete
        && matches.len() > 1
        && let Some(year) = query.year
    {
        let matching_year: Vec<_> = matches
            .iter()
            .copied()
            .filter(|issue| {
                ["cover_date", "store_date"]
                    .iter()
                    .any(|key| string(&issue[*key]).and_then(|s| s.get(..4)) == Some(year))
            })
            .collect();
        if !matching_year.is_empty() {
            matches = matching_year;
        }
    }
    let mut confidence = if direct.is_some() {
        Confidence::High
    } else {
        Confidence::Low
    };
    if complete
        && matches.len() > 1
        && let Some(local) = cover_hash()
    {
        let close: Vec<_> = matches
            .iter()
            .copied()
            .filter(|issue| {
                string(&issue["cover_hash"])
                    .and_then(|hash| u64::from_str_radix(hash, 16).ok())
                    .is_some_and(|hash| (local ^ hash).count_ones() <= 6)
            })
            .collect();
        if close.len() == 1 {
            matches = close;
            confidence = Confidence::High;
        }
    }
    match matches.as_slice() {
        [] => Ok(None),
        [issue] if complete => Ok(issue["id"].as_u64().map(|id| (id, confidence))),
        _ => Err(MatchError::Ambiguous(
            matches
                .into_iter()
                .filter_map(|issue| {
                    let id = issue["id"].as_u64()?;
                    let title = string(&issue["issue"]).map_or_else(
                        || {
                            format!(
                                "{} #{} · Metron issue {id}",
                                query.series,
                                query.issue.unwrap_or_default()
                            )
                        },
                        str::to_owned,
                    );
                    Some(IssueChoice { id, title })
                })
                .collect(),
        )),
    }
}

pub fn metron(
    fetch: &dyn Fetch,
    token: &str,
    query: &Query<'_>,
    cover_hash: impl FnOnce() -> Option<u64>,
) -> Result<Option<Match>, MatchError> {
    let Some((id, confidence)) = metron_issue_id(fetch, token, query, cover_hash)? else {
        return Ok(None);
    };
    let issue = get_json(fetch.get_authenticated(&format!("{METRON}{id}/"), &[], token))?;
    let series = &issue["series"];
    let (Some(title), Some(url)) = (string(&series["name"]), string(&issue["resource_url"])) else {
        return Ok(None);
    };
    if !Catalog::Metron.allows_url(url) {
        return Ok(None);
    }
    let (mut authors, mut contributors) = (Vec::new(), Vec::new());
    for credit in array(&issue["credits"]) {
        let Some(name) = string(&credit["creator"]) else {
            continue;
        };
        for role in array(&credit["role"])
            .iter()
            .filter_map(|r| string(&r["name"]))
        {
            let role = match role.to_ascii_lowercase().as_str() {
                "writer" | "script" | "plot" => {
                    if !authors.iter().any(|author: &String| author == name) {
                        authors.push(name.to_owned());
                    }
                    continue;
                }
                "artist" | "penciller" | "penciler" | "inker" => "art",
                "colorist" | "colourist" => "clr",
                "letterer" => "ctb",
                "cover artist" | "cover" => "cov",
                "editor" => "edt",
                "translator" => "trl",
                _ => continue,
            };
            let credit = Credit {
                name: name.into(),
                role: role.into(),
            };
            if !contributors.contains(&credit) {
                contributors.push(credit);
            }
        }
    }
    if confidence == Confidence::Low && !authors.is_empty() && !name_agrees(query.authors, &authors)
    {
        return Ok(None);
    }
    // Publisher solicitation copy has different rights from Metron's own metadata.
    let description = (issue["desc_is_solicitation"].as_bool() == Some(false))
        .then(|| string(&issue["desc"]))
        .flatten()
        .map(str::to_owned);
    Ok(Some(Match {
        record: CatalogRecord {
            catalog: Catalog::Metron,
            url: url.into(),
            issue_id: Some(id),
        },
        series: title.into(),
        index: string(&issue["number"]).map(str::to_owned),
        authors,
        contributors,
        description,
        publisher: string(&issue["publisher"]["name"]).map(str::to_owned),
        genres: array(&series["genres"])
            .iter()
            .filter_map(|genre| string(&genre["name"]))
            .map(str::to_owned)
            .collect(),
        confidence,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture;

    impl Fetch for Fixture {
        fn get(&self, url: &str, params: &[(&str, &str)]) -> Result<String, FetchError> {
            Ok(match url.strip_prefix(MANGAINFO).unwrap() {
                "/series" => {
                    assert_eq!(params, [("q", "Beruseruku")]);
                    r#"{"data":[{"slug":"berserk","title":"Berserk","titles":{"other":["Beruseruku"]}},{"slug":"berserk-of-gluttony","title":"Berserk of Gluttony"}]}"#
                }
                "/volumes/9781593070205" => r#"{"data":{"series":{"slug":"berserk"},"edition":{"volumes":[1],"publisher":"Dark Horse"}}}"#,
                "/series/berserk/volumes" => r#"{"data":[{"number":1},{"number":2}]}"#,
                "/series/berserk" => r#"{"data":{"title":"Berserk","url":"https://mangainfo.com/series/berserk","genres":["Fantasy"],"creators":[{"name":"Kentaro Miura","role":"story_and_art"}],"description":"A fictional synopsis for this fixture."}}"#,
                other => panic!("unexpected request {other}"),
            }.into())
        }

        fn get_authenticated(
            &self,
            url: &str,
            params: &[(&str, &str)],
            token: &str,
        ) -> Result<String, FetchError> {
            assert_eq!(token, "fixture");
            Ok(match url.strip_prefix(METRON).unwrap() {
                "" => {
                    assert_eq!(params, [("series_name", "Spider-Man"), ("number", "1")]);
                    r#"{"next":null,"results":[{"id":40,"series":{"name":"Spider-Man"},"cover_hash":"abcdeff123456789"},{"id":41,"series":{"name":"Spider-Man"},"cover_hash":"0123456789abcdef"}]}"#
                }
                "41/" => r#"{"id":41,"series":{"name":"Spider-Man","genres":[{"name":"Superhero"}]},"number":"1","publisher":{"name":"Marvel"},"credits":[{"creator":"Fixture Writer","role":[{"name":"Writer"}]},{"creator":"Fixture Artist","role":[{"name":"Penciller"}]}],"desc":"Publisher solicitation text","desc_is_solicitation":true,"resource_url":"https://metron.cloud/issue/spider-man-1/"}"#,
                other => panic!("unexpected request {other}"),
            }.into())
        }
    }

    #[test]
    fn manga_alias_and_volume_match_the_series_without_guessing_an_edition() {
        let found = mangainfo(
            &Fixture,
            &Query {
                series: "Beruseruku",
                volume: Some("2"),
                ..Query::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.series, "Berserk");
        assert_eq!(found.index.as_deref(), Some("2"));
        assert_eq!(found.authors, ["Kentaro Miura"]);
        assert!(found.publisher.is_none());
        assert_eq!(found.lookup().genres[0].name, "Fantasy");
    }

    #[test]
    fn manga_isbn_selects_the_publisher_and_exact_volume() {
        let found = mangainfo(
            &Fixture,
            &Query {
                series: "Scanned pages",
                isbn: Some("978-1-59307-020-5"),
                ..Query::default()
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(found.publisher.as_deref(), Some("Dark Horse"));
        assert_eq!(found.index.as_deref(), Some("1"));
    }

    #[test]
    fn metron_uses_a_cover_tie_breaker_or_a_known_issue_id() {
        let query = Query {
            series: "Spider-Man",
            issue: Some("1"),
            ..Query::default()
        };
        assert!(metron(&Fixture, "fixture", &query, || None).is_err());
        let by_cover = metron(&Fixture, "fixture", &query, || Some(0x0123_4567_89ab_cdef))
            .unwrap()
            .unwrap();
        let by_id = metron(
            &Fixture,
            "fixture",
            &Query {
                metron_id: Some(41),
                ..query
            },
            || {
                panic!("a known ID does not require cover comparison");
            },
        )
        .unwrap()
        .unwrap();
        assert_eq!(by_cover.record, by_id.record);
        assert_eq!(by_id.confidence, Confidence::High);
        assert_eq!(by_id.authors, ["Fixture Writer"]);
        assert_eq!(by_id.contributors[0].role, "art");
        assert!(by_id.description.is_none());
    }
}
