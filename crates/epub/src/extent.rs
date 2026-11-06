//! How long a book is.
//!
//! EPUBs have no page count of their own, since pages depend on the reader's
//! screen and font. Some publishers include a list of the print edition's
//! pages, which is used when present; otherwise the count is estimated from
//! the words in the book.

use roxmltree::Document;

use crate::{Epub, Error, Result, has_drm, open_zip, read_entry, resolve_href};

/// Words on a printed page. The books in a real library that list their print
/// pages ran from 200 to 400, averaging about 300.
const WORDS_PER_PAGE: u64 = 300;
/// A page list shorter than this is a stub, not the print edition's pages.
const MIN_PAGE_LIST: u64 = 10;
/// A document with an image and fewer words than this is a page of pictures,
/// as in comics, which word counts would otherwise miss.
const MAX_IMAGE_PAGE_WORDS: u64 = 50;

/// A book's length.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extent {
    pub words: u64,
    /// `None` when the book has neither text nor pictures.
    pub pages: Option<u64>,
    /// The pages were worked out from the words, not listed by the publisher.
    pub pages_estimated: bool,
}

impl Epub {
    /// Measures the book, reading every document in its spine.
    pub fn extent(&self) -> Result<Extent> {
        if has_drm(&self.path)? {
            return Err(Error::Invalid("encrypted content".to_owned()));
        }
        let parts = Parts::of(&self.doc());
        let mut zip = open_zip(&self.path)?;
        let mut read = |href: &str| read_entry(&mut zip, &resolve_href(&self.opf_path, href)).ok();

        let (mut words, mut image_pages) = (0, 0);
        for text in parts.spine.iter().filter_map(|href| read(href)) {
            let doc = scan(&text);
            words += doc.words;
            image_pages += u64::from(doc.has_image && doc.words < MAX_IMAGE_PAGE_WORDS);
        }
        let listed = parts
            .navigation
            .iter()
            .filter_map(|href| read(href))
            .map(|text| listed_pages(&text))
            .max()
            .filter(|n| *n >= MIN_PAGE_LIST);

        let estimate = (words + WORDS_PER_PAGE / 2) / WORDS_PER_PAGE + image_pages;
        Ok(Extent {
            words,
            pages: listed.or((words > 0 || image_pages > 0).then_some(estimate.max(1))),
            pages_estimated: listed.is_none(),
        })
    }
}

/// The hrefs that matter for measuring a book.
struct Parts {
    /// Content documents in reading order.
    spine: Vec<String>,
    /// The EPUB3 navigation document and EPUB2 NCX, which can list pages.
    navigation: Vec<String>,
}

impl Parts {
    fn of(doc: &Document) -> Self {
        let items: Vec<_> = doc
            .descendants()
            .filter(|n| n.has_tag_name("item"))
            .collect();
        let spine = doc
            .descendants()
            .filter(|n| n.has_tag_name("itemref"))
            .filter_map(|r| r.attribute("idref"))
            .filter_map(|id| items.iter().find(|i| i.attribute("id") == Some(id)))
            .filter(|i| {
                i.attribute("media-type")
                    .is_some_and(|t| t.contains("html"))
            })
            .filter_map(|i| i.attribute("href"))
            .map(str::to_owned)
            .collect();
        let navigation = items
            .iter()
            .filter(|i| {
                i.attribute("media-type") == Some("application/x-dtbncx+xml")
                    || i.attribute("properties")
                        .is_some_and(|p| p.split_whitespace().any(|p| p == "nav"))
            })
            .filter_map(|i| i.attribute("href"))
            .map(str::to_owned)
            .collect();
        Self { spine, navigation }
    }
}

/// Entries in a navigation document's page list. A text search, since these
/// files are too often not well-formed enough to parse.
fn listed_pages(text: &str) -> u64 {
    let ncx = text.matches("<pageTarget").count();
    let nav = text
        .split("<nav")
        .skip(1)
        .filter(|nav| {
            nav.split('>')
                .next()
                .is_some_and(|t| t.contains("page-list"))
        })
        .map(|nav| {
            let list = nav.split("</nav").next().unwrap_or(nav);
            list.matches("<li>").count() + list.matches("<li ").count()
        })
        .max()
        .unwrap_or(0);
    ncx.max(nav) as u64
}

