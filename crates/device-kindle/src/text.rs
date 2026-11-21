//! A book's text as the Kindle counts positions in it: the text records of a
//! MOBI or KF8 file, decompressed and joined. Positions in APNX files and
//! KRDS annotations are byte offsets into this. Follows calibre
//! (`ebooks/mobi/reader/mobi6.py`, `extract_text` and `sizeof_trailing_entries`).
//!
//! Only PalmDOC and uncompressed books are read. HUFF/CDIC compression is
//! used by Amazon's own store books, which Parallax doesn't send.

use crate::mobi::record_offsets;

const NO_COMPRESSION: u16 = 1;
const PALMDOC: u16 = 2;

/// The decompressed text of a whole MOBI/AZW3 file, or `None` when it isn't
/// one, is encrypted, or uses a compression this doesn't read.
pub fn extract(bytes: &[u8]) -> Option<Vec<u8>> {
    let offsets = record_offsets(bytes)?;
    let record = |i: usize| -> Option<&[u8]> {
        let start = *offsets.get(i)?;
        let end = offsets.get(i + 1).copied().unwrap_or(bytes.len());
        bytes.get(start..end)
    };
    let r0 = record(0)?;
    let compression = u16_at(r0, 0)?;
    let text_length = u32_at(r0, 4)? as usize;
    let count = usize::from(u16_at(r0, 8)?);
    if u16_at(r0, 12)? != 0 || !matches!(compression, NO_COMPRESSION | PALMDOC) {
        return None;
    }
    let flags = extra_flags(r0);
    let mut text = Vec::with_capacity(text_length);
    for i in 1..=count {
        let data = record(i)?;
        let data = &data[..data.len() - trailing_size(data, flags).min(data.len())];
        if compression == PALMDOC {
            text.extend(palmdoc(data)?);
        } else {
            text.extend_from_slice(data);
        }
    }
    text.truncate(text_length);
    Some(text)
}

/// The Kindle location of a position, as the Kindle and calibre number
/// them: one every 150 bytes of text, starting at 1.
pub fn location(position: u64) -> u32 {
    u32::try_from(position / 150 + 1).unwrap_or(u32::MAX)
}

/// The words between two positions, as a reader would see them: markup
/// dropped, character references decoded and whitespace collapsed. `end` is
/// the last byte, as KRDS annotations give it. `None` when nothing's left.
pub fn passage(text: &[u8], start: u64, end: u64) -> Option<String> {
    let start = usize::try_from(start).ok()?;
    let end = usize::try_from(end).ok()?.saturating_add(1).min(text.len());
    let mut bytes = text.get(start..end)?;
    // A passage that starts inside a tag begins after it.
    if let Some(close) = bytes.iter().position(|&b| b == b'>')
        && !bytes[..close].contains(&b'<')
    {
        bytes = &bytes[close + 1..];
    }
    let mut plain = String::new();
    let mut in_tag = false;
    for ch in String::from_utf8_lossy(bytes).chars() {
        match ch {
            '<' => {
                in_tag = true;
                plain.push(' ');
            }
            '>' => in_tag = false,
            _ if !in_tag => plain.push(ch),
            _ => {}
        }
    }
    let plain = decode_entities(&plain);
    // A cut through a multibyte character leaves a replacement character.
    let plain = plain.replace('\u{FFFD}', "");
    let words: Vec<&str> = plain.split_whitespace().collect();
    (!words.is_empty()).then(|| words.join(" "))
}

