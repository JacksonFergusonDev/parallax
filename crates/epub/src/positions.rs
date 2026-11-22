//! Places in a book, moved between a Kobo and the desktop reader.
//!
//! A Kobo keeps its place in a KEPUB as a chapter and a `koboSpan` id
//! (`OEBPS/ch2.xhtml#kobo.3.1`, Nickel's `ChapterIDBookmarked`). The desktop
//! reader keeps an EPUB CFI into the library's plain EPUB, which has no spans.
//! Span ids from a fresh conversion can differ from the copy on the Kobo, so
//! the KEPUB read is always the Kobo's own, and the two are matched by the
//! text at the place, in the same chapter.
//!
//! CFIs follow the EPUB CFI spec as foliate-js writes them
//! (`vendor/foliate-js/epubcfi.js`): the spine step comes from the package
//! document, then even steps for element children from `<html>` down. They
//! point at the element holding the text, so the reader opens at the start of
//! its paragraph.

use std::path::Path;

use roxmltree::{Document, Node, ParsingOptions};

use crate::{Error, Result, open_zip, opf, read_entry, resolve_href};

/// Characters of text compared to find a place. Long enough to be unique in a
/// chapter, short enough to survive small differences in markup.
const MATCH_CHARS: usize = 48;
/// The shortest text worth matching on.
const MIN_MATCH_CHARS: usize = 12;

/// The chapter path and span id of a Kobo place, `None` for anything else
/// (an Adobe position in a plain EPUB, or no fragment).
pub fn split_kobo_place(chapter: &str) -> Option<(&str, &str)> {
    let (path, span) = chapter.split_once('#')?;
    (span.starts_with("kobo.") && !path.is_empty()).then_some((path, span))
}

/// The text from a span onwards in a KEPUB chapter, whitespace collapsed.
pub fn kobo_span_text(kepub: &Path, chapter: &str, span: &str) -> Result<Option<String>> {
    let Some(xhtml) = read_chapter(kepub, chapter)? else {
        return Ok(None);
    };
    let doc = parse(&xhtml)?;
    let Some(start) = doc
        .descendants()
        .find(|n| n.is_element() && n.attribute("id") == Some(span))
    else {
        return Ok(None);
    };
    let text = Text::of(&doc);
    // The first text inside or after the span.
    let at = text
        .runs
        .iter()
        .find(|run| {
            run.node.ancestors().any(|a| a == start)
                || run.node.range().start >= start.range().start
        })
        .map(|run| run.start);
    Ok(at.map(|at| text.plain.chars().skip(at).take(MATCH_CHARS * 2).collect()))
}

/// A CFI for the place in the EPUB's `href` chapter whose text starts with
/// `quote`, or failing that the chapter's start. `None` when the chapter
/// isn't in the book's spine.
pub fn cfi_for_quote(epub: &Path, href: &str, quote: &str) -> Result<Option<String>> {
    let mut zip = open_zip(epub)?;
    let container = read_entry(&mut zip, "META-INF/container.xml")?;
    let opf_path = opf::rootfile_path(&container)?;
    let package = read_entry(&mut zip, &opf_path)?;
    let package = parse(&package)?;
    let Some((base, path)) = spine_step(&package, &opf_path, href) else {
        return Ok(None);
    };
    let Ok(xhtml) = read_entry(&mut zip, &path) else {
        return Ok(None);
    };
    let doc = parse(&xhtml)?;
    let target =
        find(&Text::of(&doc), quote).or_else(|| doc.descendants().find(|n| n.has_tag_name("body")));
    Ok(Some(match target {
        Some(node) => format!("epubcfi({base}!{})", element_steps(node)),
        None => format!("epubcfi({base}!/4)"),
    }))
}

/// The Kobo place (`chapter#kobo.x.y`) in a KEPUB chapter whose text starts
/// with `quote`: the span holding the start of the match.
pub fn kobo_place_for_quote(kepub: &Path, chapter: &str, quote: &str) -> Result<Option<String>> {
    let Some(path) = chapter_path(kepub, chapter)? else {
        return Ok(None);
    };
    let Some(xhtml) = read_chapter(kepub, &path)? else {
        return Ok(None);
    };
    let doc = parse(&xhtml)?;
    let span = find(&Text::of(&doc), quote).and_then(|node| {
        node.ancestors()
            .find(|a| a.is_element() && a.attribute("id").is_some_and(|id| id.starts_with("kobo.")))
    });
    Ok(span
        .and_then(|s| s.attribute("id"))
        .map(|id| format!("{path}#{id}")))
}

/// A chapter's path as stored in the KEPUB: `href` itself, or the entry
/// with the same name when the two were written with different folders.
fn chapter_path(book: &Path, href: &str) -> Result<Option<String>> {
    let zip = open_zip(book)?;
    if zip.index_for_name(href).is_some() {
        return Ok(Some(href.to_owned()));
    }
    let name = href.rsplit('/').next().unwrap_or(href);
    let matches: Vec<String> = (0..zip.len())
        .filter_map(|i| zip.name_for_index(i)?.ok().map(|n| n.to_string()))
        .filter(|n| n.rsplit('/').next() == Some(name))
        .collect();
    Ok((matches.len() == 1).then(|| matches[0].clone()))
}

fn read_chapter(book: &Path, chapter: &str) -> Result<Option<String>> {
    let mut zip = open_zip(book)?;
    Ok(read_entry(&mut zip, chapter).ok())
}

