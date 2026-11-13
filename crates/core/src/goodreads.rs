//! Reading a Goodreads library export (My Books → Import and export).
//!
//! Goodreads has no public API, so the export CSV is the supported way to get
//! your shelves out. This turns its rows into books without touching the
//! network, a library or a device. Columns are found by header name, so
//! reordered, missing or extra columns don't break it. Quirks handled:
//! - ISBNs are written as `="0618057072"` so spreadsheets keep leading zeros;
//! - the series is a trailing `(Series, #N)` in the title, after any
//!   subtitle or other parenthetical;
//! - a rating of 0 means unrated;
//! - reviews use `<br/>` for line breaks;
//! - dates are `YYYY/MM/DD`, blank when unset;
//! - `Bookshelves` lists every shelf, including the exclusive one, and
//!   `Bookshelves with positions` adds each book's place on it (`favorites (#2)`).

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use chrono::NaiveDate;
use csv::StringRecord;
use parallax_library::{GoodreadsWrite, Library};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum GoodreadsError {
    #[error("couldn't read the Goodreads export: {0}")]
    Csv(#[from] csv::Error),
    #[error("couldn't open the Goodreads export: {0}")]
    Io(#[from] std::io::Error),
    #[error("the Goodreads export has no books, so nothing was changed")]
    Empty,
    #[error("no saved Goodreads book with id {0}")]
    NotSaved(String),
    #[error("couldn't save the books: {0}")]
    Library(#[from] parallax_library::Error),
    #[error("couldn't save a book: {0}")]
    Json(#[from] serde_json::Error),
    #[error("this doesn't look like a Goodreads export: no {0} column")]
    MissingColumn(&'static str),
    #[error("\"{0}\" is a Goodreads status, so set it as the book's status instead")]
    SystemShelf(String),
}

/// The exclusive shelf every Goodreads book is on.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Shelf {
    Read,
    ToRead,
    CurrentlyReading,
    /// Anything else Goodreads may add (it was once "did-not-finish" for some users).
    Other(String),
}

impl Shelf {
    fn parse(name: &str) -> Self {
        match name {
            "read" => Shelf::Read,
            "to-read" => Shelf::ToRead,
            "currently-reading" => Shelf::CurrentlyReading,
            other => Shelf::Other(other.to_owned()),
        }
    }

    /// The name Goodreads uses for it, as written in the export.
    pub fn name(&self) -> &str {
        match self {
            Shelf::Read => "read",
            Shelf::ToRead => "to-read",
            Shelf::CurrentlyReading => "currently-reading",
            Shelf::Other(name) => name,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Series {
    pub name: String,
    /// As Goodreads writes it: "1", "0.5", or a range like "1-3".
    pub number: String,
}

/// A place on a shelf, with its 1-based position when the export gave one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnShelf {
    pub name: String,
    pub position: Option<u32>,
}

/// Saved as JSON in the library, so a field added later needs a default.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Book {
    pub goodreads_id: String,
    /// The title without its trailing "(Series, #N)".
    pub title: String,
    pub series: Option<Series>,
    /// The main author first, then the additional authors (translators, editors).
    #[serde(default)]
    pub authors: Vec<String>,
    pub isbn: Option<String>,
    pub isbn13: Option<String>,
    pub publisher: Option<String>,
    pub binding: Option<String>,
    pub pages: Option<u32>,
    pub year_published: Option<i32>,
    pub original_year: Option<i32>,
    /// 1 to 5; `None` when you haven't rated it.
    pub rating: Option<u8>,
    pub shelf: Shelf,
    /// Every shelf it's on, the exclusive one and your own.
    #[serde(default)]
    pub shelves: Vec<OnShelf>,
    pub date_read: Option<NaiveDate>,
    /// The date read was filled in with the day you marked it read here,
    /// rather than chosen, so an export's real date replaces it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub date_read_guessed: bool,
    pub date_added: Option<NaiveDate>,
    #[serde(default)]
    pub read_count: u32,
    pub review: Option<String>,
}

impl Book {
    /// The shelves you made yourself.
    pub fn custom_shelves(&self) -> impl Iterator<Item = &OnShelf> {
        self.shelves
            .iter()
            .filter(|s| !is_system(&s.name) && s.name != self.shelf.name())
    }
}

pub fn read_file(path: &Path) -> Result<Vec<Book>, GoodreadsError> {
    parse(std::fs::File::open(path)?)
}

pub fn parse(reader: impl Read) -> Result<Vec<Book>, GoodreadsError> {
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(reader);
    let headers = reader.headers()?.clone();
    let columns = Columns::find(&headers)?;
    let mut books = Vec::new();
    for record in reader.records() {
        let record = record?;
        if let Some(book) = columns.book(&record) {
            books.push(book);
        }
    }
    Ok(books)
}

/// Where each column is, found by name (a UTF-8 BOM on the first is ignored).
struct Columns(BTreeMap<String, usize>);

impl Columns {
    fn find(headers: &StringRecord) -> Result<Self, GoodreadsError> {
        let map: BTreeMap<_, _> = headers
            .iter()
            .enumerate()
            .map(|(i, name)| (name.trim_start_matches('\u{feff}').trim().to_owned(), i))
            .collect();
        for required in ["Title", "Author"] {
            if !map.contains_key(required) {
                return Err(GoodreadsError::MissingColumn(required));
            }
        }
        Ok(Self(map))
    }

    fn get<'a>(&self, record: &'a StringRecord, name: &str) -> &'a str {
        self.0
            .get(name)
            .and_then(|&i| record.get(i))
            .map_or("", str::trim)
    }

    fn some<'a>(&self, record: &'a StringRecord, name: &str) -> Option<&'a str> {
        Some(self.get(record, name)).filter(|v| !v.is_empty())
    }

    /// A row with no title is skipped rather than failing the whole import.
    fn book(&self, record: &StringRecord) -> Option<Book> {
        let (title, series) = split_series(self.some(record, "Title")?);
        let mut authors: Vec<String> = Vec::new();
        for name in std::iter::once(self.get(record, "Author"))
            .chain(self.get(record, "Additional Authors").split(','))
        {
            let name = name.trim();
            if !name.is_empty() && !authors.iter().any(|a| a == name) {
                authors.push(name.to_owned());
            }
        }
        let shelf = Shelf::parse(self.get(record, "Exclusive Shelf"));
        Some(Book {
            goodreads_id: self.get(record, "Book Id").to_owned(),
            title,
            series,
            authors,
            isbn: isbn(self.get(record, "ISBN")),
            isbn13: isbn(self.get(record, "ISBN13")),
            publisher: self.some(record, "Publisher").map(str::to_owned),
            binding: self.some(record, "Binding").map(str::to_owned),
            pages: self
                .get(record, "Number of Pages")
                .parse()
                .ok()
                .filter(|&n| n > 0),
            year_published: self
                .get(record, "Year Published")
                .parse()
                .ok()
                .filter(|&y| y != 0),
            original_year: self
                .get(record, "Original Publication Year")
                .parse()
                .ok()
                .filter(|&y| y != 0),
            rating: rating(self.get(record, "My Rating")),
            shelves: shelves(
                self.get(record, "Bookshelves"),
                self.get(record, "Bookshelves with positions"),
            ),
            shelf,
            date_read: date(self.get(record, "Date Read")),
            date_read_guessed: false,
            date_added: date(self.get(record, "Date Added")),
            read_count: self.get(record, "Read Count").parse().unwrap_or(0),
            review: self
                .some(record, "My Review")
                .map(|r| r.replace("<br/>", "\n")),
        })
    }
}

/// `="0618057072"` to `0618057072`; `=""` and plain empty cells to `None`.
fn isbn(cell: &str) -> Option<String> {
    let value = cell
        .strip_prefix("=\"")
        .and_then(|rest| rest.strip_suffix('"'))
        .unwrap_or(cell)
        .trim();
    (!value.is_empty()).then(|| value.to_owned())
}

/// "4.0" is four stars; 0 is unrated.
fn rating(cell: &str) -> Option<u8> {
    let stars: u8 = cell.split('.').next()?.parse().ok()?;
    (1..=5).contains(&stars).then_some(stars)
}

fn date(cell: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(cell, "%Y/%m/%d").ok()
}

/// Splits a trailing "(Series, #N)" off a title. Other parentheticals, such
/// as "We Are Legion (We Are Bob)", stay in the title.
fn split_series(full: &str) -> (String, Option<Series>) {
    let full = full.trim();
    let split = (|| {
        let inner = full.strip_suffix(')')?;
        let open = inner.rfind('(')?;
        let (name, number) = inner[open + 1..].rsplit_once('#')?;
        let name = name.trim().trim_end_matches(',').trim();
        let number = number.trim();
        let title = full[..open].trim_end();
        let looks_numbered = number.chars().next().is_some_and(|c| c.is_ascii_digit());
        (!name.is_empty() && !title.is_empty() && looks_numbered).then(|| {
            (
                title.to_owned(),
                Series {
                    name: name.to_owned(),
                    number: number.to_owned(),
                },
            )
        })
    })();
    match split {
        Some((title, series)) => (title, Some(series)),
        None => (full.to_owned(), None),
    }
}

/// Merges the plain list with the positioned one: "favorites (#2), to-read (#7)".
fn shelves(names: &str, with_positions: &str) -> Vec<OnShelf> {
    let mut out: Vec<OnShelf> = with_positions
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            let (name, position) = match part.rsplit_once(" (#") {
                Some((name, rest)) => (name, rest.strip_suffix(')').and_then(|n| n.parse().ok())),
                None => (part, None),
            };
            (!name.is_empty()).then(|| OnShelf {
                name: name.to_owned(),
                position,
            })
        })
        .collect();
    for name in names.split(',').map(str::trim).filter(|n| !n.is_empty()) {
        if !out.iter().any(|s| s.name == name) {
            out.push(OnShelf {
                name: name.to_owned(),
                position: None,
            });
        }
    }
    out
}

