//! Exporting a book's highlights and notes.

use parallax_library::{Annotation, Book};
use serde::Serialize;

/// Readable notes: the book, then each chapter's highlights as quotes with
/// their notes. Bookmarks (no text) are left out.
pub fn markdown(book: &Book, items: &[Annotation]) -> String {
    let mut out = format!("# {}\n", book.title);
    if !book.authors.is_empty() {
        out.push_str(&format!("\n{}\n", book.authors.join(", ")));
    }
    let mut chapter: Option<&str> = None;
    for a in items
        .iter()
        .filter(|a| a.text.is_some() || a.note.is_some())
    {
        let this = a.chapter.as_deref();
        if this != chapter && this.is_some() {
            out.push_str(&format!("\n## {}\n", this.unwrap_or_default()));
        }
        chapter = this;
        out.push('\n');
        if let Some(text) = &a.text {
            for line in text.lines() {
                out.push_str(&format!("> {line}\n"));
            }
        }
        if let Some(note) = &a.note {
            if a.text.is_some() {
                out.push('\n');
            }
            out.push_str(&format!("{note}\n"));
        }
        if !a.on_device {
            out.push_str("\n*Removed from the Kobo.*\n");
        }
    }
    out
}

#[derive(Serialize)]
struct Export<'a> {
    title: &'a str,
    authors: &'a [String],
    series: Option<&'a str>,
    annotations: &'a [Annotation],
}

/// Everything, for other tools.
pub fn json(book: &Book, items: &[Annotation]) -> String {
    serde_json::to_string_pretty(&Export {
        title: &book.title,
        authors: &book.authors,
        series: book.series.as_deref(),
        annotations: items,
    })
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(
        chapter: Option<&str>,
        text: Option<&str>,
        note: Option<&str>,
        on_device: bool,
    ) -> Annotation {
        Annotation {
            id: "x".into(),
            device_serial: "N1".into(),
            kind: if note.is_some() { "note" } else { "highlight" }.into(),
            text: text.map(Into::into),
            note: note.map(Into::into),
            chapter: chapter.map(Into::into),
            position: 0.0,
            created: None,
            modified: None,
            on_device,
        }
    }

    #[test]
    fn markdown_groups_by_chapter() {
        let book = Book {
            id: 1,
            title: "Dune".into(),
            authors: vec!["Frank Herbert".into()],
            contributors: None,
            series: None,
            series_index: None,
            tags: vec![],
            language: None,
            publisher: None,
            published: None,
            description: None,
            identifiers: vec![],
            words: None,
            pages: None,
            pages_estimated: true,
            original_name: "d.epub".into(),
            file_size: 1,
            format: parallax_library::Format::Epub,
            kind: parallax_library::Kind::Book,
            direction: parallax_library::Direction::Ltr,
            has_cover: false,
            custom_cover: false,
            cover_upscaled: false,
            added_at: String::new(),
            modified_at: String::new(),
        };
        let items = [
            a(None, Some("Front matter."), None, true),
            a(
                Some("Book One"),
                Some("Fear is the mind-killer.\nI will face my fear."),
                Some("Litany"),
                true,
            ),
            a(Some("Book One"), None, None, true),
            a(Some("Book Two"), Some("Old one."), None, false),
        ];
        assert_eq!(
            markdown(&book, &items),
            "# Dune\n\nFrank Herbert\n\n> Front matter.\n\n## Book One\n\n> Fear is the mind-killer.\n> I will face my fear.\n\nLitany\n\n## Book Two\n\n> Old one.\n\n*Removed from the Kobo.*\n"
        );
        assert!(json(&book, &items).contains("\"Litany\""));
    }
}
