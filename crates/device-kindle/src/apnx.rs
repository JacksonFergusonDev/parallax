//! APNX page-number files, ported from calibre (`devices/kindle/apnx.py` and
//! `apnx_page_generator/generators/fast_page_generator.py`). The pages are an
//! estimate, not a print edition's: one every 2,300 characters of
//! uncompressed text, calibre's default "fast" method. calibre's slower
//! methods need the decompressed book text and aren't ported.

use crate::mobi::MobiHeader;

/// Characters of uncompressed text per page, calibre's fast method.
pub const CHARS_PER_PAGE: u32 = 2300;

/// Page start positions: one every [`CHARS_PER_PAGE`] characters.
pub fn fast_pages(text_length: u32) -> Vec<u32> {
    (0..text_length).step_by(CHARS_PER_PAGE as usize).collect()
}

/// The APNX file for a book, or `None` when it has no text to number.
/// `guid` is the 8-character content GUID; calibre uses a random one.
pub fn build(header: &MobiHeader, guid: &str) -> Option<Vec<u8>> {
    let pages = fast_pages(header.text_length);
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
        let apnx = build(&header(8, 5000), "abcdef01").unwrap();
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
    fn older_mobi_gets_the_simple_header() {
        let apnx = build(&header(6, 100), "abcdef01").unwrap();
        let text = String::from_utf8_lossy(&apnx);
        assert!(!text.contains("MOBI_8"));
        assert!(text.contains(r#""cdeType":"EBOK","fileRevisionId":"1"}"#));
        assert!(build(&header(8, 0), "x").is_none());
        assert_eq!(guid_for("a"), guid_for("a"));
        assert_eq!(guid_for("a").len(), 8);
    }
}