// Importing and merging --------------------------------------------------------
//
// Saved books are keyed by Goodreads Book Id and kept in two copies: what you
// see (`data`) and what the last export said (`base`). Importing a newer export
// merges field by field, for the fields you can edit here (status, rating,
// date read, times read, your shelves and the review):
// - only the export changed it: the export's value;
// - only you changed it: yours;
// - both changed it, differently: a conflict you decide, for the whole book.
// A date read filled in here when you marked the book read gives way to the
// export's date without asking. Everything else (title, ISBN, pages...) always
// follows the export. After a merge the book you see is the result and the
// export's copy is the new export, so the next import compares against it and
// nothing is left half-decided.

/// Fields you can change here that an export can also change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Field {
    Shelf,
    Rating,
    DateRead,
    ReadCount,
    Shelves,
    Review,
}

const FIELDS: [Field; 6] = [
    Field::Shelf,
    Field::Rating,
    Field::DateRead,
    Field::ReadCount,
    Field::Shelves,
    Field::Review,
];

impl Field {
    pub fn label(self) -> &'static str {
        match self {
            Field::Shelf => "Status",
            Field::Rating => "Rating",
            Field::DateRead => "Date read",
            Field::ReadCount => "Times read",
            Field::Shelves => "Your shelves",
            Field::Review => "Review",
        }
    }
}