fn parse(text: &str) -> Result<Document<'_>> {
    Document::parse_with_options(
        text,
        ParsingOptions {
            allow_dtd: true,
            ..ParsingOptions::default()
        },
    )
    .map_err(|e| Error::Invalid(e.to_string()))
}

/// The CFI step to `href`'s spine item, as foliate builds it from the
/// package document, and the chapter's path in the zip.
fn spine_step(package: &Document, opf_path: &str, href: &str) -> Option<(String, String)> {
    let root = package.root_element();
    let items: Vec<Node> = root
        .descendants()
        .filter(|n| n.has_tag_name("item"))
        .collect();
    let spine = root.children().find(|n| n.has_tag_name("spine"))?;
    let wanted = href.rsplit('/').next().unwrap_or(href);
    let mut by_name = None;
    for itemref in spine.children().filter(Node::is_element) {
        let idref = itemref.attribute("idref")?;
        let Some(path) = items
            .iter()
            .find(|i| i.attribute("id") == Some(idref))
            .and_then(|i| i.attribute("href"))
            .map(|h| resolve_href(opf_path, h))
        else {
            continue;
        };
        let step = format!("{}{}", element_step(spine), element_step_with_id(itemref));
        if path == href {
            return Some((step, path));
        }
        if path.rsplit('/').next() == Some(wanted) {
            by_name = Some((step, path));
        }
    }
    by_name
}

/// Steps from `<html>` down to `node`: `/4/2/6`.
fn element_steps(node: Node) -> String {
    let mut steps: Vec<String> = node
        .ancestors()
        .filter(Node::is_element)
        .take_while(|n| n.parent().is_some_and(|p| p.is_element()))
        .map(element_step)
        .collect();
    steps.reverse();
    steps.concat()
}

/// `/n` for an element: its place among its parent's element children,
/// doubled, counting from 2.
fn element_step(node: Node) -> String {
    let k = node
        .prev_siblings()
        .skip(1)
        .filter(|n| n.is_element() && n.attribute("cfi-inert").is_none())
        .count();
    format!("/{}", 2 * (k + 1))
}

fn element_step_with_id(node: Node) -> String {
    match node.attribute("id") {
        Some(id) if !id.is_empty() => format!("{}[{}]", element_step(node), escape(id)),
        _ => element_step(node),
    }
}

fn escape(id: &str) -> String {
    id.chars()
        .flat_map(|c| {
            let special = "^[](),;=".contains(c);
            special.then_some('^').into_iter().chain([c])
        })
        .collect()
}

/// A chapter's text, whitespace collapsed, and where each text node starts.
struct Text<'a, 'input> {
    plain: String,
    runs: Vec<Run<'a, 'input>>,
}

struct Run<'a, 'input> {
    /// Character offset in `plain`.
    start: usize,
    node: Node<'a, 'input>,
}

impl<'a, 'input> Text<'a, 'input> {
    fn of(doc: &'a Document<'input>) -> Self {
        let mut plain = String::new();
        let mut count = 0;
        let mut runs = Vec::new();
        let mut space = true;
        let body = doc
            .descendants()
            .find(|n| n.has_tag_name("body"))
            .unwrap_or_else(|| doc.root_element());
        for node in body.descendants().filter(Node::is_text) {
            if node
                .ancestors()
                .any(|a| a.has_tag_name("script") || a.has_tag_name("style"))
            {
                continue;
            }
            runs.push(Run { start: count, node });
            for c in node.text().unwrap_or_default().chars() {
                if c.is_whitespace() {
                    if !space {
                        plain.push(' ');
                        count += 1;
                        space = true;
                    }
                } else {
                    plain.push(c);
                    count += 1;
                    space = false;
                }
            }
        }
        Self { plain, runs }
    }
}

/// The element holding the start of `quote`'s first words in `text`.
fn find<'a, 'input>(text: &Text<'a, 'input>, quote: &str) -> Option<Node<'a, 'input>> {
    let quote: String = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = quote.chars().collect();
    let mut len = chars.len().min(MATCH_CHARS);
    while len >= MIN_MATCH_CHARS {
        let needle: String = chars[..len].iter().collect();
        if let Some(byte) = text.plain.find(&needle) {
            let at = text.plain[..byte].chars().count();
            let run = text.runs.iter().rev().find(|r| r.start <= at)?;
            return run.node.parent_element();
        }
        len /= 2;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kobo_places_are_split() {
        assert_eq!(
            split_kobo_place("OEBPS/ch2.xhtml#kobo.3.1"),
            Some(("OEBPS/ch2.xhtml", "kobo.3.1"))
        );
        assert_eq!(split_kobo_place("OEBPS/ch2.xhtml"), None);
        assert_eq!(split_kobo_place("OEBPS/ch2.xhtml#point(/1/4/2:0)"), None);
    }

    #[test]
    fn steps_count_element_children_only() {
        let doc = Document::parse(
            "<html><head/><body>text<p>a</p><!-- c --><div><p>b</p><p id=\"x\">c</p></div></body></html>",
        )
        .unwrap();
        let target = doc
            .descendants()
            .find(|n| n.attribute("id") == Some("x"))
            .unwrap();
        assert_eq!(element_steps(target), "/4/4/4");
        let text = Text::of(&doc);
        assert_eq!(text.plain, "textabc");
        assert_eq!(find(&text, "abc and more").map(|_| ()), None);
        assert_eq!(escape("a[b]"), "a^[b^]");
    }
}
