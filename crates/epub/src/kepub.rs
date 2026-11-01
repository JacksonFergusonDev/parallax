//! Turning a KEPUB back into a plain EPUB, and spotting DRM.
//!
//! KEPUBs are EPUBs with Kobo markup added by kepubify, calibre's KEPUB
//! output or the Kobo Touch Extended plugin. The markup is removed the way
//! calibre's `unkepubify` does (src/calibre/ebooks/oeb/polish/kepubify.py):
//!
//! - `span.koboSpan` sentence wrappers and the `div#book-columns` /
//!   `div#book-inner` page wrappers are unwrapped, keeping their contents;
//! - Kobo `<style>` blocks (`#kobostylehacks`, `#koboSpanStyle`,
//!   `#kepubify-extra-css`, `.kobostylehacks`), `kobo.js` scripts, links to
//!   `kobo.css` or Kobo Touch Extended's `kte-css/`, and `<!-- kobo-style -->`
//!   comments are removed;
//! - files only Kobo tools add (calibre's and kepubify's dummy title pages and
//!   covers, `kobo.js`, `kobo.css`, `kte-css/`, `plugininfo.kte`) are dropped
//!   from the zip and the manifest.
//!
//! Edits are splices into the original text, so everything else survives byte
//! for byte. A document that doesn't parse as XML is copied unchanged: the
//! markup is harmless to other readers, and a valid book matters more than a
//! clean one.

use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

use zip::CompressionMethod;
use zip::write::SimpleFileOptions;

use crate::{Error, Result, io_err, open_zip, opf, read_entry, resolve_href};

/// What [`strip_kobo_markup`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StripReport {
    /// Content documents that had Kobo markup removed.
    pub cleaned: usize,
    /// Content documents left unchanged because they aren't well-formed XML.
    pub unparsable: usize,
    /// Zip entries dropped (Kobo-only files).
    pub removed_files: usize,
}

impl StripReport {
    pub fn changed(&self) -> bool {
        self.cleaned > 0 || self.removed_files > 0
    }
}