/// One of Goodreads' own exclusive shelves (read, to-read, currently-reading).
pub fn is_system(name: &str) -> bool {
    !matches!(Shelf::parse(name), Shelf::Other(_))
}

/// The shelves you made, by name. An unusual exclusive shelf (such as
/// did-not-finish) is the book's status, not one of yours.
fn custom_names(book: &Book) -> BTreeSet<String> {
    book.shelves
        .iter()
        .filter(|s| !is_system(&s.name) && s.name != book.shelf.name())
        .map(|s| s.name.clone())
        .collect()
}

/// What to compare: equal values mean nothing changed.
fn value(book: &Book, field: Field) -> serde_json::Value {
    match field {
        Field::Shelf => book.shelf.name().into(),
        Field::Rating => book.rating.into(),
        Field::DateRead => book.date_read.map(|d| d.to_string()).into(),
        Field::ReadCount => book.read_count.into(),
        Field::Shelves => custom_names(book).into_iter().collect::<Vec<_>>().into(),
        Field::Review => book.review.clone().into(),
    }
}

/// What to show, in words.
pub fn describe(book: &Book, field: Field) -> String {
    match field {
        Field::Shelf => match &book.shelf {
            Shelf::Read => "Read".into(),
            Shelf::ToRead => "Want to read".into(),
            Shelf::CurrentlyReading => "Currently reading".into(),
            Shelf::Other(name) => name.clone(),
        },
        Field::Rating => match book.rating {
            None => "No rating".into(),
            Some(1) => "1 star".into(),
            Some(n) => format!("{n} stars"),
        },
        Field::DateRead => book
            .date_read
            .map_or_else(|| "No date".into(), |d| d.format("%b %-d, %Y").to_string()),
        Field::ReadCount => match book.read_count {
            0 => "Not read".into(),
            1 => "Read once".into(),
            n => format!("Read {n} times"),
        },
        Field::Shelves => {
            let names: Vec<_> = custom_names(book).into_iter().collect();
            if names.is_empty() {
                "None".into()
            } else {
                names.join(", ")
            }
        }
        Field::Review => book.review.as_deref().map_or_else(
            || "No review".into(),
            |r| {
                let short: String = r.chars().take(60).collect();
                if short.len() < r.len() {
                    format!("{short}…")
                } else {
                    short
                }
            },
        ),
    }
}

/// Copies one field from `from` into `to`.
fn copy_field(to: &mut Book, from: &Book, field: Field) {
    match field {
        Field::Shelf => to.shelf = from.shelf.clone(),
        Field::Rating => to.rating = from.rating,
        Field::DateRead => {
            to.date_read = from.date_read;
            to.date_read_guessed = from.date_read_guessed;
        }
        Field::ReadCount => to.read_count = from.read_count,
        Field::Review => to.review.clone_from(&from.review),
        Field::Shelves => {} // Rebuilt together with the status; see `rebuild_shelves`.
    }
}

/// Lists each shelf the merged book is on, once, with its place from whichever
/// side knows it. The status's shelf is always listed, even when neither side
/// had an entry for it.
fn rebuild_shelves(out: &mut Book, mine: &Book, theirs: &Book, custom: &BTreeSet<String>) {
    let place = |name: &str| {
        theirs
            .shelves
            .iter()
            .chain(&mine.shelves)
            .find(|s| s.name == name)
            .cloned()
    };
    let status = out.shelf.name().to_owned();
    let status_entry = place(&status).unwrap_or(OnShelf {
        name: status.clone(),
        position: None,
    });
    out.shelves = std::iter::once(status_entry)
        .chain(
            custom
                .iter()
                .filter(|name| **name != status)
                .filter_map(|name| place(name)),
        )
        .collect();
}

/// Who wins when you and the export both changed the same thing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pick {
    /// Keep what you set here. The recommended choice.
    Mine,
    Goodreads,
}

/// What to do with a saved book a newer export no longer has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingPick {
    /// Keep it as a book of your own. The recommended choice.
    Keep,
    Remove,
}

/// Your decisions, by Goodreads Book Id. Anything not listed gets the
/// recommended choice, so `Resolutions::default()` is "use the recommended
/// choices for everything" and no import is ever left half-decided.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resolutions {
    #[serde(default)]
    pub conflicts: BTreeMap<String, Pick>,
    #[serde(default)]
    pub missing: BTreeMap<String, MissingPick>,
}

impl Resolutions {
    fn conflict(&self, id: &str) -> Pick {
        self.conflicts.get(id).copied().unwrap_or(Pick::Mine)
    }

