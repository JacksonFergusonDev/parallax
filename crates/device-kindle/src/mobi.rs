//! Just enough of the PDB, MOBI and EXTH headers to list books, name cover
//! thumbnails and build APNX files, without decompressing anything. Field
//! offsets follow calibre (`ebooks/mobi/reader/headers.py`, `MetadataHeader`).

/// How much of a file to read first. Record 0 of a book made by calibre,
/// KindleGen or boko fits well within this.
pub const PREFIX_LEN: u64 = 256 * 1024;

const PDB_HEADER_LEN: usize = 78;
const MOBI_IDENT: &[u8] = b"BOOKMOBI";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MobiHeader {
    /// The PDB database name, which APNX files carry as `acr`.
    pub pdb_name: String,
    /// 8 for KF8 (AZW3); 6 or lower for older MOBI and combo files.
    pub mobi_version: u32,
    /// Uncompressed text length (PalmDOC header), used for page numbers.
    pub text_length: u32,
    /// Non-zero when the book is encrypted (DRM).
    pub encryption: u16,
    pub title: Option<String>,
    pub authors: Vec<String>,
    /// EXTH 113 (or 504): the store ASIN, or a UUID in sideloaded books.
    pub asin: Option<String>,
    /// EXTH 501: `EBOK` for books, `PDOC` for personal documents.
    pub cdetype: Option<String>,
    pub language: Option<String>,
    /// Record index of the cover image, when the book names one.
    pub cover_record: Option<usize>,
}