/// Writes `src` to `dest` without Kobo markup.
pub fn strip_kobo_markup(src: &Path, dest: &Path) -> Result<StripReport> {
    let mut zip = open_zip(src)?;
    let container = read_entry(&mut zip, "META-INF/container.xml")?;
    let opf_path = opf::rootfile_path(&container)?;
    let opf_text = read_entry(&mut zip, &opf_path)?;
    let (new_opf, dropped_items) = strip_opf(&opf_text)?;
    let dropped: Vec<String> = dropped_items
        .iter()
        .map(|href| resolve_href(&opf_path, href))
        .collect();
    // Content documents by media type: names alone can lie (calibre's MOBI
    // conversions produce `.html_split_000`).
    let documents: Vec<String> = content_docs(&opf_text)
        .iter()
        .map(|href| resolve_href(&opf_path, href))
        .collect();

    let mut report = StripReport::default();
    let file = File::create(dest).map_err(|e| io_err(dest, e))?;
    let mut out = zip::ZipWriter::new(BufWriter::new(file));
    let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    out.start_file(
        "mimetype",
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    out.write_all(b"application/epub+zip")
        .map_err(|e| io_err(dest, e))?;

    for i in 0..zip.len() {
        let name = zip.by_index_raw(i)?.name()?.into_owned();
        if name == "mimetype" || name.ends_with('/') {
            continue;
        }
        if is_kobo_only(&name) || dropped.contains(&name) {
            report.removed_files += 1;
            continue;
        }
        if name == opf_path {
            out.start_file(name, deflated)?;
            out.write_all(new_opf.as_bytes())
                .map_err(|e| io_err(dest, e))?;
            continue;
        }
        if documents.contains(&name) || is_content_doc(&name) {
            let mut bytes = Vec::new();
            zip.by_index(i)?
                .read_to_end(&mut bytes)
                .map_err(|e| Error::Invalid(format!("{name}: {e}")))?;
            if let Ok(text) = std::str::from_utf8(&bytes) {
                match strip_document(text) {
                    Some(cleaned) if cleaned != text => {
                        report.cleaned += 1;
                        out.start_file(name, deflated)?;
                        out.write_all(cleaned.as_bytes())
                            .map_err(|e| io_err(dest, e))?;
                        continue;
                    }
                    Some(_) => {}
                    None => report.unparsable += 1,
                }
            }
        }
        out.raw_copy_file(zip.by_index_raw(i)?)?;
    }
    out.finish()?.flush().map_err(|e| io_err(dest, e))?;
    Ok(report)
}

/// True when the EPUB is DRM-protected (Adobe ADEPT, Kobo, Apple…): it has
/// `rights.xml`, or `encryption.xml` encrypts something other than fonts.
pub fn has_drm(path: &Path) -> Result<bool> {
    let mut zip = open_zip(path)?;
    if zip.by_name("META-INF/rights.xml").is_ok() {
        return Ok(true);
    }
    let Ok(encryption) = read_entry(&mut zip, "META-INF/encryption.xml") else {
        return Ok(false);
    };
    // Font obfuscation (IDPF and Adobe) is not DRM.
    const FONT_OBFUSCATION: [&str; 2] = [
        "http://www.idpf.org/2008/embedding",
        "http://ns.adobe.com/pdf/enc#RC",
    ];
    let doc = roxmltree::Document::parse(&encryption)
        .map_err(|e| Error::Invalid(format!("META-INF/encryption.xml: {e}")))?;
    Ok(doc
        .descendants()
        .filter(|n| n.has_tag_name("EncryptionMethod"))
        .filter_map(|n| n.attribute("Algorithm"))
        .any(|a| !FONT_OBFUSCATION.contains(&a)))
}

fn is_content_doc(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    [".xhtml", ".html", ".htm"]
        .iter()
        .any(|e| lower.ends_with(e))
}

/// Manifest hrefs of XHTML and HTML documents.
fn content_docs(opf: &str) -> Vec<String> {
    let Ok(doc) = roxmltree::Document::parse(opf) else {
        return Vec::new();
    };
    doc.descendants()
        .filter(|n| n.has_tag_name("item"))
        .filter(|n| {
            matches!(
                n.attribute("media-type"),
                Some("application/xhtml+xml" | "text/html")
            )
        })
        .filter_map(|n| n.attribute("href").map(str::to_owned))
        .collect()
}

/// Names of files calibre and kepubify generate for the Kobo.
const GENERATED: [&str; 3] = [
    "kepubify-titlepage-dummy",
    "kobo-title-page-generated-by-calibre",
    "kobo-cover-image-generated-by-calibre",
];

/// Files only Kobo tools add, wherever they sit: also used for manifest
/// hrefs, so a dropped file never leaves a dangling manifest item.
fn is_kobo_only(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    // Kobo Touch Extended's `plugininfo.kte` / `driverinfo.kte`.
    file.ends_with(".kte")
        || file == "kobo.js"
        || file == "kobo.css"
        || name.starts_with("kte-css/")
        || name.contains("/kte-css/")
        || GENERATED.iter().any(|g| file.contains(g))
}

const KOBO_STYLE_IDS: [&str; 3] = ["kobostylehacks", "koboSpanStyle", "kepubify-extra-css"];

/// Removes manifest items (and their spine entries) for Kobo-only files.
/// Returns the new OPF text and the removed hrefs.
fn strip_opf(text: &str) -> Result<(String, Vec<String>)> {
    let doc = roxmltree::Document::parse(text).map_err(|e| Error::Invalid(e.to_string()))?;
    let mut cuts = Vec::new();
    let mut ids = Vec::new();
    let mut hrefs = Vec::new();
    for item in doc.descendants().filter(|n| n.has_tag_name("item")) {
        let id = item.attribute("id").unwrap_or_default();
        let href = item.attribute("href").unwrap_or_default();
        if GENERATED.contains(&id) || is_kobo_only(href) {
            cuts.push(with_line(text, item.range()));
            ids.push(id.to_owned());
            hrefs.push(href.to_owned());
        }
    }
    for itemref in doc.descendants().filter(|n| n.has_tag_name("itemref")) {
        if itemref
            .attribute("idref")
            .is_some_and(|r| ids.iter().any(|id| id == r))
        {
            cuts.push(with_line(text, itemref.range()));
        }
    }
    Ok((splice(text, cuts), hrefs))
}

/// The cleaned document, or `None` if it isn't well-formed XML.
fn strip_document(text: &str) -> Option<String> {
    let source = text.trim_start_matches('\u{feff}');
    let offset = text.len() - source.len();
    let doc = roxmltree::Document::parse_with_options(
        source,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .ok()?;
    let mut cuts = Vec::new();
    for node in doc.descendants().filter(|n| n.is_comment()) {
        if node.text() == Some(" kobo-style ") {
            cuts.push(with_line(source, node.range()));
        }
    }
    for node in doc.descendants().filter(|n| n.is_element()) {
        let tag = node.tag_name().name();
        let class = node.attribute("class").unwrap_or_default();
        let id = node.attribute("id").unwrap_or_default();
        let range = node.range();
        let unwrap = (tag == "span" && class.split_whitespace().any(|c| c == "koboSpan"))
            || (tag == "div" && (id == "book-columns" || id == "book-inner"));
        let remove = (tag == "style"
            && (KOBO_STYLE_IDS.contains(&id)
                || class.split_whitespace().any(|c| c == "kobostylehacks")))
            || (tag == "script"
                && node
                    .attribute("src")
                    .is_some_and(|s| s.ends_with("kobo.js")))
            || (tag == "link" && node.attribute("href").is_some_and(is_kobo_only));
        if remove {
            cuts.push(with_line(source, range));
        } else if unwrap {
            let element = &source[range.clone()];
            if element.ends_with("/>") {
                cuts.push(range);
            } else {
                let open_end = range.start + start_tag_len(element)?;
                let close_start = range.start + element.rfind("</")?;
                cuts.push(range.start..open_end);
                cuts.push(close_start..range.end);
            }
        }
    }
    let cleaned = splice(source, cuts);
    Some(format!("{}{cleaned}", &text[..offset]))
}

/// Length of the start tag at the beginning of `element`, quotes respected.
fn start_tag_len(element: &str) -> Option<usize> {
    let mut quote = None;
    for (i, c) in element.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '>') => return Some(i + 1),
            _ => {}
        }
    }
    None
}