    fn missing(&self, id: &str) -> MissingPick {
        self.missing.get(id).copied().unwrap_or(MissingPick::Keep)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FieldChange {
    pub field: Field,
    pub label: &'static str,
    /// What the book says here now.
    pub mine: String,
    /// What the new export says.
    pub theirs: String,
}

/// A book you changed here and the export changed too, differently.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Conflict {
    pub goodreads_id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub changes: Vec<FieldChange>,
}

/// A saved book the export no longer has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MissingBook {
    pub goodreads_id: String,
    pub title: String,
    pub authors: Vec<String>,
    /// Its status now, in words.
    pub status: String,
}

/// What importing an export would do, before anything is saved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportPlan {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    /// Rows without a Goodreads Book Id, which can't be matched next time.
    pub skipped: usize,
    pub conflicts: Vec<Conflict>,
    pub missing: Vec<MissingBook>,
    /// A look at the export itself, to show before importing.
    pub summary: ExportSummary,
}

/// What an export holds: how many books, on which shelves, and how many are
/// already in the library.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ExportSummary {
    pub books: usize,
    pub read: usize,
    pub to_read: usize,
    pub currently_reading: usize,
    /// Your own shelves, biggest first, with whether they're new to you here.
    pub shelves: Vec<ShelfSummary>,
    /// Books in the export that match a book in your library.
    pub in_library: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShelfSummary {
    pub name: String,
    pub books: usize,
    /// Not on any book you've saved from an earlier export.
    pub new: bool,
}

/// Describes an export against what's saved and what's in the library.
pub fn summarize(
    saved: &[Book],
    books: &[Book],
    library: &[parallax_library::Book],
) -> ExportSummary {
    let known: BTreeSet<&str> = saved
        .iter()
        .flat_map(|b| b.shelves.iter().map(|s| s.name.as_str()))
        .collect();
    let custom = |name: &str| {
        !matches!(
            Shelf::parse(name),
            Shelf::Read | Shelf::ToRead | Shelf::CurrentlyReading
        )
    };
    let mut shelves: Vec<ShelfSummary> = shelf_counts(books)
        .into_iter()
        .filter(|(name, _)| custom(name))
        .map(|(name, count)| ShelfSummary {
            new: !known.contains(name.as_str()),
            name,
            books: count,
        })
        .collect();
    shelves.sort_by(|a, b| b.books.cmp(&a.books).then_with(|| a.name.cmp(&b.name)));
    let count = |shelf: &Shelf| books.iter().filter(|b| &b.shelf == shelf).count();
    let in_library = library_matches(books, library)
        .values()
        .map(|m| &m.goodreads_id)
        .collect::<BTreeSet<_>>()
        .len();
    ExportSummary {
        books: books.len(),
        read: count(&Shelf::Read),
        to_read: count(&Shelf::ToRead),
        currently_reading: count(&Shelf::CurrentlyReading),
        shelves,
        in_library,
    }
}

impl ImportPlan {
    /// Whether anything needs a decision. If not, importing needs no questions.
    pub fn needs_decisions(&self) -> bool {
        !self.conflicts.is_empty() || !self.missing.is_empty()
    }
}

/// What importing did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    pub added: usize,
    pub updated: usize,
    pub unchanged: usize,
    pub skipped: usize,
    /// Books the export no longer had, kept as your own.
    pub kept: usize,
    /// Books the export no longer had, removed.
    pub removed: usize,
    /// Conflicts settled by keeping your changes.
    pub kept_yours: usize,
    /// Conflicts settled by taking the export's.
    pub took_goodreads: usize,
}

struct Merged {
    book: Book,
    conflicts: Vec<FieldChange>,
}

/// Merges one book: yours, the last export's and the new export's.
fn merge(mine: &Book, base: &Book, theirs: &Book, pick: Pick) -> Merged {
    let mut out = theirs.clone();
    let mut conflicts = Vec::new();
    let mut from_mine = Vec::new();
    for field in FIELDS {
        let (m, b, t) = (value(mine, field), value(base, field), value(theirs, field));
        if m == b || m == t {
            continue; // the export's value, which is yours too or unchanged by you
        }
        if t == b {
            from_mine.push(field); // only you changed it
        } else if field == Field::DateRead && mine.date_read_guessed {
            // The export's real date wins over the day you marked it read.
        } else {
            conflicts.push(FieldChange {
                field,
                label: field.label(),
                mine: describe(mine, field),
                theirs: describe(theirs, field),
            });
            if pick == Pick::Mine {
                from_mine.push(field);
            }
        }
    }
    for &field in &from_mine {
        copy_field(&mut out, mine, field);
    }
    if from_mine
        .iter()
        .any(|f| matches!(f, Field::Shelf | Field::Shelves))
    {
        let custom = if from_mine.contains(&Field::Shelves) {
            custom_names(mine)
        } else {
            custom_names(theirs)
        };
        rebuild_shelves(&mut out, mine, theirs, &custom);
    }
    Merged {
        book: out,
        conflicts,
    }
}

struct Saved {
    mine: Book,
    base: Book,
    local_only: bool,
}