fn decode_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let decoded = after.find(';').filter(|&semi| semi <= 10).and_then(|semi| {
            let name = &after[1..semi];
            let ch = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => name
                    .strip_prefix("#x")
                    .or_else(|| name.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            }?;
            Some((ch, semi))
        });
        match decoded {
            Some((ch, semi)) => {
                out.push(ch);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The MOBI header's extra data flags, which say what trails each record.
fn extra_flags(r0: &[u8]) -> u16 {
    let header_len = u32_at(r0, 20).unwrap_or(0) as usize;
    if r0.get(16..20) != Some(b"MOBI") || header_len < 0xE4 {
        return 0;
    }
    u16_at(r0, 0xF2).unwrap_or(0)
}

/// How many bytes at the end of a text record aren't text: one
/// backward-encoded size per flag bit above bit 0, then the multibyte
/// overlap when bit 0 is set.
fn trailing_size(data: &[u8], flags: u16) -> usize {
    let mut size = 0;
    let mut bits = flags >> 1;
    while bits != 0 {
        if bits & 1 != 0 {
            size += backward_varint(&data[..data.len().saturating_sub(size)]);
        }
        bits >>= 1;
    }
    if flags & 1 != 0
        && let Some(&last) = data.len().checked_sub(size + 1).and_then(|i| data.get(i))
    {
        size += usize::from(last & 0b11) + 1;
    }
    size
}

/// A size stored at the end of `data`, read backwards, seven bits a byte,
/// the first byte read carrying the high bit as its stop marker.
fn backward_varint(data: &[u8]) -> usize {
    let mut value = 0usize;
    let mut shift = 0;
    for &byte in data.iter().rev().take(4) {
        value |= usize::from(byte & 0x7F) << shift;
        if byte & 0x80 != 0 {
            break;
        }
        shift += 7;
    }
    value
}

/// PalmDOC's LZ77 variant.
fn palmdoc(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(input.len() * 2);
    let mut i = 0;
    while let Some(&byte) = input.get(i) {
        i += 1;
        match byte {
            0x00 | 0x09..=0x7F => out.push(byte),
            0x01..=0x08 => {
                out.extend_from_slice(input.get(i..i + usize::from(byte))?);
                i += usize::from(byte);
            }
            0x80..=0xBF => {
                let next = usize::from(*input.get(i)?);
                i += 1;
                let distance = ((usize::from(byte) & 0x3F) << 5) | (next >> 3);
                let length = (next & 0x07) + 3;
                if distance == 0 || distance > out.len() {
                    return None;
                }
                let start = out.len() - distance;
                for j in 0..length {
                    out.push(out[start + j % distance]);
                }
            }
            0xC0..=0xFF => {
                out.push(b' ');
                out.push(byte ^ 0x80);
            }
        }
    }
    Some(out)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/amazon")
                .join(name),
        )
        .unwrap()
    }

    #[test]
    fn extracts_the_text_of_an_azw3() {
        let bytes = fixture("book.azw3");
        let header = crate::mobi::parse(&bytes).unwrap();
        let text = extract(&bytes).unwrap();
        assert_eq!(text.len(), header.text_length as usize);
        let html = String::from_utf8_lossy(&text);
        assert!(html.contains("<html"), "{}", &html[..200.min(html.len())]);
        assert!(html.contains("</p>"));
    }

    #[test]
    fn extracts_the_text_of_an_old_mobi() {
        let bytes = fixture("old.mobi");
        let header = crate::mobi::parse(&bytes).unwrap();
        if let Some(text) = extract(&bytes) {
            assert_eq!(text.len(), header.text_length as usize);
            assert!(String::from_utf8_lossy(&text).contains("<p"));
        }
    }

    #[test]
    fn palmdoc_round_trips_each_code() {
        // Literal, a counted literal run, a back-reference and space + char.
        let data = [b'a', b'b', b'c', 0x80, 0x18, 0x02, 0x80, 0x81, 0xC1];
        let out = palmdoc(&data).unwrap();
        assert_eq!(out, b"abcabc\x80\x81 A");
        assert!(palmdoc(&[0x80]).is_none());
        assert!(palmdoc(&[0x80, 0x18]).is_none());
    }

    #[test]
    fn passages_read_like_the_page() {
        let text = b"<p class=\"a\">It was the <i>best</i> of&nbsp;times &amp; the\n worst.</p><p>Next</p>";
        let start = 13; // the `I` of "It"
        let end = text.iter().position(|&b| b == b'.').unwrap() as u64;
        assert_eq!(
            passage(text, start, end).unwrap(),
            "It was the best of times & the worst."
        );
        // Starting inside a tag skips the rest of it.
        assert_eq!(passage(text, 5, 18).unwrap(), "It was");
        assert_eq!(passage(text, 0, 12), None);
        assert_eq!(passage(text, 500, 600), None);
        assert_eq!(
            decode_entities("&#8217;&#x2014;&bogus &"),
            "\u{2019}\u{2014}&bogus &"
        );
        assert_eq!(location(0), 1);
        assert_eq!(location(1499), 10);
    }

    #[test]
    fn strips_trailing_entries() {
        // Text, a multibyte overlap of 2 (low bits 01, plus 1), then a
        // trailing entry of 2 bytes, its size byte last.
        let data = [b'x', b'y', 0xAA, 0x01, 0xBB, 0x82];
        assert_eq!(trailing_size(&data, 0b11), 4);
        assert_eq!(trailing_size(&data, 0), 0);
        assert_eq!(backward_varint(&[0x80 | 0x01, 0x02]), 0x82);
    }
}
