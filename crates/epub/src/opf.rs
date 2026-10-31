//! OPF package document handling.

use std::collections::HashSet;
use std::ops::Range;

use roxmltree::{Document, Node};
use serde::{Deserialize, Serialize};

use crate::{Error, Metadata, MetadataEdit, Result};

const DC_NS: &str = "http://purl.org/dc/elements/1.1/";
const OPF_NS: &str = "http://www.idpf.org/2007/opf";
const CONTAINER_NS: &str = "urn:oasis:names:tc:opendocument:xmlns:container";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Identifier {
    /// e.g. "isbn", "uuid", "calibre"; lowercased.
    pub scheme: Option<String>,
    pub value: String,
}

pub fn rootfile_path(container_xml: &str) -> Result<String> {
    let doc = Document::parse(container_xml)
        .map_err(|e| Error::Invalid(format!("container.xml: {e}")))?;
    doc.descendants()
        .find(|n| n.has_tag_name((CONTAINER_NS, "rootfile")) || n.has_tag_name("rootfile"))
        .and_then(|n| n.attribute("full-path"))
        .map(str::to_owned)
        .ok_or_else(|| Error::Invalid("container.xml has no rootfile".into()))
}

fn metadata_node<'a>(doc: &'a Document) -> Option<Node<'a, 'a>> {
    doc.root_element()
        .children()
        .find(|n| n.has_tag_name((OPF_NS, "metadata")) || n.tag_name().name() == "metadata")
}

fn is_dc(n: &Node, name: &str) -> bool {
    n.has_tag_name((DC_NS, name))
}

fn is_meta(n: &Node) -> bool {
    n.is_element() && n.tag_name().name() == "meta"
}