fn reconcile(
    saved: &BTreeMap<String, Saved>,
    incoming: &[Book],
    resolutions: &Resolutions,
) -> Result<(ImportPlan, ImportReport, Vec<GoodreadsWrite>), GoodreadsError> {
    let mut latest: BTreeMap<&str, &Book> = BTreeMap::new();
    let mut skipped = 0;
    for book in incoming {
        if book.goodreads_id.is_empty() {
            skipped += 1;
        } else {
            latest.insert(&book.goodreads_id, book);
        }
    }
    if latest.is_empty() {
        return Err(GoodreadsError::Empty);
    }
    let mut plan = ImportPlan {
        skipped,
        ..ImportPlan::default()
    };
    let mut report = ImportReport {
        skipped,
        ..ImportReport::default()
    };
    let mut writes = Vec::new();
    for (&id, &theirs) in &latest {
        let Some(old) = saved.get(id) else {
            plan.added += 1;
            report.added += 1;
            writes.push(GoodreadsWrite::Add {
                id: id.to_owned(),
                data: serde_json::to_string(theirs)?,
            });
            continue;
        };
        let pick = resolutions.conflict(id);
        let merged = merge(&old.mine, &old.base, theirs, pick);
        if !merged.conflicts.is_empty() {
            plan.conflicts.push(Conflict {
                goodreads_id: id.to_owned(),
                title: theirs.title.clone(),
                authors: theirs.authors.clone(),
                changes: merged.conflicts.clone(),
            });
            match pick {
                Pick::Mine => report.kept_yours += 1,
                Pick::Goodreads => report.took_goodreads += 1,
            }
        }
        let changed = merged.book != old.mine;
        if changed {
            plan.updated += 1;
            report.updated += 1;
        } else {
            plan.unchanged += 1;
            report.unchanged += 1;
        }
        if changed || *theirs != old.base || old.local_only {
            writes.push(GoodreadsWrite::Merge {
                id: id.to_owned(),
                data: serde_json::to_string(&merged.book)?,
                base: serde_json::to_string(theirs)?,
            });
        }
    }
    for (id, old) in saved {
        if latest.contains_key(id.as_str()) || old.local_only {
            continue;
        }
        plan.missing.push(MissingBook {
            goodreads_id: id.clone(),
            title: old.mine.title.clone(),
            authors: old.mine.authors.clone(),
            status: describe(&old.mine, Field::Shelf),
        });
        match resolutions.missing(id) {
            MissingPick::Keep => {
                report.kept += 1;
                writes.push(GoodreadsWrite::KeepLocal { id: id.clone() });
            }
            MissingPick::Remove => {
                report.removed += 1;
                writes.push(GoodreadsWrite::Remove { id: id.clone() });
            }
        }
    }
    Ok((plan, report, writes))
}

fn load(lib: &Library) -> Result<BTreeMap<String, Saved>, GoodreadsError> {
    lib.goodreads_saved()?
        .into_iter()
        .map(|row| {
            Ok((
                row.goodreads_id,
                Saved {
                    mine: serde_json::from_str(&row.data)?,
                    base: serde_json::from_str(&row.base)?,
                    local_only: row.local_only,
                },
            ))
        })
        .collect()
}

/// What importing this export would change and what needs your decision.
/// Writes nothing. An export with no books is refused, so a wrong file can't
/// make every saved book look missing.
pub fn plan_file(lib: &Library, path: &Path) -> Result<ImportPlan, GoodreadsError> {
    plan(lib, &read_file(path)?)
}

pub fn plan(lib: &Library, books: &[Book]) -> Result<ImportPlan, GoodreadsError> {
    let saved = load(lib)?;
    let mut plan = reconcile(&saved, books, &Resolutions::default())?.0;
    let mine: Vec<Book> = saved.into_values().map(|s| s.mine).collect();
    plan.summary = summarize(&mine, books, &lib.books()?);
    Ok(plan)
}

/// Saves an export with your decisions, all together or not at all. Anything
/// you didn't decide gets the recommended choice. Importing the same file
/// again changes nothing.
pub fn import_file(
    lib: &mut Library,
    path: &Path,
    resolutions: &Resolutions,
) -> Result<ImportReport, GoodreadsError> {
    import(lib, &read_file(path)?, resolutions)
}

pub fn import(
    lib: &mut Library,
    books: &[Book],
    resolutions: &Resolutions,
) -> Result<ImportReport, GoodreadsError> {
    let (_, report, writes) = reconcile(&load(lib)?, books, resolutions)?;
    lib.goodreads_write(&writes)?;
    Ok(report)
}

/// [`import`] with one choice for every conflict and every book the export no
/// longer has, without planning first.
pub fn import_choosing(
    lib: &mut Library,
    books: &[Book],
    conflicts: Pick,
    missing: MissingPick,
) -> Result<ImportReport, GoodreadsError> {
    let saved = load(lib)?;
    let resolutions = Resolutions {
        conflicts: saved.keys().map(|id| (id.clone(), conflicts)).collect(),
        missing: saved.keys().map(|id| (id.clone(), missing)).collect(),
    };
    let (_, report, writes) = reconcile(&saved, books, &resolutions)?;
    lib.goodreads_write(&writes)?;
    Ok(report)
}

/// The saved books as you see them.
pub fn saved(lib: &Library) -> Result<Vec<Book>, GoodreadsError> {
    Ok(load(lib)?.into_values().map(|s| s.mine).collect())
}

/// What your Goodreads shelves say about a book in the library.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LibraryMatch {
    pub goodreads_id: String,
    pub shelf: Shelf,
    pub rating: Option<u8>,
    pub date_read: Option<NaiveDate>,
    /// Your own shelves it's on.
    pub shelves: Vec<String>,
}

