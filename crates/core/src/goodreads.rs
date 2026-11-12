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

use std::collections::BTreeMap;
use std::io::Read;
use std::path::Path;

use chrono::NaiveDate;
use csv::StringRecord;
use parallax_library::{GoodreadsSync, Library};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum GoodreadsError {
    #[error("couldn't read the Goodreads export: {0}")]
    Csv(#[from] csv::Error),
    #[error("couldn't open the Goodreads export: {0}")]
    Io(#[from] std::io::Error),
    #[error("the Goodreads export has no books, so nothing was changed")]
    Empty,
    #[error("couldn't save the books: {0}")]
    Library(#[from] parallax_library::Error),
    #[error("couldn't save a book: {0}")]
    Json(#[from] serde_json::Error),
    #[error("this doesn't look like a Goodreads export: no {0} column")]
    MissingColumn(&'static str),
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
    pub date_added: Option<NaiveDate>,
    #[serde(default)]
    pub read_count: u32,
    pub review: Option<String>,
}

impl Book {
    /// The shelves you made yourself.
    pub fn custom_shelves(&self) -> impl Iterator<Item = &OnShelf> {
        self.shelves.iter().filter(|s| {
            !matches!(
                Shelf::parse(&s.name),
                Shelf::Read | Shelf::ToRead | Shelf::CurrentlyReading
            )
        })
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

/// What importing an export did to the saved books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImportReport {
    #[serde(flatten)]
    pub sync: GoodreadsSync,
    /// Rows without a Goodreads Book Id, which can't be matched next time.
    pub skipped: usize,
}

/// Saves an export, updating books imported from an earlier one by their
/// Goodreads Book Id. Safe to repeat: the same file changes nothing, a newer
/// one adds and updates books, and books it no longer has are flagged, never
/// deleted. An export with no books is refused, so a wrong file can't flag
/// everything.
pub fn import(lib: &mut Library, path: &Path) -> Result<ImportReport, GoodreadsError> {
    save(lib, &read_file(path)?)
}

pub fn save(lib: &mut Library, books: &[Book]) -> Result<ImportReport, GoodreadsError> {
    if books.is_empty() {
        return Err(GoodreadsError::Empty);
    }
    let total = books.len();
    let rows = books
        .iter()
        .filter(|b| !b.goodreads_id.is_empty())
        .map(|b| Ok((b.goodreads_id.clone(), serde_json::to_string(b)?)))
        .collect::<Result<Vec<_>, serde_json::Error>>()?;
    let skipped = total - rows.len();
    if rows.is_empty() {
        return Err(GoodreadsError::Empty);
    }
    Ok(ImportReport {
        sync: lib.goodreads_sync(&rows)?,
        skipped,
    })
}

/// A saved book, and whether the latest export no longer had it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SavedBook {
    #[serde(flatten)]
    pub book: Book,
    pub missing: bool,
}

pub fn saved(lib: &Library) -> Result<Vec<SavedBook>, GoodreadsError> {
    lib.goodreads_saved()?
        .into_iter()
        .map(|row| {
            Ok(SavedBook {
                book: serde_json::from_str(&row.data)?,
                missing: row.missing,
            })
        })
        .collect()
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

    const HEADER: &str = "Book Id,Title,Author,Author l-f,Additional Authors,ISBN,ISBN13,My Rating,Publisher,Binding,Number of Pages,Year Published,Original Publication Year,Date Read,Date Added,Bookshelves,Bookshelves with positions,Exclusive Shelf,My Review,Spoiler,Private Notes,Read Count,Owned Copies";

    fn parse_rows(rows: &[&str]) -> Vec<Book> {
        let text = format!("{HEADER}\n{}\n", rows.join("\n"));
        parse(text.as_bytes()).unwrap()
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