fn text(n: &Node) -> Option<String> {
    let t: String = n
        .descendants()
        .filter(|d| d.is_text())
        .filter_map(|d| d.text())
        .collect();
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// EPUB3 `<meta refines="#id" property="...">` values for `id`.
fn refinement(meta: &Node, id: Option<&str>, property: &str) -> Option<String> {
    let id = id?;
    meta.children()
        .filter(is_meta)
        .find(|m| {
            m.attribute("refines") == Some(&format!("#{id}"))
                && m.attribute("property") == Some(property)
        })
        .and_then(|m| text(&m))
}

pub fn read_metadata(doc: &Document) -> Metadata {
    let Some(meta) = metadata_node(doc) else {
        return Metadata::default();
    };
    let children: Vec<Node> = meta.children().filter(Node::is_element).collect();
    let named_meta = |name: &str| {
        children
            .iter()
            .find(|n| is_meta(n) && n.attribute("name") == Some(name))
            .and_then(|n| n.attribute("content"))
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };

    let authors = children
        .iter()
        .filter(|n| is_dc(n, "creator"))
        .filter(|n| {
            let role = n
                .attribute((OPF_NS, "role"))
                .map(str::to_owned)
                .or_else(|| refinement(&meta, n.attribute("id"), "role"));
            role.is_none_or(|r| r.eq_ignore_ascii_case("aut"))
        })
        .filter_map(text)
        .collect();

    // calibre-style series first; EPUB3 collections as a fallback.
    let (series, series_index) = match named_meta("calibre:series") {
        Some(series) => (Some(series), named_meta("calibre:series_index")),
        None => children
            .iter()
            .find(|n| is_meta(n) && n.attribute("property") == Some("belongs-to-collection"))
            .map_or((None, None), |n| {
                (
                    text(n),
                    refinement(&meta, n.attribute("id"), "group-position"),
                )
            }),
    };

    let identifiers = children
        .iter()
        .filter(|n| is_dc(n, "identifier"))
        .filter_map(|n| {
            let raw = text(n)?;
            let attr_scheme = n.attribute((OPF_NS, "scheme")).map(str::to_lowercase);
            let (prefix_scheme, value) =
                match raw.strip_prefix("urn:").and_then(|r| r.split_once(':')) {
                    Some((scheme, value)) => (Some(scheme.to_lowercase()), value.to_owned()),
                    None => (None, raw),
                };
            Some(Identifier {
                scheme: attr_scheme.or(prefix_scheme),
                value,
            })
        })
        .collect();

    let first_dc = |name: &str| children.iter().find(|n| is_dc(n, name)).and_then(text);
    Metadata {
        title: first_dc("title"),
        authors,
        series,
        series_index,
        tags: children
            .iter()
            .filter(|n| is_dc(n, "subject"))
            .filter_map(text)
            .collect(),
        language: first_dc("language"),
        publisher: first_dc("publisher"),
        description: first_dc("description"),
        identifiers,
    }
}

/// `(href, media-type)` of the cover image: EPUB3 `cover-image` property,
/// then the EPUB2 `<meta name="cover">` pointer, then an image item whose id or
/// href mentions "cover".
pub fn cover_item(doc: &Document) -> Option<(String, String)> {
    let manifest = doc
        .root_element()
        .children()
        .find(|n| n.tag_name().name() == "manifest")?;
    let items: Vec<Node> = manifest
        .children()
        .filter(|n| n.tag_name().name() == "item")
        .collect();
    let is_image = |n: &&Node| {
        n.attribute("media-type")
            .is_some_and(|m| m.starts_with("image/"))
    };
    let pick = |n: &Node| {
        Some((
            n.attribute("href")?.to_owned(),
            n.attribute("media-type")?.to_owned(),
        ))
    };

    if let Some(item) = items.iter().find(|n| {
        n.attribute("properties")
            .is_some_and(|p| p.split_whitespace().any(|p| p == "cover-image"))
    }) {
        return pick(item);
    }
    let cover_id = metadata_node(doc).and_then(|m| {
        m.children()
            .filter(is_meta)
            .find(|n| n.attribute("name") == Some("cover"))
            .and_then(|n| n.attribute("content"))
    });
    if let Some(item) = cover_id.and_then(|id| {
        items
            .iter()
            .filter(is_image)
            .find(|n| n.attribute("id") == Some(id))
    }) {
        return pick(item);
    }
    items
        .iter()
        .filter(is_image)
        .find(|n| {
            let id = n.attribute("id").unwrap_or_default().to_lowercase();
            let href = n.attribute("href").unwrap_or_default().to_lowercase();
            id.contains("cover") || href.contains("cover")
        })
        .and_then(pick)
}

/// Returns `opf` with the managed metadata elements replaced by `edit`.
pub fn apply_edit(opf: &str, edit: &MetadataEdit) -> Result<String> {
    let doc = Document::parse(opf).map_err(|e| Error::Invalid(format!("OPF: {e}")))?;
    let meta = metadata_node(&doc).ok_or_else(|| Error::Invalid("OPF has no <metadata>".into()))?;
    let epub3 = doc
        .root_element()
        .attribute("version")
        .is_some_and(|v| v.starts_with('3'));

    // Elements to drop: managed ones, plus EPUB3 refinements pointing at them.
    let managed: Vec<Node> = meta
        .children()
        .filter(|n| {
            is_dc(n, "title")
                || is_dc(n, "creator") && is_author(&meta, n)
                || is_dc(n, "subject")
                || is_meta(n)
                    && (matches!(
                        n.attribute("name"),
                        Some("calibre:series" | "calibre:series_index")
                    ) || n.attribute("property") == Some("belongs-to-collection"))
        })
        .collect();
    let dropped_ids: HashSet<String> = managed
        .iter()
        .filter_map(|n| n.attribute("id"))
        .map(|id| format!("#{id}"))
        .collect();
    let mut remove: Vec<Range<usize>> = meta
        .children()
        .filter(|n| {
            managed.contains(n)
                || is_meta(n)
                    && n.attribute("refines")
                        .is_some_and(|r| dropped_ids.contains(r))
        })
        .map(|n| with_leading_whitespace(opf, n.range()))
        .collect();
    remove.sort_by_key(|r| r.start);

    let dc = prefix_for(&meta, DC_NS, "dc");
    let opf_attr = prefix_for(&meta, OPF_NS, "opf");
    let indent = child_indent(opf, &meta);
    let block = render_managed(edit, epub3, &dc, &opf_attr, &indent);

    // Insert just before the closing </metadata> tag.
    let meta_range = meta.range();
    let close = opf[..meta_range.end]
        .rfind("</")
        .ok_or_else(|| Error::Invalid("unterminated <metadata>".into()))?;
    let insert_at = opf[..close].trim_end().len();

    let mut out = String::with_capacity(opf.len() + block.len());
    let mut pos = 0;
    for r in remove.iter().filter(|r| r.start < insert_at) {
        out.push_str(&opf[pos..r.start]);
        pos = r.end.max(pos);
    }
    out.push_str(&opf[pos..insert_at]);
    out.push_str(&block);
    out.push_str(&opf[insert_at..]);
    Ok(out)
}

fn is_author(meta: &Node, creator: &Node) -> bool {
    let role = creator
        .attribute((OPF_NS, "role"))
        .map(str::to_owned)
        .or_else(|| refinement(meta, creator.attribute("id"), "role"));
    role.is_none_or(|r| r.eq_ignore_ascii_case("aut"))
}

/// Widens `range` to swallow the indentation/newline before it so removals
/// don't leave blank lines.
fn with_leading_whitespace(src: &str, range: Range<usize>) -> Range<usize> {
    let before = src[..range.start].trim_end_matches([' ', '\t']);
    let start = match before.strip_suffix('\n') {
        Some(rest) => rest.strip_suffix('\r').unwrap_or(rest).len(),
        None => range.start,
    };
    start..range.end
}

/// The prefix bound to `ns` at `node`, or an inline declaration fallback.
struct Prefix {
    name: String,
    declare: Option<String>,
}

fn prefix_for(node: &Node, ns: &str, fallback: &str) -> Prefix {
    match node.lookup_prefix(ns) {
        Some(p) if !p.is_empty() => Prefix {
            name: p.to_owned(),
            declare: None,
        },
        _ => Prefix {
            name: fallback.to_owned(),
            declare: Some(format!(" xmlns:{fallback}=\"{ns}\"")),
        },
    }
}

fn child_indent(src: &str, meta: &Node) -> String {
    meta.children()
        .find(Node::is_element)
        .map(|first| {
            let before = &src[..first.range().start];
            let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
            if line.trim().is_empty() {
                line.to_owned()
            } else {
                "    ".to_owned()
            }
        })
        .unwrap_or_else(|| "    ".to_owned())
}

fn render_managed(
    edit: &MetadataEdit,
    epub3: bool,
    dc: &Prefix,
    opf: &Prefix,
    indent: &str,
) -> String {
    let d = &dc.name;
    let dd = dc.declare.as_deref().unwrap_or("");
    let mut lines = vec![format!("<{d}:title{dd}>{}</{d}:title>", esc(&edit.title))];
    for (i, author) in edit.authors.iter().enumerate() {
        if epub3 {
            let id = format!("erc-creator-{}", i + 1);
            lines.push(format!(
                "<{d}:creator{dd} id=\"{id}\">{}</{d}:creator>",
                esc(author)
            ));
            lines.push(format!(
                r##"<meta refines="#{id}" property="role" scheme="marc:relators">aut</meta>"##
            ));
        } else {
            let o = &opf.name;
            let od = opf.declare.as_deref().unwrap_or("");
            lines.push(format!(
                "<{d}:creator{dd}{od} {o}:role=\"aut\">{}</{d}:creator>",
                esc(author)
            ));
        }
    }
    for tag in &edit.tags {
        lines.push(format!("<{d}:subject{dd}>{}</{d}:subject>", esc(tag)));
    }
    if let Some(series) = edit.series.as_deref().filter(|s| !s.trim().is_empty()) {
        // calibre's tags are what Kobo tooling and calibre itself read.
        lines.push(format!(
            r#"<meta name="calibre:series" content="{}"/>"#,
            esc(series)
        ));
        if let Some(index) = &edit.series_index {
            lines.push(format!(
                r#"<meta name="calibre:series_index" content="{}"/>"#,
                esc(index)
            ));
        }
        if epub3 {
            lines.push(format!(
                r#"<meta property="belongs-to-collection" id="erc-series">{}</meta>"#,
                esc(series)
            ));
            lines.push(
                r##"<meta refines="#erc-series" property="collection-type">series</meta>"##
                    .to_owned(),
            );
            if let Some(index) = &edit.series_index {
                lines.push(format!(
                    r##"<meta refines="#erc-series" property="group-position">{}</meta>"##,
                    esc(index)
                ));
            }
        }
    }
    lines.iter().map(|l| format!("\n{indent}{l}")).collect()
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
