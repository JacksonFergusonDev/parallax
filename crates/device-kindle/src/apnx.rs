//! APNX page-number files, ported from calibre (`devices/kindle/apnx.py` and
//! `apnx_page_generator/generators/`). The pages are an estimate, not a print
//! edition's. When the book's text can be read, pages follow calibre's
//! "accurate" method, which lays the text out in lines of paragraphs and
//! starts a page every 32 lines. Otherwise there's a page every 2,300
//! characters, calibre's "fast" method.

use crate::mobi::MobiHeader;

/// Characters of uncompressed text per page, calibre's fast method.
pub const CHARS_PER_PAGE: u32 = 2300;

/// Page start positions: one every [`CHARS_PER_PAGE`] characters.
pub fn fast_pages(text_length: u32) -> Vec<u32> {
    (0..text_length).step_by(CHARS_PER_PAGE as usize).collect()
}

/// Characters in a line of a paragraph, calibre's accurate method.
const CHARS_PER_LINE: u32 = 70;
/// Lines on a page, calibre's accurate method.
const LINES_PER_PAGE: usize = 32;

/// Page start positions from the book's decompressed text, as calibre's
/// `accurate_page_generator.py`. A line starts just before each `<p` and after every
/// 70 characters of paragraph text outside tags; every 32nd line starts a
/// page. Like calibre, the character count runs on across paragraphs.
pub fn accurate_pages(text: &[u8]) -> Vec<u32> {
    let mut lines = Vec::new();
    let (mut in_tag, mut in_p, mut check_p, mut closing) = (false, false, false, false);
    let mut chars = 0;
    for (pos, &c) in text.iter().enumerate() {
        let c = c.to_ascii_lowercase();
        if check_p {
            if c == b'/' {
                closing = true;
                continue;
            }
            if c == b'p' {
                if closing {
                    in_p = false;
                } else {
                    in_p = true;
                    lines.push(pos.saturating_sub(2));
                }
            }
            check_p = false;
            closing = false;
            continue;
        }
        match c {
            b'<' => {
                in_tag = true;
                check_p = true;
                continue;
            }
            b'>' => {
                in_tag = false;
                check_p = false;
                continue;
            }
            _ => {}
        }
        if in_p && !in_tag {
            chars += 1;
            if chars == CHARS_PER_LINE {
                lines.push(pos);
                chars = 0;
            }
        }
    }
    lines
        .into_iter()
        .step_by(LINES_PER_PAGE)
        .filter_map(|p| u32::try_from(p).ok())
        .collect()
}

/// Page starts for a whole book file: the accurate method when its text can
/// be read and has paragraphs, otherwise the fast one.
pub fn pages(book: &[u8], header: &MobiHeader) -> Vec<u32> {
    crate::text::extract(book)
        .map(|text| accurate_pages(&text))
        .filter(|pages| !pages.is_empty())
        .unwrap_or_else(|| fast_pages(header.text_length))
}