/// Widens a removal to its whole line when the element is alone on it, so no
/// blank lines are left behind.
fn with_line(text: &str, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
    let line_start = text[..range.start].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[range.end..]
        .find('\n')
        .map_or(text.len(), |i| range.end + i + 1);
    if text[line_start..range.start].trim().is_empty()
        && text[range.end..line_end].trim().is_empty()
    {
        line_start..line_end
    } else {
        range
    }
}

/// Deletes `cuts` from `text`. Cuts inside another cut are ignored.
fn splice(text: &str, mut cuts: Vec<std::ops::Range<usize>>) -> String {
    cuts.sort_by_key(|r| (r.start, std::cmp::Reverse(r.end)));
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for cut in cuts {
        if cut.start < pos {
            continue;
        }
        out.push_str(&text[pos..cut.start]);
        pos = cut.end;
    }
    out.push_str(&text[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unwraps_spans_and_page_divs() {
        let doc = r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title><style type="text/css" class="kobostylehacks">div#book-inner { margin-top: 0; }</style>
    <link rel="stylesheet" href="../kte-css/stylehacks.css"/>
  </head><body><div id="book-columns"><div id="book-inner"><p class="a"><span class="koboSpan" id="kobo.1.1">One. </span><span class="koboSpan" id="kobo.1.2">Two <b>bold</b>.</span><span class="koboSpan" id="kobo.1.3"/></p></div></div></body></html>"#;
        let cleaned = strip_document(doc).unwrap();
        assert_eq!(
            cleaned,
            r#"<?xml version="1.0"?><html xmlns="http://www.w3.org/1999/xhtml"><head><title>T</title>
  </head><body><p class="a">One. Two <b>bold</b>.</p></body></html>"#
        );
    }

    #[test]
    fn removes_calibre_kepub_styles() {
        let doc = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head>\n    <style type=\"text/css\" id=\"kobostylehacks\">div#book-inner { margin-top: 0; }</style>\n    <style type=\"text/css\" id=\"koboSpanStyle\">.koboSpan { x: y; }\n  </style>\n    <!-- kobo-style -->\n    <link rel=\"stylesheet\" type=\"text/css\" href=\"kobo.css\"/>\n  </head><body/></html>";
        assert_eq!(
            strip_document(doc).unwrap(),
            "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head>\n  </head><body/></html>"
        );
    }

    #[test]
    fn leaves_other_markup_alone() {
        let doc = r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><span class="koboSpanish">x</span><div id="main">y</div></body></html>"#;
        assert_eq!(strip_document(doc).as_deref(), Some(doc));
    }

    #[test]
    fn unparsable_documents_are_reported() {
        assert_eq!(strip_document("<p>caf&eacute;</p>"), None);
    }

    #[test]
    fn opf_drops_kobo_items() {
        let opf = r#"<package><manifest>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
    <item id="kepubify-titlepage-dummy" href="kepubify-titlepage-dummy.xhtml" media-type="application/xhtml+xml"/>
    <item id="kte" href="kte-css/stylehacks.css" media-type="text/css"/>
    <item id="id" href="plugininfo.kte" media-type="application/json"/>
  </manifest><spine>
    <itemref idref="kepubify-titlepage-dummy"/>
    <itemref idref="c1"/>
  </spine></package>"#;
        let (cleaned, hrefs) = strip_opf(opf).unwrap();
        assert_eq!(
            cleaned,
            r#"<package><manifest>
    <item id="c1" href="c1.xhtml" media-type="application/xhtml+xml"/>
  </manifest><spine>
    <itemref idref="c1"/>
  </spine></package>"#
        );
        assert_eq!(
            hrefs,
            [
                "kepubify-titlepage-dummy.xhtml",
                "kte-css/stylehacks.css",
                "plugininfo.kte"
            ]
        );
    }
}
