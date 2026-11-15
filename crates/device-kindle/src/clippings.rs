//! `documents/My Clippings.txt`, where a Kindle appends every highlight, note
//! and bookmark with its text. calibre imports the whole file as one book
//! (`devices/kindle/driver.py`, `add_annotation_to_library`); this splits it
//! into entries so each can be filed under its own book.
//!
//! Each entry is a title line, a details line, a blank line and the text,
//! ending with a line of ten `=`. Only English details lines are fully
//! understood. Other languages keep their text but are taken as highlights.

/// One entry from the clippings file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Clipping {
    /// The book's title as the Kindle shows it.
    pub title: String,
    pub author: Option<String>,
    /// "highlight", "note" or "bookmark".
    pub kind: &'static str,
    pub page: Option<String>,
    /// Kindle location, the start of a range like `120-125`.
    pub location: Option<u32>,
    /// The "Added on" text, as written.
    pub added: Option<String>,
    pub text: String,
}

const SEPARATOR: &str = "==========";

/// Every entry, in file order. Malformed entries are skipped.
pub fn parse(content: &str) -> Vec<Clipping> {
    content
        .trim_start_matches('\u{feff}')
        .split(SEPARATOR)
        .filter_map(parse_entry)
        .collect()
}

fn parse_entry(entry: &str) -> Option<Clipping> {
    let mut lines = entry
        .lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}'))
        .skip_while(|l| l.is_empty());
    let (title, author) = split_title(lines.next()?);
    let details = lines.next()?;
    if !details.starts_with('-') {
        return None;
    }
    let text = lines.collect::<Vec<_>>().join("\n").trim().to_owned();
    let lower = details.to_lowercase();
    let kind = if lower.contains("bookmark") {
        "bookmark"
    } else if lower.contains(" note ") || lower.contains("your note") {
        "note"
    } else {
        "highlight"
    };
    let mut page = None;
    let mut location = None;
    let mut added = None;
    for part in details.trim_start_matches('-').split('|').map(str::trim) {
        let lower = part.to_lowercase();
        if let Some(at) = lower.find("added on") {
            added = Some(part[at + "added on".len()..].trim().to_owned());
        } else if let Some(at) = lower.find("page") {
            page = part[at + 4..].split_whitespace().next().map(str::to_owned);
        }
        if let Some(at) = lower.find("location") {
            location = part[at + "location".len()..]
                .trim()
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse().ok());
        }
    }
    Some(Clipping {
        title,
        author,
        kind,
        page,
        location,
        added,
        text,
    })
}

/// `Title (Author)` → ("Title", Some("Author")). Only a final parenthesised
/// group counts as the author, so "Book (Vol. 2) (Author)" keeps its volume.
fn split_title(line: &str) -> (String, Option<String>) {
    if let Some(stripped) = line.strip_suffix(')') {
        let mut depth = 0;
        for (i, c) in stripped.char_indices().rev() {
            match c {
                ')' => depth += 1,
                '(' if depth == 0 => {
                    let title = stripped[..i].trim();
                    let author = stripped[i + 1..].trim();
                    if !title.is_empty() && !author.is_empty() {
                        return (title.to_owned(), Some(author.to_owned()));
                    }
                    break;
                }
                '(' => depth -= 1,
                _ => {}
            }
        }
    }
    (line.to_owned(), None)
}

/// A stable id for a clipping, so syncing twice updates rather than adds.
pub fn id(clipping: &Clipping) -> String {
    let key = format!(
        "{}\n{}\n{:?}\n{:?}\n{}",
        clipping.title, clipping.kind, clipping.location, clipping.page, clipping.text
    );
    format!(
        "clip-{}",
        &sha1_smol::Sha1::from(key).digest().to_string()[..16]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\u{feff}The Enchiridion (Epictetus)\r
- Your Highlight on page 3 | Location 41-43 | Added on Monday, 3 November 2025 21:04:10\r
\r
Some things are in our control and others not.\r
==========\r
The Enchiridion (Epictetus)\r
- Your Note on Location 43 | Added on Monday, 3 November 2025 21:05:00\r
\r
Remember this.\r
==========\r
Meditations: Book (Vol. 2) (Marcus Aurelius)\r
- Your Bookmark on Location 120 | Added on Tuesday, 4 November 2025 08:00:00\r
\r
\r
==========\r
broken entry without details\r
==========\r
";

    #[test]
    fn parses_entries() {
        let c = parse(SAMPLE);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0].title, "The Enchiridion");
        assert_eq!(c[0].author.as_deref(), Some("Epictetus"));
        assert_eq!(c[0].kind, "highlight");
        assert_eq!(c[0].page.as_deref(), Some("3"));
        assert_eq!(c[0].location, Some(41));
        assert_eq!(
            c[0].added.as_deref(),
            Some("Monday, 3 November 2025 21:04:10")
        );
        assert_eq!(c[0].text, "Some things are in our control and others not.");
        assert_eq!(c[1].kind, "note");
        assert_eq!(c[1].location, Some(43));
        assert_eq!(c[2].title, "Meditations: Book (Vol. 2)");
        assert_eq!(c[2].kind, "bookmark");
        assert_eq!(c[2].text, "");
        assert_ne!(id(&c[0]), id(&c[1]));
        assert_eq!(id(&c[0]), id(&parse(SAMPLE)[0]));
    }

    #[test]
    fn title_without_author() {
        assert_eq!(split_title("Notes"), ("Notes".into(), None));
        assert_eq!(split_title("()"), ("()".into(), None));
    }
}
