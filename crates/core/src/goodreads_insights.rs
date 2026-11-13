//! What your Goodreads shelves say about your reading: books and pages a
//! year, how you rate, the to-read pile, favourite authors, series in
//! progress and the shape of what you read.
//!
//! Worked out on demand from the saved books as you see them (with your edits)
//! and the library, so it follows both. Nothing here writes. Books refer to
//! each other by Goodreads Book Id; [`Insights::books`] has every one.
//!
//! Goodreads only keeps the latest date read, and many books on the read
//! shelf have none, so anything over time counts each dated book once and
//! says how many were undated rather than leaving them out silently.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use chrono::{Datelike, NaiveDate};
use parallax_library::{GoodreadsImport, Library};
use serde::Serialize;

use crate::goodreads::{self, Book, GoodreadsError, Shelf};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Insights {
    /// The latest import, if any was recorded.
    pub import: Option<GoodreadsImport>,
    /// How many exports have been imported.
    pub imports: usize,
    pub totals: Totals,
    /// Dated reads per year, from the first to this year, with empty years.
    pub years: Vec<YearCount>,
    /// This year so far against the same point last year.
    pub pace: Option<Pace>,
    pub ratings: Ratings,
    pub to_read: Backlog,
    /// How long books waited on your shelves before you read them.
    pub wait: Option<Wait>,
    /// Authors you've read most, most first.
    pub authors: Vec<AuthorCount>,
    /// Series you've started and have more of to read.
    pub series: Vec<SeriesProgress>,
    /// When the books were first published, oldest first.
    pub decades: Vec<DecadeCount>,
    pub lengths: Lengths,
    /// The editions on your shelves, by kind, most first.
    pub formats: Vec<Count>,
    /// Your own shelves, biggest first.
    pub shelves: Vec<Count>,
    /// Books read more than once, most reads first.
    pub rereads: Vec<String>,
    /// Every saved book, in the order saved.
    pub books: Vec<InsightBook>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub books: usize,
    pub read: usize,
    pub to_read: usize,
    pub reading: usize,
    /// On another exclusive shelf Goodreads may add.
    pub other: usize,
    /// Read books with a rating.
    pub rated: usize,
    pub reviewed: usize,
    /// Pages of the read books that have a page count.
    pub pages_read: u64,
    /// Read books without a date read.
    pub read_undated: usize,
    /// Books matching at least one library book.
    pub in_library: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YearCount {
    pub year: i32,
    pub books: usize,
    pub pages: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pace {
    pub year: i32,
    pub this_year: usize,
    /// Read by today's date last year.
    pub last_year_by_now: usize,
    pub last_year: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Ratings {
    /// Read books with each rating, one star first.
    pub stars: [usize; 5],
    /// Read books you didn't rate.
    pub unrated: usize,
    pub average: Option<f32>,
    /// Against Goodreads readers, when the export had their averages.
    pub against_goodreads: Option<RatingGap>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RatingGap {
    /// Books you rated that have a Goodreads average.
    pub books: usize,
    pub mine: f32,
    pub goodreads: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Backlog {
    pub books: usize,
    /// Already in your library, ready to read.
    pub in_library: usize,
    pub pages: u64,
    /// Median days since the books were added.
    pub median_age_days: Option<i64>,
    /// The one waiting longest.
    pub oldest: Option<String>,
    /// At the last 12 months' pace of dated reads.
    pub years_to_clear: Option<f32>,
    /// Dated reads in the last 12 months.
    pub read_last_year: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Wait {
    /// Books added before the day they were read.
    pub books: usize,
    pub median_days: i64,
    /// The one that waited longest.
    pub longest: String,
    pub longest_days: i64,
    /// Read books added on or after the day read: logged once finished.
    pub logged_after: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuthorCount {
    pub name: String,
    pub read: usize,
    /// On any shelf.
    pub books: usize,
    /// Your average for their books you rated.
    pub average: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SeriesProgress {
    pub name: String,
    pub author: Option<String>,
    /// Numbers read, in order, as Goodreads writes them.
    pub read: Vec<String>,
    /// Numbers on your other shelves.
    pub unread: Vec<String>,
    /// Whole numbers before the latest one read that aren't on your shelves.
    pub missing: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DecadeCount {
    /// The decade's first year; `None` for anything before 1900.
    pub decade: Option<i32>,
    pub read: usize,
    pub to_read: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Lengths {
    pub buckets: Vec<LengthBucket>,
    /// Over read books with a page count.
    pub average: Option<u32>,
    pub longest: Option<String>,
    pub shortest: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LengthBucket {
    pub min: u32,
    /// Exclusive; `None` for the last.
    pub max: Option<u32>,
    pub read: usize,
    pub to_read: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Count {
    pub name: String,
    pub books: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InsightBook {
    #[serde(flatten)]
    pub book: Book,
    /// Library books it matches.
    pub library: Vec<i64>,
}

/// Today's date where you are, the day insights are worked out as of.
pub fn today() -> NaiveDate {
    chrono::Local::now().date_naive()
}

/// Insights from the saved books, the library and the last import.
pub fn insights(lib: &Library, today: NaiveDate) -> Result<Insights, GoodreadsError> {
    let saved = goodreads::saved(lib)?;
    let mut out = compute(&saved, &lib.books()?, today);
    if let Some((import, count)) = lib.goodreads_last_import()? {
        out.import = Some(import);
        out.imports = count;
    }
    Ok(out)
}

/// Insights from these books against this library, as of `today`.
pub fn compute(books: &[Book], library: &[parallax_library::Book], today: NaiveDate) -> Insights {
    let mut matches: HashMap<&str, Vec<i64>> = HashMap::new();
    let found = goodreads::library_matches(books, library);
    for (&library_id, m) in &found {
        matches
            .entry(m.goodreads_id.as_str())
            .or_default()
            .push(library_id);
    }
    for ids in matches.values_mut() {
        ids.sort_unstable();
    }
    let in_library = |b: &Book| matches.contains_key(b.goodreads_id.as_str());
    let read: Vec<&Book> = books.iter().filter(|b| b.shelf == Shelf::Read).collect();
    let to_read: Vec<&Book> = books.iter().filter(|b| b.shelf == Shelf::ToRead).collect();

    let totals = Totals {
        books: books.len(),
        read: read.len(),
        to_read: to_read.len(),
        reading: books
            .iter()
            .filter(|b| b.shelf == Shelf::CurrentlyReading)
            .count(),
        other: books
            .iter()
            .filter(|b| matches!(b.shelf, Shelf::Other(_)))
            .count(),
        rated: read.iter().filter(|b| b.rating.is_some()).count(),
        reviewed: books.iter().filter(|b| b.review.is_some()).count(),
        pages_read: read.iter().filter_map(|b| b.pages).map(u64::from).sum(),
        read_undated: read.iter().filter(|b| b.date_read.is_none()).count(),
        in_library: books.iter().filter(|b| in_library(b)).count(),
    };

    Insights {
        import: None,
        imports: 0,
        years: years(&read, today),
        pace: pace(&read, today),
        ratings: ratings(&read),
        to_read: backlog(&read, &to_read, &in_library, today),
        wait: wait(&read),
        authors: authors(books),
        series: series(books),
        decades: decades(books),
        lengths: lengths(books, &read),
        formats: formats(books),
        shelves: shelves(books),
        rereads: rereads(&read),
        books: books
            .iter()
            .map(|b| InsightBook {
                book: b.clone(),
                library: matches
                    .get(b.goodreads_id.as_str())
                    .cloned()
                    .unwrap_or_default(),
            })
            .collect(),
        totals,
    }
}

fn years(read: &[&Book], today: NaiveDate) -> Vec<YearCount> {
    let mut by_year: BTreeMap<i32, (usize, u64)> = BTreeMap::new();
    for b in read {
        if let Some(date) = b.date_read {
            let entry = by_year.entry(date.year()).or_default();
            entry.0 += 1;
            entry.1 += u64::from(b.pages.unwrap_or(0));
        }
    }
    let Some(&first) = by_year.keys().next() else {
        return Vec::new();
    };
    let last = today.year().max(*by_year.keys().last().unwrap_or(&first));
    (first..=last)
        .map(|year| {
            let (books, pages) = by_year.get(&year).copied().unwrap_or_default();
            YearCount { year, books, pages }
        })
        .collect()
}

fn pace(read: &[&Book], today: NaiveDate) -> Option<Pace> {
    let year = today.year();
    let by_now = |d: NaiveDate| (d.month(), d.day()) <= (today.month(), today.day());
    let dates: Vec<NaiveDate> = read.iter().filter_map(|b| b.date_read).collect();
    let this_year = dates
        .iter()
        .filter(|d| d.year() == year && **d <= today)
        .count();
    let last: Vec<&NaiveDate> = dates.iter().filter(|d| d.year() == year - 1).collect();
    (this_year > 0 || !last.is_empty()).then(|| Pace {
        year,
        this_year,
        last_year_by_now: last.iter().filter(|d| by_now(***d)).count(),
        last_year: last.len(),
    })
}

fn mean(values: impl Iterator<Item = f32>) -> Option<f32> {
    let (sum, n) = values.fold((0.0, 0u32), |(s, n), v| (s + v, n + 1));
    (n > 0).then(|| sum / n as f32)
}

fn ratings(read: &[&Book]) -> Ratings {
    let mut stars = [0; 5];
    for r in read.iter().filter_map(|b| b.rating) {
        stars[usize::from(r) - 1] += 1;
    }
    let both: Vec<(f32, f32)> = read
        .iter()
        .filter_map(|b| Some((f32::from(b.rating?), b.average_rating?)))
        .collect();
    Ratings {
        stars,
        unrated: read.iter().filter(|b| b.rating.is_none()).count(),
        average: mean(read.iter().filter_map(|b| b.rating).map(f32::from)),
        // A handful of books says little about how you rate.
        against_goodreads: (both.len() >= 5).then(|| RatingGap {
            books: both.len(),
            mine: mean(both.iter().map(|p| p.0)).unwrap_or_default(),
            goodreads: mean(both.iter().map(|p| p.1)).unwrap_or_default(),
        }),
    }
}

fn median(mut values: Vec<i64>) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    let mid = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        i64::midpoint(values[mid - 1], values[mid])
    } else {
        values[mid]
    })
}

fn backlog(
    read: &[&Book],
    to_read: &[&Book],
    in_library: &dyn Fn(&Book) -> bool,
    today: NaiveDate,
) -> Backlog {
    let year_ago = today - chrono::Duration::days(365);
    let read_last_year = read
        .iter()
        .filter_map(|b| b.date_read)
        .filter(|d| *d > year_ago && *d <= today)
        .count();
    let oldest = to_read
        .iter()
        .filter(|b| b.date_added.is_some())
        .min_by_key(|b| (b.date_added, b.goodreads_id.as_str()))
        .map(|b| b.goodreads_id.clone());
    Backlog {
        books: to_read.len(),
        in_library: to_read.iter().filter(|b| in_library(b)).count(),
        pages: to_read.iter().filter_map(|b| b.pages).map(u64::from).sum(),
        median_age_days: median(
            to_read
                .iter()
                .filter_map(|b| b.date_added)
                .map(|d| (today - d).num_days().max(0))
                .collect(),
        ),
        oldest,
        years_to_clear: (read_last_year > 0 && !to_read.is_empty())
            .then(|| to_read.len() as f32 / read_last_year as f32),
        read_last_year,
    }
}

fn wait(read: &[&Book]) -> Option<Wait> {
    let mut waits = Vec::new();
    let mut logged_after = 0;
    for b in read {
        let (Some(added), Some(done)) = (b.date_added, b.date_read) else {
            continue;
        };
        let days = (done - added).num_days();
        if days > 0 {
            waits.push((days, b.goodreads_id.as_str()));
        } else {
            logged_after += 1;
        }
    }
    let &(longest_days, longest) = waits
        .iter()
        .max_by_key(|(d, id)| (*d, std::cmp::Reverse(*id)))?;
    Some(Wait {
        books: waits.len(),
        median_days: median(waits.iter().map(|w| w.0).collect())?,
        longest: longest.to_owned(),
        longest_days,
        logged_after,
    })
}

fn authors(books: &[Book]) -> Vec<AuthorCount> {
    // The main author only: the others are often translators and editors.
    let mut by_name: BTreeMap<&str, (usize, usize, Vec<f32>)> = BTreeMap::new();
    for b in books {
        let Some(name) = b.authors.first() else {
            continue;
        };
        let entry = by_name.entry(name).or_default();
        entry.1 += 1;
        if b.shelf == Shelf::Read {
            entry.0 += 1;
            entry.2.extend(b.rating.map(f32::from));
        }
    }
    let mut out: Vec<AuthorCount> = by_name
        .into_iter()
        .filter(|(_, (read, _, _))| *read >= 2)
        .map(|(name, (read, books, rated))| AuthorCount {
            name: name.to_owned(),
            read,
            books,
            average: mean(rated.into_iter()),
        })
        .collect();
    out.sort_by(|a, b| {
        b.read
            .cmp(&a.read)
            .then(b.books.cmp(&a.books))
            .then_with(|| a.name.cmp(&b.name))
    });
    out.truncate(10);
    out
}

/// The whole numbers a series number covers: "3" is 3, "1-3" is 1 to 3,
/// "0.5" and other parts are none.
fn whole_numbers(number: &str) -> Vec<u32> {
    match number.split_once('-') {
        Some((from, to)) => match (from.trim().parse::<u32>(), to.trim().parse::<u32>()) {
            (Ok(from), Ok(to)) if from <= to && to - from < 100 => (from..=to).collect(),
            _ => Vec::new(),
        },
        None => number.trim().parse().into_iter().collect(),
    }
}

/// Orders "2" before "10" and "1.5" between them.
fn number_order(number: &str) -> (f64, String) {
    let lead = number.split('-').next().unwrap_or(number).trim();
    (lead.parse().unwrap_or(f64::MAX), number.to_owned())
}

/// A series name for grouping, so "Harbor's" and "Harbor‘s" or a change of case
/// don't split one series in two.
fn series_key(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{02bc}' | '`' | '\u{00b4}' => '\'',
            c => c,
        })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A series and number from a title Goodreads didn't mark as one, as it
/// often doesn't for manga volumes: "Glass Harbor, Vol. 10" or "ガラスの港 3
/// [Glass Harbor 3]".
fn title_series(title: &str) -> Option<(String, String)> {
    let title = title.trim();
    let whole = |n: &str| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit());
    let bracketed = title
        .strip_suffix(']')
        .and_then(|t| t.rsplit_once('['))
        .and_then(|(_, inner)| inner.trim().rsplit_once(' '));
    if let Some((name, n)) = bracketed
        && whole(n)
        && !name.trim().is_empty()
    {
        return Some((name.trim().to_owned(), n.to_owned()));
    }
    // ASCII lowercase keeps byte offsets, so they index the title too.
    let lower = title.to_ascii_lowercase();
    for marker in [" vol. ", " vol ", " volume ", ",vol. "] {
        let Some(at) = lower.rfind(marker) else {
            continue;
        };
        let n: String = title[at + marker.len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let name = title[..at]
            .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ',' | ':' | '-' | '–'));
        if whole(&n) && !name.is_empty() {
            return Some((name.to_owned(), n));
        }
    }
    None
}

struct Entry<'a> {
    book: &'a Book,
    name: String,
    number: String,
    /// From the title rather than Goodreads' series.
    inferred: bool,
}

/// The name given most often; ties go to the first alphabetically.
fn most_common<'a>(names: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for n in names {
        *counts.entry(n).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|(name, n)| (*n, std::cmp::Reverse(*name)))
        .map(|(name, _)| name.to_owned())
}

fn sorted_numbers(mut numbers: Vec<String>) -> Vec<String> {
    numbers.sort_by(|a, b| {
        let (a, b) = (number_order(a), number_order(b));
        a.0.total_cmp(&b.0).then(a.1.cmp(&b.1))
    });
    numbers.dedup();
    numbers
}

fn progress(list: &[Entry]) -> Option<SeriesProgress> {
    // A lone "Vol. 2" is as likely a collection as a series.
    if list.len() < 2 && list.iter().all(|e| e.inferred) {
        return None;
    }
    let read: Vec<String> = list
        .iter()
        .filter(|e| e.book.shelf == Shelf::Read)
        .map(|e| e.number.clone())
        .collect();
    if read.is_empty() {
        return None;
    }
    let read_whole: BTreeSet<u32> = read.iter().flat_map(|n| whole_numbers(n)).collect();
    // Another edition of a book you've read isn't more to read.
    let unread: Vec<String> = list
        .iter()
        .filter(|e| e.book.shelf != Shelf::Read)
        .map(|e| e.number.clone())
        .filter(|n| {
            let covers = whole_numbers(n);
            covers.is_empty() || covers.iter().any(|c| !read_whole.contains(c))
        })
        .collect();
    let have: BTreeSet<u32> = list.iter().flat_map(|e| whole_numbers(&e.number)).collect();
    let highest_read = read_whole.iter().max().copied().unwrap_or(0);
    let missing: Vec<u32> = (1..highest_read).filter(|n| !have.contains(n)).collect();
    if unread.is_empty() && missing.is_empty() {
        return None;
    }
    let explicit = list.iter().filter(|e| !e.inferred).map(|e| e.name.as_str());
    let name =
        most_common(explicit).or_else(|| most_common(list.iter().map(|e| e.name.as_str())))?;
    Some(SeriesProgress {
        name,
        author: most_common(
            list.iter()
                .filter_map(|e| e.book.authors.first().map(String::as_str)),
        ),
        read: sorted_numbers(read),
        unread: sorted_numbers(unread),
        missing,
    })
}

fn series(books: &[Book]) -> Vec<SeriesProgress> {
    let mut groups: BTreeMap<String, Vec<Entry>> = BTreeMap::new();
    for book in books {
        let found = match &book.series {
            Some(s) => Some((s.name.clone(), s.number.clone(), false)),
            None => title_series(&book.title).map(|(name, number)| (name, number, true)),
        };
        if let Some((name, number, inferred)) = found {
            groups.entry(series_key(&name)).or_default().push(Entry {
                book,
                name,
                number,
                inferred,
            });
        }
    }
    let mut out: Vec<SeriesProgress> = groups.values().filter_map(|l| progress(l)).collect();
    out.sort_by(|a, b| {
        b.read
            .len()
            .cmp(&a.read.len())
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

fn decades(books: &[Book]) -> Vec<DecadeCount> {
    let mut by: BTreeMap<Option<i32>, (usize, usize)> = BTreeMap::new();
    for b in books {
        let Some(year) = b.original_year.or(b.year_published) else {
            continue;
        };
        let decade = (year >= 1900).then(|| year - year.rem_euclid(10));
        let entry = by.entry(decade).or_default();
        match b.shelf {
            Shelf::Read => entry.0 += 1,
            Shelf::ToRead => entry.1 += 1,
            _ => {}
        }
    }
    // `None` sorts first, which is where "before 1900" belongs.
    by.into_iter()
        .filter(|(_, (read, to_read))| read + to_read > 0)
        .map(|(decade, (read, to_read))| DecadeCount {
            decade,
            read,
            to_read,
        })
        .collect()
}

const LENGTH_EDGES: [u32; 6] = [0, 200, 300, 400, 500, 700];

fn lengths(books: &[Book], read: &[&Book]) -> Lengths {
    let mut buckets: Vec<LengthBucket> = LENGTH_EDGES
        .iter()
        .enumerate()
        .map(|(i, &min)| LengthBucket {
            min,
            max: LENGTH_EDGES.get(i + 1).copied(),
            read: 0,
            to_read: 0,
        })
        .collect();
    for b in books {
        let Some(pages) = b.pages else { continue };
        let i = LENGTH_EDGES.iter().rposition(|&e| pages >= e).unwrap_or(0);
        match b.shelf {
            Shelf::Read => buckets[i].read += 1,
            Shelf::ToRead => buckets[i].to_read += 1,
            _ => {}
        }
    }
    let paged: Vec<(&&Book, u32)> = read.iter().filter_map(|b| Some((b, b.pages?))).collect();
    Lengths {
        buckets,
        average: (!paged.is_empty())
            .then(|| paged.iter().map(|p| u64::from(p.1)).sum::<u64>() / paged.len() as u64)
            .and_then(|a| u32::try_from(a).ok()),
        longest: paged
            .iter()
            .max_by_key(|p| p.1)
            .map(|p| p.0.goodreads_id.clone()),
        shortest: paged
            .iter()
            .min_by_key(|p| p.1)
            .map(|p| p.0.goodreads_id.clone()),
    }
}

/// Goodreads' many binding names, as the few kinds of book they are.
fn format_kind(binding: &str) -> &'static str {
    let b = binding.to_lowercase();
    if b.contains("audio") || b.contains("audible") {
        "Audiobook"
    } else if b.contains("kindle")
        || b.contains("ebook")
        || b.contains("nook")
        || b.contains("kobo")
    {
        "Ebook"
    } else if b.contains("paperback") || b.contains("softcover") {
        "Paperback"
    } else if b.contains("hardcover")
        || b.contains("hardback")
        || b.contains("leather")
        || b.contains("library binding")
    {
        "Hardcover"
    } else {
        "Other"
    }
}

fn counted(counts: BTreeMap<String, usize>) -> Vec<Count> {
    let mut out: Vec<Count> = counts
        .into_iter()
        .map(|(name, books)| Count { name, books })
        .collect();
    out.sort_by(|a, b| b.books.cmp(&a.books).then_with(|| a.name.cmp(&b.name)));
    out
}

fn formats(books: &[Book]) -> Vec<Count> {
    let mut counts = BTreeMap::new();
    for b in books {
        let kind = b.binding.as_deref().map_or("Other", format_kind);
        *counts.entry(kind.to_owned()).or_default() += 1;
    }
    counted(counts)
}

fn shelves(books: &[Book]) -> Vec<Count> {
    let mut counts = BTreeMap::new();
    for b in books {
        for s in b.custom_shelves() {
            *counts.entry(s.name.clone()).or_default() += 1;
        }
    }
    counted(counts)
}

fn rereads(read: &[&Book]) -> Vec<String> {
    let mut out: Vec<&&Book> = read.iter().filter(|b| b.read_count > 1).collect();
    out.sort_by(|a, b| {
        b.read_count
            .cmp(&a.read_count)
            .then_with(|| a.title.cmp(&b.title))
    });
    out.into_iter().map(|b| b.goodreads_id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_numbers_of_series_entries() {
        assert_eq!(whole_numbers("3"), [3]);
        assert_eq!(whole_numbers("1-3"), [1, 2, 3]);
        assert!(whole_numbers("0.5").is_empty());
        assert!(whole_numbers("2.5").is_empty());
    }

    #[test]
    fn series_from_titles_goodreads_didnt_mark() {
        let found = |t: &str| title_series(t).map(|(n, v)| format!("{n} #{v}"));
        assert_eq!(
            found("Glass Harbor, Vol. 10").as_deref(),
            Some("Glass Harbor #10")
        );
        assert_eq!(found("Saga Volume 3").as_deref(), Some("Saga #3"));
        assert_eq!(
            found("ガラスの港 3 [Glass Harbor 3]").as_deref(),
            Some("Glass Harbor #3")
        );
        assert_eq!(found("Volume Control"), None);
        assert_eq!(found("The Vol. Collection"), None);
        assert_eq!(found("Notes [Draft]"), None);
    }

    #[test]
    fn series_names_group_whatever_the_apostrophe() {
        assert_eq!(
            series_key("Harbor\u{2018}s  Last Light"),
            series_key("harbor's last light")
        );
    }

    #[test]
    fn bindings_become_kinds() {
        assert_eq!(format_kind("Mass Market Paperback"), "Paperback");
        assert_eq!(format_kind("Kindle Edition"), "Ebook");
        assert_eq!(format_kind("Audible Audio"), "Audiobook");
        assert_eq!(format_kind("Leather Bound"), "Hardcover");
        assert_eq!(format_kind("Unknown Binding"), "Other");
    }

    #[test]
    fn median_of_even_and_odd() {
        assert_eq!(median(vec![]), None);
        assert_eq!(median(vec![5, 1, 3]), Some(3));
        assert_eq!(median(vec![4, 1, 3, 2]), Some(2));
    }
}