impl MobiHeader {
    /// The name a Kindle gives this book's home-screen thumbnail, from
    /// calibre `devices/kindle/driver.py` (`thumbnail_filename`).
    pub fn thumbnail_name(&self) -> Option<String> {
        let asin = self.asin.as_deref().filter(|a| !a.is_empty())?;
        let cdetype = self.cdetype.as_deref().filter(|c| !c.is_empty())?;
        Some(format!("thumbnail_{asin}_{cdetype}_portrait.jpg"))
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(b.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Byte offset of each PDB record.
pub fn record_offsets(bytes: &[u8]) -> Option<Vec<usize>> {
    if bytes.get(60..68)? != MOBI_IDENT {
        return None;
    }
    let count = usize::from(u16_at(bytes, 76)?);
    (0..count)
        .map(|i| u32_at(bytes, PDB_HEADER_LEN + i * 8).map(|o| o as usize))
        .collect()
}

/// How many bytes of the file [`parse`] needs, once the record list is in
/// `prefix`: through the end of record 0.
pub fn needed_len(prefix: &[u8]) -> Option<usize> {
    let offsets = record_offsets(prefix)?;
    Some(offsets.get(1).copied().unwrap_or(prefix.len()))
}

/// Reads the headers from the start of a MOBI/AZW3 file. `None` if it isn't
/// one or `bytes` stops before the end of record 0.
pub fn parse(bytes: &[u8]) -> Option<MobiHeader> {
    let offsets = record_offsets(bytes)?;
    let start = *offsets.first()?;
    let end = offsets.get(1).copied().unwrap_or(bytes.len());
    let r0 = bytes.get(start..end.min(bytes.len()))?;
    let pdb_name = String::from_utf8_lossy(bytes.get(..32)?)
        .trim_end_matches('\0')
        .to_owned();
    let mut header = MobiHeader {
        pdb_name,
        text_length: u32_at(r0, 4)?,
        encryption: u16_at(r0, 12)?,
        ..MobiHeader::default()
    };
    if r0.get(16..20) != Some(b"MOBI") {
        return Some(header);
    }
    let header_len = u32_at(r0, 20)? as usize;
    header.mobi_version = u32_at(r0, 36).unwrap_or(0);
    let first_image = u32_at(r0, 108).map(|i| i as usize);
    if let (Some(off), Some(len)) = (u32_at(r0, 84), u32_at(r0, 88)) {
        let (off, len) = (off as usize, len as usize);
        if let Some(name) = r0.get(off..off + len) {
            let name = String::from_utf8_lossy(name).trim().to_owned();
            if !name.is_empty() {
                header.title = Some(name);
            }
        }
    }
    let exth_flags = u32_at(r0, 128).unwrap_or(0);
    if exth_flags & 0x40 != 0 {
        read_exth(r0.get(16 + header_len..)?, &mut header, first_image);
    }
    Some(header)
}

fn read_exth(exth: &[u8], header: &mut MobiHeader, first_image: Option<usize>) {
    if exth.get(..4) != Some(b"EXTH") {
        return;
    }
    let Some(count) = u32_at(exth, 8) else { return };
    let mut p = 12;
    let mut asin_504 = None;
    for _ in 0..count {
        let (Some(kind), Some(len)) = (u32_at(exth, p), u32_at(exth, p + 4)) else {
            return;
        };
        let len = len as usize;
        if len < 8 {
            return;
        }
        let Some(data) = exth.get(p + 8..p + len) else {
            return;
        };
        let text = || String::from_utf8_lossy(data).trim().to_owned();
        match kind {
            100 => header.authors.push(text()),
            113 => header.asin = Some(text()),
            201 => {
                if let (Some(first), Some(index)) = (first_image, u32_at(data, 0)) {
                    // 0xFFFFFFFF means no cover.
                    if index != u32::MAX {
                        header.cover_record = Some(first + index as usize);
                    }
                }
            }
            501 => header.cdetype = Some(text()),
            503 => header.title = Some(text()),
            504 => asin_504 = Some(text()),
            524 => header.language = Some(text()),
            _ => {}
        }
        p += len;
    }
    if header.asin.as_deref().is_none_or(str::is_empty) {
        header.asin = asin_504;
    }
    header.authors.retain(|a| !a.is_empty());
}

/// The cover image's bytes from a whole MOBI/AZW3 file.
pub fn cover(bytes: &[u8], header: &MobiHeader) -> Option<Vec<u8>> {
    let offsets = record_offsets(bytes)?;
    let index = header.cover_record?;
    let start = *offsets.get(index)?;
    let end = offsets.get(index + 1).copied().unwrap_or(bytes.len());
    let data = bytes.get(start..end)?;
    // JPEG, PNG or GIF; anything else is a resource marker, not an image.
    let image = data.starts_with(&[0xFF, 0xD8])
        || data.starts_with(b"\x89PNG")
        || data.starts_with(b"GIF8");
    image.then(|| data.to_vec())
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
    fn reads_azw3_headers() {
        let bytes = fixture("book.azw3");
        let h = parse(&bytes).unwrap();
        assert_eq!(h.mobi_version, 8);
        assert!(h.text_length > 0);
        assert_eq!(h.encryption, 0);
        assert!(h.title.is_some());
        assert!(!h.authors.is_empty());
        assert!(!h.pdb_name.is_empty());
        if h.cover_record.is_some() {
            assert!(cover(&bytes, &h).is_some());
        }
        assert!(needed_len(&bytes).unwrap() <= bytes.len());
    }

    #[test]
    fn reads_old_mobi_headers() {
        let h = parse(&fixture("old.mobi")).unwrap();
        assert!(h.mobi_version < 8);
        assert!(h.title.is_some());
    }

    #[test]
    fn rejects_other_files() {
        assert!(parse(b"PK\x03\x04 not a mobi").is_none());
        assert!(parse(&[0; 100]).is_none());
    }

    #[test]
    fn thumbnail_name_needs_asin_and_type() {
        let mut h = MobiHeader::default();
        assert_eq!(h.thumbnail_name(), None);
        h.asin = Some("abc".into());
        h.cdetype = Some("EBOK".into());
        assert_eq!(
            h.thumbnail_name().unwrap(),
            "thumbnail_abc_EBOK_portrait.jpg"
        );
    }
}
