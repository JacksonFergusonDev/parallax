//! Structural checks on an EPUB, a subset of the Quality Check calibre
//! plugin's (kiwidude68) that needs nothing but the file: manifest items
//! with no file, files the manifest doesn't list, links and images pointing
//! at files that aren't there, operating-system clutter, embedded fonts and DRM.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use crate::{Result, has_drm, open_zip, opf, read_entry, resolve_href};

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Inspection {
    /// Manifest items whose file isn't in the zip.
    pub missing_files: Vec<String>,
    /// Zip entries the manifest doesn't list (besides the container, the
    /// package document and clutter).
    pub unmanifested: Vec<String>,
    /// (document, target) for links and images pointing at missing files.
    pub broken_links: Vec<(String, String)>,
    /// `.DS_Store`, `__MACOSX/…`, `Thumbs.db`, `desktop.ini`.
    pub clutter: Vec<String>,
    pub fonts: Vec<String>,
    pub drm: bool,
}

impl Inspection {
    /// Nothing worth showing.
    pub fn is_clean(&self) -> bool {
        self.missing_files.is_empty()
            && self.unmanifested.is_empty()
            && self.broken_links.is_empty()
            && self.clutter.is_empty()
            && !self.drm
    }
}

/// Files operating systems leave in folders that end up zipped into EPUBs.
pub fn is_clutter(name: &str) -> bool {
    let file = name.rsplit('/').next().unwrap_or(name);
    name.starts_with("__MACOSX/")
        || file == ".DS_Store"
        || file == "Thumbs.db"
        || file.eq_ignore_ascii_case("desktop.ini")
        || file.starts_with("._")
}

pub fn inspect(path: &Path) -> Result<Inspection> {
    let mut zip = open_zip(path)?;
    let entries: Vec<String> = (0..zip.len())
        .filter_map(|i| Some(zip.by_index_raw(i).ok()?.name().ok()?.into_owned()))
        .filter(|n| !n.ends_with('/'))
        .collect();
    let present: HashSet<&str> = entries.iter().map(String::as_str).collect();
    let container = read_entry(&mut zip, "META-INF/container.xml")?;
    let opf_path = opf::rootfile_path(&container)?;
    let opf_text = read_entry(&mut zip, &opf_path)?;
    let doc = roxmltree::Document::parse(&opf_text)
        .map_err(|e| crate::Error::Invalid(format!("{opf_path}: {e}")))?;

    let mut out = Inspection {
        drm: has_drm(path).unwrap_or(false),
        ..Inspection::default()
    };
    let mut manifested: HashSet<String> = HashSet::new();
    let mut documents = Vec::new();
    for item in doc.descendants().filter(|n| n.tag_name().name() == "item") {
        let Some(href) = item.attribute("href") else {
            continue;
        };
        if href.contains("://") {
            continue;
        }
        let name = resolve_href(&opf_path, href);
        let media = item.attribute("media-type").unwrap_or_default();
        if !present.contains(name.as_str()) {
            out.missing_files.push(name.clone());
        }
        if media.contains("font")
            || [".ttf", ".otf", ".woff", ".woff2"]
                .iter()
                .any(|e| name.to_lowercase().ends_with(e))
        {
            out.fonts.push(name.clone());
        }
        if matches!(media, "application/xhtml+xml" | "text/html") {
            documents.push(name.clone());
        }
        manifested.insert(name);
    }
    for name in &entries {
        if is_clutter(name) {
            out.clutter.push(name.clone());
        } else if name != "mimetype"
            && !name.starts_with("META-INF/")
            && *name != opf_path
            && !manifested.contains(name)
        {
            out.unmanifested.push(name.clone());
        }
    }

    for name in documents.iter().filter(|n| present.contains(n.as_str())) {
        let Ok(text) = read_entry(&mut zip, name) else {
            continue;
        };
        let Ok(html) = roxmltree::Document::parse_with_options(
            &text,
            roxmltree::ParsingOptions {
                allow_dtd: true,
                ..Default::default()
            },
        ) else {
            // Not well-formed; the link check is skipped rather than guessed.
            continue;
        };
        for node in html.descendants().filter(|n| n.is_element()) {
            let target = match node.tag_name().name() {
                // Only what a reader would notice: links, images and media.
                // Missing stylesheets and page templates (common leftovers of
                // old conversions) can't be fixed here and readers ignore them.
                "a" | "area" => node.attribute("href"),
                "img" | "source" | "audio" | "video" => node.attribute("src"),
                "image" => node
                    .attribute(("http://www.w3.org/1999/xlink", "href"))
                    .or_else(|| node.attribute("href")),
                _ => None,
            };
            let Some(target) = target.map(str::trim) else {
                continue;
            };
            if target.is_empty() || target.starts_with('#') || target.contains(':') {
                continue;
            }
            let resolved = resolve_href(name, target);
            if !present.contains(resolved.as_str()) {
                out.broken_links.push((name.clone(), target.to_owned()));
            }
        }
    }
    Ok(out)
}
