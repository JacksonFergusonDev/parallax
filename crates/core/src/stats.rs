//! Reading statistics from a Kobo: time read, books in progress, and every
//! book Nickel recorded as finished (including ones removed since).
//!
//! Only reads the device. Grouping by month or year happens where it's shown,
//! so a saved copy stays right as time passes.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use erc_device::{DeviceBook, ReadStatus};
use erc_device_kobo::{FinishedEvent, KoboDevice};
use erc_library::Library;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatBook {
    pub content_id: String,
    pub title: String,
    pub author: Option<String>,
    pub percent: u8,
    /// Time Nickel counted in this book (only while it's on the device).
    pub time_secs: Option<u64>,
    /// Nickel's estimate of the time left.
    pub left_secs: Option<u64>,
    pub last_read: Option<DateTime<Utc>>,
    /// The latest finish; `None` if never finished.
    pub finished_at: Option<DateTime<Utc>>,
    pub times_finished: u32,
    /// Still on the Kobo (finished books can have been removed).
    pub on_device: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ReadingStats {
    pub device_model: String,
    /// Reading time over the books on the device.
    pub total_secs: u64,
    pub books_on_device: usize,
    /// In progress, most recently read first.
    pub reading: Vec<StatBook>,
    /// Every finished book, most recently finished first.
    pub finished: Vec<StatBook>,
    /// The ten books with the most reading time.
    pub most_time: Vec<StatBook>,
}

/// Reads the statistics from `device`. `library` names finished books that
/// have left the device when it knows them.
pub fn reading_stats(
    device: &KoboDevice,
    library: Option<&Library>,
) -> erc_device_kobo::Result<ReadingStats> {
    let db = device.open_db()?;
    let books = db.books()?;
    let events = db.finished_events()?;
    let left = db.time_left()?;
    let titles = db.titles()?;
    let mut known: HashMap<String, (String, Option<String>)> = HashMap::new();
    if let Some(library) = library {
        let serial = &device.version().serial;
        for copy in library.device_copies(serial).unwrap_or_default() {
            if let Ok(book) = library.book(copy.book_id) {
                known.insert(copy.content_id, (book.title, book.authors.first().cloned()));
            }
        }
    }
    Ok(compute(
        device.version().model_name().to_owned(),
        &books,
        &events,
        &left,
        |id| titles.get(id).or_else(|| known.get(id)).cloned(),
    ))
}

fn compute(
    model: String,
    books: &[DeviceBook],
    events: &[FinishedEvent],
    left: &HashMap<String, u64>,
    title_of: impl Fn(&str) -> Option<(String, Option<String>)>,
) -> ReadingStats {
    let finishes: HashMap<&str, &FinishedEvent> =
        events.iter().map(|e| (e.content_id.as_str(), e)).collect();
    let on_device: HashMap<&str, &DeviceBook> = books.iter().map(|b| (b.id.as_str(), b)).collect();
    let stat = |b: &DeviceBook| {
        let event = finishes.get(b.id.as_str());
        let finished_at = event.and_then(|e| e.at).or(
            // Marked finished without a recorded event (e.g. by an app).
            (b.read_status == ReadStatus::Finished)
                .then_some(b.last_read)
                .flatten(),
        );
        StatBook {
            content_id: b.id.clone(),
            title: b.title.clone(),
            author: b.author.clone(),
            percent: b.percent_read,
            time_secs: b.reading_time_secs,
            left_secs: left
                .get(&b.id)
                .copied()
                .filter(|_| b.read_status == ReadStatus::Reading),
            last_read: b.last_read,
            finished_at,
            times_finished: event.map_or(u32::from(b.read_status == ReadStatus::Finished), |e| {
                e.times
            }),
            on_device: true,
        }
    };

    let mut reading: Vec<StatBook> = books
        .iter()
        .filter(|b| b.read_status == ReadStatus::Reading)
        .map(stat)
        .collect();
    reading.sort_by_key(|b| std::cmp::Reverse(b.last_read));

    let mut finished: Vec<StatBook> = books
        .iter()
        .map(stat)
        .filter(|s| s.times_finished > 0)
        .collect();
    for event in events {
        if on_device.contains_key(event.content_id.as_str()) {
            continue;
        }
        let (title, author) =
            title_of(&event.content_id).unwrap_or_else(|| title_from_path(&event.content_id));
        finished.push(StatBook {
            content_id: event.content_id.clone(),
            title,
            author,
            percent: 100,
            time_secs: None,
            left_secs: None,
            last_read: event.at,
            finished_at: event.at,
            times_finished: event.times,
            on_device: false,
        });
    }
    finished.sort_by(|a, b| {
        b.finished_at
            .cmp(&a.finished_at)
            .then_with(|| a.title.cmp(&b.title))
    });

    let mut most_time: Vec<StatBook> = books
        .iter()
        .filter(|b| b.reading_time_secs.is_some())
        .map(stat)
        .collect();
    most_time.sort_by_key(|b| std::cmp::Reverse(b.time_secs));
    most_time.truncate(10);

    ReadingStats {
        device_model: model,
        total_secs: books.iter().filter_map(|b| b.reading_time_secs).sum(),
        books_on_device: books.len(),
        reading,
        finished,
        most_time,
    }
}

/// A title (and author) from a sideloaded book's file name, for books that
/// are gone. Store books have opaque ids: "Unknown book". Understands:
/// - calibre and ERC: `Title - Author`, with unsafe characters as `_`;
/// - Library Genesis: `Author - Title (2009, Publisher) - libgen.li`;
/// - Anna's Archive: `Title -- Author -- Publisher, 2014 -- … -- <hash>`;
/// - sort forms: `Hobbit, The`, `Rothfuss, Patrick - Title`.
fn title_from_path(content_id: &str) -> (String, Option<String>) {
    let Some(path) = content_id.strip_prefix("file:///") else {
        return ("Unknown book".to_owned(), None);
    };
    let name = path.rsplit('/').next().unwrap_or(path);
    let lower = name.to_lowercase();
    let stem = [".kepub.epub", ".epub", ".pdf", ".cbz", ".mobi"]
        .iter()
        .find(|ext| lower.ends_with(*ext))
        .map_or(name, |ext| &name[..name.len() - ext.len()]);
    // calibre and Kobo write unsafe characters as "_": "Legion_ Skin Deep",
    // "King_s", "Dennis E_".
    let mut text = format!("{stem} ")
        .replace("_s ", "'s ")
        .replace("_ ", ": ")
        .replace(": -", ". -")
        .replace('_', " ")
        .trim()
        .to_owned();
    if text.ends_with(':') {
        text.pop();
        text.push('.');
    }
    let author = |s: &str| {
        let a = strip_brackets(s, true);
        let a = a.trim().trim_end_matches(',').trim();
        (!a.is_empty()).then(|| a.to_owned())
    };

    // Anna's Archive: title first, then author, then details.
    if text.contains(" -- ") {
        let parts: Vec<&str> = text.split(" -- ").collect();
        return (
            unsort(&strip_brackets(parts[0], true)),
            parts.get(1).and_then(|a| author(a)),
        );
    }
    // A slug with no spaces: "project-hail-mary-andy-weir-2021--annas-archive".
    if !text.contains(' ') && text.contains('-') {
        let slug = text.split("--").next().unwrap_or(&text);
        return (slug.replace('-', " "), None);
    }
    // Library Genesis: author first, then title, then the site.
    if let Some((body, site)) = text.rsplit_once(" - ") {
        let site = site.trim().to_lowercase();
        if (site.contains('.') && !site.contains(' ')) || site.contains("archive") {
            let body = strip_brackets(body, true);
            return match body.split_once(" - ") {
                Some((first, title)) => (unsort(title), author(first)),
                None => (unsort(&body), None),
            };
        }
    }
    let parts: Vec<&str> = text.split(" - ").map(str::trim).collect();
    match parts.as_slice() {
        [only] => (unsort(&strip_brackets(only, false)), None),
        // "Rothfuss, Patrick - The Name of the Wind"; not "Hobbit, The - J. R. R. Tolkien".
        [first, rest @ ..] if first.contains(',') && !is_sort_title(first) => (
            unsort(&strip_brackets(&rest.join(" - "), false)),
            author(first),
        ),
        [title @ .., last] => (
            unsort(&strip_brackets(&title.join(" - "), false)),
            author(last),
        ),
        [] => (stem.trim().to_owned(), None),
    }
}

/// "Hobbit, The": a title with its article moved to the end for sorting.
fn is_sort_title(s: &str) -> bool {
    let lower = s.trim().to_lowercase();
    [", the", ", a", ", an"].iter().any(|a| lower.ends_with(a))
}

/// "Hobbit, The" → "The Hobbit".
fn unsort(s: &str) -> String {
    let s = s.trim();
    if is_sort_title(s)
        && let Some((title, article)) = s.rsplit_once(", ")
    {
        return format!("{} {}", article.trim(), title.trim());
    }
    s.to_owned()
}

/// Drops `[…]` and `(…)` notes: all of them when `all`, else only ones with
/// a number ("(2009, DAW)", "[Murderbot Diaries 3 ]"), keeping ones that are
/// part of a title ("(We Are Bob)").
fn strip_brackets(s: &str, all: bool) -> String {
    let mut out = String::new();
    let mut note = String::new();
    let mut depth = 0usize;
    for c in s.chars() {
        match c {
            '(' | '[' => {
                depth += 1;
                note.push(c);
            }
            ')' | ']' if depth > 0 => {
                depth -= 1;
                note.push(c);
                if depth == 0 {
                    if !all && !note.chars().any(|c| c.is_ascii_digit()) {
                        out.push_str(&note);
                    }
                    note.clear();
                }
            }
            _ if depth > 0 => note.push(c),
            _ => out.push(c),
        }
    }
    if !all {
        out.push_str(&note);
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[expect(clippy::unnecessary_wraps, reason = "the fields it fills are optional")]
    fn at(s: &str) -> Option<DateTime<Utc>> {
        Some(s.parse().unwrap())
    }

    fn book(
        id: &str,
        status: ReadStatus,
        percent: u8,
        secs: Option<u64>,
        last: &str,
    ) -> DeviceBook {
        DeviceBook {
            id: id.into(),
            title: id.into(),
            author: Some("Author".into()),
            path: None,
            sideloaded: true,
            series: None,
            series_number: None,
            read_status: status,
            percent_read: percent,
            last_read: at(last),
            reading_time_secs: secs,
            added: None,
            file_size: None,
            collections: vec![],
        }
    }

    #[test]
    fn titles_from_file_names() {
        let t = |name: &str| title_from_path(&format!("file:///mnt/onboard/x/{name}"));
        let pair = |title: &str, author: &str| (title.to_owned(), Some(author.to_owned()));
        assert_eq!(
            t("Dune - Frank Herbert.kepub.epub"),
            pair("Dune", "Frank Herbert")
        );
        assert_eq!(
            t("Rothfuss, Patrick - The Name of the Wind (2009, DAW) - libgen.li.epub"),
            pair("The Name of the Wind", "Rothfuss, Patrick")
        );
        assert_eq!(
            t("Martha Wells, - Network Effect (2020, Tor Books) - libgen.li.epub"),
            pair("Network Effect", "Martha Wells")
        );
        assert_eq!(
            t("Martha Wells - Exit Strategy (Tom Doherty Associates) - libgen.li.epub"),
            pair("Exit Strategy", "Martha Wells")
        );
        assert_eq!(
            t("[Library of Essential Reading ] Abbott, Edwin - Flatland (2004) - libgen.li.epub"),
            pair("Flatland", "Abbott, Edwin")
        );
        assert_eq!(
            t("Hobbit, The - J. R. R. Tolkien.epub"),
            pair("The Hobbit", "J. R. R. Tolkien")
        );
        assert_eq!(
            t("Elantris -- Brandon Sanderson [Sanderson, Brandon] -- 2006 -- bfb265ff3c0b.epub"),
            pair("Elantris", "Brandon Sanderson")
        );
        assert_eq!(
            t("project-hail-mary-andy-weir-2021--annas-archive--x.epub"),
            ("project hail mary andy weir 2021".to_owned(), None)
        );
        assert_eq!(
            t("[Murderbot Diaries 3 ] Wells, Martha - Rogue Protocol (2018) - libgen.li.epub"),
            pair("Rogue Protocol", "Wells, Martha")
        );
        assert_eq!(
            t("Legion_ Skin Deep - Brandon Sanderson.kepub.epub"),
            pair("Legion: Skin Deep", "Brandon Sanderson")
        );
        assert_eq!(t("The Martian.epub"), ("The Martian".to_owned(), None));
        assert_eq!(
            t("We Are Legion (We Are Bob) - Taylor, Dennis E_.kepub.epub"),
            pair("We Are Legion (We Are Bob)", "Taylor, Dennis E.")
        );
        assert_eq!(
            t("Legends of Ahn (King_s Dark Tidings Book 3) - Kel Kade.kepub.epub"),
            pair("Legends of Ahn", "Kel Kade")
        );
        assert_eq!(
            title_from_path("0f1e2d3c"),
            ("Unknown book".to_owned(), None)
        );
    }

    #[test]
    fn combines_current_books_with_finish_history() {
        let books = vec![
            book(
                "Reading",
                ReadStatus::Reading,
                40,
                Some(3600),
                "2026-11-01T10:00:00Z",
            ),
            book(
                "Older read",
                ReadStatus::Reading,
                5,
                Some(600),
                "2026-10-01T10:00:00Z",
            ),
            book(
                "Done here",
                ReadStatus::Finished,
                100,
                Some(7200),
                "2026-09-01T10:00:00Z",
            ),
            book(
                "Reread",
                ReadStatus::Reading,
                10,
                None,
                "2026-08-01T10:00:00Z",
            ),
            book(
                "Unread",
                ReadStatus::Unread,
                0,
                None,
                "2026-01-01T10:00:00Z",
            ),
        ];
        let events = vec![
            FinishedEvent {
                content_id: "Reread".into(),
                at: at("2025-12-31T23:00:00Z"),
                times: 2,
            },
            FinishedEvent {
                content_id: "file:///mnt/onboard/KEpub/Sci-Fi/We Are Legion (We Are Bob) - Taylor, Dennis E_.kepub.epub".into(),
                at: at("2026-07-13T20:57:11Z"),
                times: 1,
            },
            FinishedEvent {
                content_id: "file:///mnt/onboard/gone.epub".into(),
                at: at("2024-01-05T00:00:00Z"),
                times: 1,
            },
            FinishedEvent {
                content_id: "0f1e2d3c-store".into(),
                at: at("2023-12-24T00:00:00Z"),
                times: 1,
            },
        ];
        let left = HashMap::from([("Reading".to_owned(), 5400), ("Done here".to_owned(), 60)]);
        let stats = compute("Kobo Libra 2".into(), &books, &events, &left, |id| {
            (id == "file:///mnt/onboard/gone.epub").then(|| ("Known title".to_owned(), None))
        });

        assert_eq!(stats.total_secs, 3600 + 600 + 7200);
        assert_eq!(stats.books_on_device, 5);
        let titles = |list: &[StatBook]| list.iter().map(|b| b.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(&stats.reading), ["Reading", "Older read", "Reread"]);
        assert_eq!(stats.reading[0].left_secs, Some(5400));
        assert_eq!(
            titles(&stats.finished),
            [
                "Done here",
                "We Are Legion (We Are Bob)",
                "Reread",
                "Known title",
                "Unknown book"
            ]
        );
        let legion = &stats.finished[1];
        assert_eq!(legion.author.as_deref(), Some("Taylor, Dennis E."));
        assert!(!legion.on_device);
        let done = &stats.finished[0];
        assert_eq!(done.finished_at, at("2026-09-01T10:00:00Z"));
        assert_eq!(done.left_secs, None);
        assert_eq!(stats.finished[2].times_finished, 2);
        assert_eq!(
            titles(&stats.most_time),
            ["Done here", "Reading", "Older read"]
        );
    }
}