/// Finds the library books on your saved Goodreads shelves, by ISBN first and
/// then by title and author (the same way duplicates are found), keyed by
/// library book id. Each library book matches at most one Goodreads book, the
/// first saved one with that key. One Goodreads book can match several library
/// books, such as two editions or copies of it, and each shows its status.
pub fn library_matches(
    saved: &[Book],
    books: &[parallax_library::Book],
) -> std::collections::HashMap<i64, LibraryMatch> {
    use crate::duplicates::{normalise_isbn, title_author_key};
    use std::collections::HashMap;
    let mut by_isbn: HashMap<String, &Book> = HashMap::new();
    let mut by_title: HashMap<String, &Book> = HashMap::new();
    for g in saved {
        for isbn in [&g.isbn, &g.isbn13].into_iter().flatten() {
            let key = normalise_isbn(isbn);
            if !key.is_empty() {
                by_isbn.entry(key).or_insert(g);
            }
        }
        if let Some(key) = title_author_key(&g.title, &g.authors) {
            by_title.entry(key).or_insert(g);
        }
    }
    let mut out = HashMap::new();
    for b in books {
        let by_number = b
            .identifiers
            .iter()
            .map(|i| normalise_isbn(i.value.trim_start_matches("urn:isbn:")))
            .find_map(|k| by_isbn.get(&k).copied());
        let found = by_number.or_else(|| {
            title_author_key(&b.title, &b.authors).and_then(|k| by_title.get(&k).copied())
        });
        if let Some(g) = found {
            out.insert(
                b.id,
                LibraryMatch {
                    goodreads_id: g.goodreads_id.clone(),
                    shelf: g.shelf.clone(),
                    rating: g.rating,
                    date_read: g.date_read,
                    shelves: g.custom_shelves().map(|s| s.name.clone()).collect(),
                },
            );
        }
    }
    out
}

/// A change you make here. Anything left out stays as it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Edit {
    pub shelf: Option<Shelf>,
    /// 1 to 5; 0 clears it.
    pub rating: Option<u8>,
    pub date_read: Option<NaiveDate>,
    /// Remove the date read (ignored if `date_read` is set).
    #[serde(default)]
    pub clear_date_read: bool,
    #[serde(default)]
    pub add_shelves: Vec<String>,
    #[serde(default)]
    pub remove_shelves: Vec<String>,
}

/// [`edit`], dating a newly read book with today's date.
pub fn edit_today(
    lib: &mut Library,
    goodreads_id: &str,
    change: &Edit,
) -> Result<Book, GoodreadsError> {
    edit(lib, goodreads_id, change, chrono::Local::now().date_naive())
}

/// Applies an edit to a saved book. Marking it read also counts one read and
/// dates it `today` if it had no date. The export's copy is untouched, so a
/// later export can tell what you changed.
pub fn edit(
    lib: &mut Library,
    goodreads_id: &str,
    edit: &Edit,
    today: NaiveDate,
) -> Result<Book, GoodreadsError> {
    let mut book = load(lib)?
        .remove(goodreads_id)
        .ok_or_else(|| GoodreadsError::NotSaved(goodreads_id.to_owned()))?
        .mine;
    if let Some(shelf) = &edit.shelf {
        if *shelf == Shelf::Read && book.shelf != Shelf::Read {
            book.read_count = book.read_count.max(1);
            if book.date_read.is_none() && edit.date_read.is_none() && !edit.clear_date_read {
                book.date_read = Some(today);
                book.date_read_guessed = true;
            }
        }
        if *shelf != book.shelf {
            // The old status's entry goes; positions on it no longer apply.
            book.shelves.retain(|s| s.name != book.shelf.name());
            book.shelves.insert(
                0,
                OnShelf {
                    name: shelf.name().to_owned(),
                    position: None,
                },
            );
            book.shelf = shelf.clone();
        }
    }
    if let Some(rating) = edit.rating {
        book.rating = (1..=5).contains(&rating).then_some(rating);
    }
    if let Some(date) = edit.date_read {
        book.date_read = Some(date);
        book.date_read_guessed = false;
    } else if edit.clear_date_read {
        book.date_read = None;
        book.date_read_guessed = false;
    }
    // Only your own shelves come and go here; the status is set above.
    book.shelves
        .retain(|s| is_system(&s.name) || !edit.remove_shelves.contains(&s.name));
    for name in &edit.add_shelves {
        if is_system(name) {
            return Err(GoodreadsError::SystemShelf(name.clone()));
        }
        if !book.shelves.iter().any(|s| &s.name == name) {
            book.shelves.push(OnShelf {
                name: name.clone(),
                position: None,
            });
        }
    }
    lib.goodreads_write(&[GoodreadsWrite::Edit {
        id: goodreads_id.to_owned(),
        data: serde_json::to_string(&book)?,
    }])?;
    Ok(book)
}