/// The APNX file for a book, or `None` when it has no text to number.
/// `guid` is the 8-character content GUID; calibre uses a random one.
pub fn build(header: &MobiHeader, pages: &[u32], guid: &str) -> Option<Vec<u8>> {
    if pages.is_empty() {
        return None;
    }
    let pages_len = u16::try_from(pages.len()).ok()?;
    let asin = json_escape(header.asin.as_deref().unwrap_or_default());
    let cdetype = json_escape(header.cdetype.as_deref().unwrap_or("EBOK"));
    let acr = json_escape(&header.pdb_name);
    let content_header = if header.mobi_version == 8 {
        format!(
            r#"{{"contentGuid":"{guid}","asin":"{asin}","cdeType":"{cdetype}","format":"MOBI_8","fileRevisionId":"1","acr":"{acr}"}}"#
        )
    } else {
        format!(
            r#"{{"contentGuid":"{guid}","asin":"{asin}","cdeType":"{cdetype}","fileRevisionId":"1"}}"#
        )
    };
    // One group of arabic numerals starting at page 1, as calibre's `Pages`.
    let page_header = format!(r#"{{"asin":"{asin}","pageMap":"(1,a,1)"}}"#);
    let content_len = u32::try_from(content_header.len()).ok()?;
    let page_header_len = u16::try_from(page_header.len()).ok()?;

    let mut out =
        Vec::with_capacity(32 + content_header.len() + page_header.len() + pages.len() * 4);
    out.extend_from_slice(&65537u32.to_be_bytes());
    out.extend_from_slice(&(12 + content_len).to_be_bytes());
    out.extend_from_slice(&content_len.to_be_bytes());
    out.extend_from_slice(content_header.as_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&page_header_len.to_be_bytes());
    out.extend_from_slice(&pages_len.to_be_bytes());
    out.extend_from_slice(&32u16.to_be_bytes());
    out.extend_from_slice(page_header.as_bytes());
    for page in pages {
        out.extend_from_slice(&page.to_be_bytes());
    }
    Some(out)
}

fn json_escape(s: &str) -> String {
    let quoted = serde_json::to_string(s).unwrap_or_default();
    quoted[1..quoted.len() - 1].to_owned()
}

/// A content GUID derived from the ASIN, so sending a book again gives the
/// same APNX. calibre uses 8 hex digits of a random UUID.
pub fn guid_for(asin: &str) -> String {
    sha1_smol::Sha1::from(asin.as_bytes()).digest().to_string()[..8].to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(version: u32, text: u32) -> MobiHeader {
        MobiHeader {
            pdb_name: "Book".into(),
            mobi_version: version,
            text_length: text,
            asin: Some("0b1e1f8a-3c2d-4e5f-8a9b-0c1d2e3f4a5b".into()),
            cdetype: Some("EBOK".into()),
            ..MobiHeader::default()
        }
    }

    #[test]
    fn pages_every_2300_characters() {
        assert_eq!(fast_pages(0), Vec::<u32>::new());
        assert_eq!(fast_pages(1), [0]);
        assert_eq!(fast_pages(2300), [0]);
        assert_eq!(fast_pages(2301), [0, 2300]);
        assert_eq!(fast_pages(5000), [0, 2300, 4600]);
    }

    /// The same bytes calibre's `generate_apnx` writes for these inputs.
    #[test]
    fn matches_calibre_layout() {
        let apnx = build(&header(8, 5000), &fast_pages(5000), "abcdef01").unwrap();
        let content = r#"{"contentGuid":"abcdef01","asin":"0b1e1f8a-3c2d-4e5f-8a9b-0c1d2e3f4a5b","cdeType":"EBOK","format":"MOBI_8","fileRevisionId":"1","acr":"Book"}"#;
        let page = r#"{"asin":"0b1e1f8a-3c2d-4e5f-8a9b-0c1d2e3f4a5b","pageMap":"(1,a,1)"}"#;
        let mut expected = Vec::new();
        expected.extend_from_slice(&65537u32.to_be_bytes());
        expected.extend_from_slice(&(12 + u32::try_from(content.len()).unwrap()).to_be_bytes());
        expected.extend_from_slice(&(u32::try_from(content.len()).unwrap()).to_be_bytes());
        expected.extend_from_slice(content.as_bytes());
        expected.extend_from_slice(&[0, 1]);
        expected.extend_from_slice(&(u16::try_from(page.len()).unwrap()).to_be_bytes());
        expected.extend_from_slice(&[0, 3, 0, 32]);
        expected.extend_from_slice(page.as_bytes());
        for p in [0u32, 2300, 4600] {
            expected.extend_from_slice(&p.to_be_bytes());
        }
        assert_eq!(apnx, expected);
    }

    #[test]
    fn accurate_pages_count_paragraph_lines() {
        // 40 paragraphs of 100 characters: each is a line at `<p` and one
        // more after 70 characters (the count runs on, so some get two).
        let para = format!("<p class=\"x\">{}</p>", "a".repeat(100));
        let html = format!("<html><body>{}</body></html>", para.repeat(40));
        let pages = accurate_pages(html.as_bytes());
        let lines: usize = 40 + 40 * 100 / 70;
        assert_eq!(pages.len(), lines.div_ceil(32));
        // calibre records a line two bytes before the `p`, one before `<`.
        assert_eq!(pages[0] as usize, html.find("<p").unwrap() - 1);
        // Text outside paragraphs and inside tags isn't counted.
        assert!(accurate_pages(b"<div>no paragraphs here</div>").is_empty());
        assert_eq!(accurate_pages(b"<P>Upper</P>"), [0]);
    }

    #[test]
    fn whole_book_uses_the_accurate_method() {
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/amazon/book.azw3"),
        )
        .unwrap();
        let header = crate::mobi::parse(&bytes).unwrap();
        let text = crate::text::extract(&bytes).unwrap();
        assert_eq!(pages(&bytes, &header), accurate_pages(&text));
        assert!(!pages(&bytes, &header).is_empty());
        // Not a readable book: the fast method from the header's length.
        assert_eq!(pages(b"junk", &header), fast_pages(header.text_length));
    }

    #[test]
    fn older_mobi_gets_the_simple_header() {
        let apnx = build(&header(6, 100), &fast_pages(100), "abcdef01").unwrap();
        let text = String::from_utf8_lossy(&apnx);
        assert!(!text.contains("MOBI_8"));
        assert!(text.contains(r#""cdeType":"EBOK","fileRevisionId":"1"}"#));
        assert!(build(&header(8, 0), &[], "x").is_none());
        assert_eq!(guid_for("a"), guid_for("a"));
        assert_eq!(guid_for("a").len(), 8);
    }
}