#[derive(Debug, Default, PartialEq, Eq)]
struct Scan {
    words: u64,
    has_image: bool,
}

/// Counts the words in an (X)HTML document's body. A scanner rather than a
/// parser, so that broken markup still gets counted.
fn scan(html: &str) -> Scan {
    let mut rest = html.find("<body").map_or(html, |i| &html[i..]);
    let mut result = Scan::default();
    let mut cjk = 0;
    while let Some(start) = rest.find('<') {
        count_text(&rest[..start], &mut result.words, &mut cjk);
        let tag = &rest[start + 1..];
        let end = if tag.starts_with("!--") {
            after(tag, "-->")
        } else if starts_with_tag(tag, "script") {
            after(tag, "</script")
        } else if starts_with_tag(tag, "style") {
            after(tag, "</style")
        } else {
            result.has_image |= ["img", "image", "svg"]
                .iter()
                .any(|name| starts_with_tag(tag, name));
            after(tag, ">")
        };
        rest = &tag[end..];
    }
    count_text(rest, &mut result.words, &mut cjk);
    // Chinese and Japanese don't space their words, which average two characters.
    result.words += cjk / 2;
    result
}

/// The offset just past `needle`, or the end of the text.
fn after(text: &str, needle: &str) -> usize {
    text.find(needle).map_or(text.len(), |i| i + needle.len())
}

fn starts_with_tag(tag: &str, name: &str) -> bool {
    tag.get(..name.len())
        .is_some_and(|t| t.eq_ignore_ascii_case(name))
        && !tag[name.len()..].starts_with(|c: char| c.is_alphanumeric())
}

fn count_text(text: &str, words: &mut u64, cjk: &mut u64) {
    for token in text.split_whitespace() {
        // A lone "&amp;" or "&#8212;" isn't a word.
        if token.starts_with('&') && token.ends_with(';') {
            continue;
        }
        let unspaced = token.chars().filter(|c| is_cjk(*c)).count() as u64;
        *cjk += unspaced;
        *words += u64::from(token.chars().any(|c| c.is_alphanumeric() && !is_cjk(c)));
    }
}

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(html: &str) -> u64 {
        scan(html).words
    }

    #[test]
    fn counts_only_the_body_text() {
        let html = "<html><head><title>Not this</title></head>
            <body><!-- nor this --><style>p { color: red }</style>
            <script>nor(this)</script><p>Three <b>small</b> words</p></body></html>";
        assert_eq!(words(html), 3);
    }

    #[test]
    fn ignores_punctuation_and_entities() {
        assert_eq!(words("<body>Salt &amp; pepper — don’t stop</body>"), 4);
    }

    #[test]
    fn paragraphs_without_spaces_are_separate_words() {
        assert_eq!(words("<body><p>one</p><p>two</p></body>"), 2);
    }

    #[test]
    fn survives_broken_markup() {
        assert_eq!(words("no body at all <p>and an unclosed"), 7);
    }

    #[test]
    fn counts_unspaced_scripts_by_character_pairs() {
        assert_eq!(words("<body>吾輩は猫である</body>"), 3);
    }

    #[test]
    fn notices_images() {
        assert!(scan("<body><IMG src='a.jpg'/></body>").has_image);
        assert!(scan("<body><svg><image href='a'/></svg></body>").has_image);
        assert!(!scan("<body><imagery>text</imagery></body>").has_image);
    }

    #[test]
    fn counts_ncx_page_targets() {
        let ncx = "<ncx><pageList><pageTarget/><pageTarget/></pageList></ncx>";
        assert_eq!(listed_pages(ncx), 2);
    }

    #[test]
    fn counts_only_the_page_list_nav() {
        let nav = r#"<nav epub:type="toc"><ol><li>Chapter</li></ol></nav>
            <nav epub:type="page-list" hidden=""><ol><li><a>1</a></li><li id="p2"><a>2</a></li></ol></nav>"#;
        assert_eq!(listed_pages(nav), 2);
    }
}