/// How many books are on each shelf, the exclusive ones and your own.
pub fn shelf_counts(books: &[Book]) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for book in books {
        for shelf in &book.shelves {
            *counts.entry(shelf.name.clone()).or_default() += 1;
        }
        if !book.shelves.iter().any(|s| s.name == book.shelf.name()) {
            *counts.entry(book.shelf.name().to_owned()).or_default() += 1;
        }
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library_book(
        id: i64,
        title: &str,
        author: &str,
        isbn: Option<&str>,
    ) -> parallax_library::Book {
        parallax_library::Book {
            id,
            title: title.into(),
            authors: vec![author.into()],
            contributors: None,
            series: None,
            series_index: None,
            tags: vec![],
            language: None,
            publisher: None,
            published: None,
            description: None,
            identifiers: isbn
                .map(|v| parallax_epub::Identifier {
                    scheme: Some("isbn".into()),
                    value: v.into(),
                })
                .into_iter()
                .collect(),
            words: None,
            pages: None,
            pages_estimated: true,
            original_name: "b.epub".into(),
            file_size: 1,
            format: parallax_library::Format::Epub,
            kind: parallax_library::Kind::Book,
            direction: parallax_library::Direction::Ltr,
            has_cover: false,
            custom_cover: false,
            cover_upscaled: false,
            added_at: String::new(),
            modified_at: String::new(),
        }
    }

    #[test]
    fn matches_library_books_by_isbn_then_by_title_and_author() {
        let books = parse_rows(&[
            r#"1,"The Color of Magic (Discworld, #1)",Terry Pratchett,"Pratchett, Terry",,="0061020656",="9780061020650",5,,,,,,2026/01/02,,read,read (#1),read,,,1,"#,
            r#"2,Emma,Jane Austen,"Austen, Jane",,="",="",0,,,,,,,,to-read,to-read (#1),to-read,,,0,"#,
        ]);
        let library = [
            library_book(
                10,
                "Colour of Magic",
                "T. Pratchett",
                Some("urn:isbn:978-0-06-102065-0"),
            ),
            library_book(11, "Emma", "Jane Austen", None),
            library_book(12, "Something Else", "Nobody", None),
        ];
        let found = library_matches(&books, &library);
        assert_eq!(found.len(), 2);
        assert_eq!(found[&10].goodreads_id, "1");
        assert_eq!(found[&10].shelf, Shelf::Read);
        assert_eq!(found[&10].rating, Some(5));
        assert_eq!(found[&11].shelf, Shelf::ToRead);
        assert!(!found.contains_key(&12));
    }

    #[test]
    fn summarizes_an_export_and_flags_new_shelves() {
        let books = parse_rows(&[
            r#"1,One,Ann Author,"Author, Ann",,="",="",0,,,,,,,,"read, favorites","read (#1), favorites (#1)",read,,,1,"#,
            r#"2,Two,Ann Author,"Author, Ann",,="",="",0,,,,,,,,"to-read, sci-fi","to-read (#1), sci-fi (#1)",to-read,,,0,"#,
        ]);
        let earlier = parse_rows(&[
            r#"3,Old,Ann Author,"Author, Ann",,="",="",0,,,,,,,,"read, favorites","read (#1), favorites (#1)",read,,,1,"#,
        ]);
        let summary = summarize(&earlier, &books, &[]);
        assert_eq!((summary.books, summary.read, summary.to_read), (2, 1, 1));
        assert_eq!(summary.currently_reading, 0);
        let names: Vec<(&str, bool)> = summary
            .shelves
            .iter()
            .map(|s| (s.name.as_str(), s.new))
            .collect();
        assert_eq!(names, [("favorites", false), ("sci-fi", true)]);
    }

    const HEADER: &str = "Book Id,Title,Author,Author l-f,Additional Authors,ISBN,ISBN13,My Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Bookshelves with positions,Exclusive Shelf,My Review,Spoiler,Private Notes,Read Count,Owned Copies";

    fn parse_rows(rows: &[&str]) -> Vec<Book> {
        let text = format!("{HEADER}\n{}\n", rows.join("\n"));
        parse(text.as_bytes()).unwrap()
    }

    #[test]
    fn an_unusual_status_is_listed_once_after_a_merge() {
        let base = parse_rows(&[
            r#"1,One,Ann Author,"Author, Ann",,="",="",0,,,,,,,,"did-not-finish, sci-fi","did-not-finish (#1), sci-fi (#1)",did-not-finish,,,0,"#,
        ])
        .remove(0);
        let shelf = |name: &str| OnShelf {
            name: name.into(),
            position: None,
        };
        let mut mine = base.clone();
        mine.shelves.push(shelf("horror"));
        let mut theirs = base.clone();
        theirs.shelves.push(shelf("fantasy"));
        let merged = merge(&mine, &base, &theirs, Pick::Mine);
        assert_eq!(merged.conflicts.len(), 1);
        let names: Vec<&str> = merged
            .book
            .shelves
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert_eq!(names, ["did-not-finish", "horror", "sci-fi"]);
    }

    #[test]
    fn splits_the_series_off_the_title() {
        let cases = [
            ("Dune (Dune, #1)", "Dune", Some(("Dune", "1"))),
            (
                "Dragons and Demons (King‘s Dark Tidings, #5)",
                "Dragons and Demons",
                Some(("King‘s Dark Tidings", "5")),
            ),
            (
                "We Are Legion (We Are Bob) (Bobiverse, #1)",
                "We Are Legion (We Are Bob)",
                Some(("Bobiverse", "1")),
            ),
            (
                "Novella (Cosmere, #0.5)",
                "Novella",
                Some(("Cosmere", "0.5")),
            ),
            (
                "Box Set (Trilogy, #1-3)",
                "Box Set",
                Some(("Trilogy", "1-3")),
            ),
            ("No Number (Just A Note)", "No Number (Just A Note)", None),
            (
                "Plain Title: With Subtitle",
                "Plain Title: With Subtitle",
                None,
            ),
            ("(Series, #1)", "(Series, #1)", None),
            ("Hashtag (Is #winning)", "Hashtag (Is #winning)", None),
        ];
        for (full, title, series) in cases {
            let (got_title, got_series) = split_series(full);
            assert_eq!(got_title, title, "{full}");
            let want = series.map(|(name, number)| Series {
                name: name.into(),
                number: number.into(),
            });
            assert_eq!(got_series, want, "{full}");
        }
    }

    #[test]
    fn reads_a_full_row() {
        let books = parse_rows(&[
            r#"169485073,"The Strength of the Few (Hierarchy, #2)",James Islington,"Islington, James","Myra Ramos, Donaldo Macedo","=""0618057072""","=""9780618057078""",5.0,Text Publishing,Hardcover,736,2025,2024,2026/06/21,2026/06/16,"favorites, favourite-scifi","favorites (#2), favourite-scifi (#3)",read,"Pros<br/>Fun<br/><br/>Cons<br/>None",,,2,0"#,
        ]);
        let [book] = books.as_slice() else {
            panic!("one book")
        };
        assert_eq!(book.goodreads_id, "169485073");
        assert_eq!(book.title, "The Strength of the Few");
        assert_eq!(
            book.series,
            Some(Series {
                name: "Hierarchy".into(),
                number: "2".into()
            })
        );
        assert_eq!(
            book.authors,
            ["James Islington", "Myra Ramos", "Donaldo Macedo"]
        );
        assert_eq!(book.isbn.as_deref(), Some("0618057072"));
        assert_eq!(book.isbn13.as_deref(), Some("9780618057078"));
        assert_eq!(book.rating, Some(5));
        assert_eq!(book.shelf, Shelf::Read);
        assert_eq!(
            (book.pages, book.year_published, book.original_year),
            (Some(736), Some(2025), Some(2024))
        );
        assert_eq!(book.date_read, NaiveDate::from_ymd_opt(2026, 6, 21));
        assert_eq!(book.date_added, NaiveDate::from_ymd_opt(2026, 6, 16));
        assert_eq!(book.read_count, 2);
        assert_eq!(book.review.as_deref(), Some("Pros\nFun\n\nCons\nNone"));
        assert_eq!(
            book.shelves,
            [
                OnShelf {
                    name: "favorites".into(),
                    position: Some(2)
                },
                OnShelf {
                    name: "favourite-scifi".into(),
                    position: Some(3)
                },
            ]
        );
    }

    #[test]
    fn unrated_unread_and_blank_cells_are_none() {
        let books = parse_rows(&[
            r#"1,Plain,Someone,"Someone",,"=""""","=""""",0,,,,,,,2026/09/13,to-read,to-read (#166),to-read,,,,0,0"#,
            r"2,Zero,Someone,Someone,,,,0.0,,,,0,0,,2026/09/13,,,read,,,,1,0",
        ]);
        let book = &books[0];
        assert_eq!((book.isbn.as_deref(), book.isbn13.as_deref()), (None, None));
        assert_eq!(book.rating, None);
        assert_eq!(
            (
                book.publisher.as_deref(),
                book.binding.as_deref(),
                book.pages
            ),
            (None, None, None)
        );
        assert_eq!(book.date_read, None);
        assert_eq!(book.review, None);
        assert_eq!(book.shelf, Shelf::ToRead);
        assert_eq!(
            book.shelves,
            [OnShelf {
                name: "to-read".into(),
                position: Some(166)
            }]
        );
        assert_eq!(books[1].rating, None);
        assert_eq!(
            (books[1].year_published, books[1].original_year),
            (None, None)
        );
        assert!(books[1].shelves.is_empty());
    }

    #[test]
    fn finds_columns_by_name_whatever_their_order() {
        let text =
            "\u{feff}Exclusive Shelf,Author,Title,Extra\nread,Ann Author,\"A Book (S, #1)\",x\n";
        let books = parse(text.as_bytes()).unwrap();
        assert_eq!(books[0].title, "A Book");
        assert_eq!(books[0].authors, ["Ann Author"]);
        assert_eq!(books[0].shelf, Shelf::Read);
        assert_eq!(books[0].goodreads_id, "");
    }

    #[test]
    fn rejects_other_csv_files() {
        let err = parse("Name,Price\nA,1\n".as_bytes()).unwrap_err();
        assert!(
            matches!(err, GoodreadsError::MissingColumn("Title")),
            "{err}"
        );
    }

    #[test]
    fn counts_every_shelf() {
        let books = parse_rows(&[
            r"1,A,X,X,,,,0,,,,,,,2026/01/01,favorites,favorites (#1),read,,,,1,0",
            r"2,B,X,X,,,,0,,,,,,,2026/01/01,to-read,to-read (#1),to-read,,,,0,0",
            r"3,C,X,X,,,,0,,,,,,,2026/01/01,,,read,,,,1,0",
        ]);
        let counts = shelf_counts(&books);
        assert_eq!(counts["read"], 2);
        assert_eq!(counts["to-read"], 1);
        assert_eq!(counts["favorites"], 1);
        assert_eq!(books[0].custom_shelves().count(), 1);
        assert_eq!(books[1].custom_shelves().count(), 0);
    }
}
