//! ComicInfo.xml, the de facto metadata file in comic archives (Anansi
//! Project schema 2.0). Every simple field is kept, known or not, and the
//! `<Pages>` block is kept verbatim, so rewriting the file loses nothing.

use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::Path;

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::entries::{Kind, classify, normalise};
use crate::error::{Error, Result, damaged, io};

/// Schema 2.0's element order, which some readers expect when writing.
const ORDER: [&str; 44] = [
    "Title",
    "Series",
    "Number",
    "Count",
    "Volume",
    "AlternateSeries",
    "AlternateNumber",
    "AlternateCount",
    "Summary",
    "Notes",
    "Year",
    "Month",
    "Day",
    "Writer",
    "Penciller",
    "Inker",
    "Colorist",
    "Letterer",
    "CoverArtist",
    "Editor",
    "Translator",
    "Publisher",
    "Imprint",
    "Genre",
    "Tags",
    "Web",
    "PageCount",
    "LanguageISO",
    "Format",
    "BlackAndWhite",
    "Manga",
    "Characters",
    "Teams",
    "Locations",
    "ScanInformation",
    "StoryArc",
    "StoryArcNumber",
    "SeriesGroup",
    "AgeRating",
    "Pages",
    "CommunityRating",
    "MainCharacterOrTeam",
    "Review",
    "GTIN",
];

/// ComicInfo's `Manga` field: whether pages read right to left.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manga {
    Unknown,
    No,
    Yes,
    YesAndRightToLeft,
}

impl Manga {
    pub fn is_manga(self) -> bool {
        matches!(self, Self::Yes | Self::YesAndRightToLeft)
    }
}

/// A person's part in making the comic, from the ComicInfo field they're in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Writer,
    Penciller,
    Inker,
    Colorist,
    Letterer,
    CoverArtist,
    Editor,
    Translator,
}

impl Role {
    pub const ALL: [Role; 8] = [
        Role::Writer,
        Role::Penciller,
        Role::Inker,
        Role::Colorist,
        Role::Letterer,
        Role::CoverArtist,
        Role::Editor,
        Role::Translator,
    ];

    pub fn field(self) -> &'static str {
        match self {
            Role::Writer => "Writer",
            Role::Penciller => "Penciller",
            Role::Inker => "Inker",
            Role::Colorist => "Colorist",
            Role::Letterer => "Letterer",
            Role::CoverArtist => "CoverArtist",
            Role::Editor => "Editor",
            Role::Translator => "Translator",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credit {
    pub name: String,
    pub role: Role,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComicInfo {
    /// Simple elements in document order: (name, text).
    fields: Vec<(String, String)>,
    /// The `<Pages>` element as it was written.
    pages_xml: Option<String>,
}

impl ComicInfo {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn parse(xml: &str) -> Result<Self> {
        let doc = roxmltree::Document::parse(xml).map_err(|e| Error::ComicInfo(e.to_string()))?;
        let root = doc.root_element();
        if root.tag_name().name() != "ComicInfo" {
            return Err(Error::ComicInfo(format!(
                "root element is <{}>",
                root.tag_name().name()
            )));
        }
        let mut info = Self::default();
        for child in root.children().filter(roxmltree::Node::is_element) {
            let name = child.tag_name().name();
            if name == "Pages" {
                info.pages_xml = Some(xml[child.range()].to_owned());
            } else if child.children().all(|c| c.is_text()) {
                let text = child.text().unwrap_or_default().trim();
                if !text.is_empty() {
                    info.set(name, Some(text));
                }
            }
        }
        Ok(info)
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.fields
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v.as_str())
    }

    /// Sets a field, or removes it with `None` or blank text.
    pub fn set(&mut self, name: &str, value: Option<&str>) {
        let value = value.map(str::trim).filter(|v| !v.is_empty());
        match (self.fields.iter().position(|(n, _)| n == name), value) {
            (Some(i), Some(v)) => v.clone_into(&mut self.fields[i].1),
            (Some(i), None) => {
                self.fields.remove(i);
            }
            (None, Some(v)) => self.fields.push((name.to_owned(), v.to_owned())),
            (None, None) => {}
        }
    }

    pub fn title(&self) -> Option<&str> {
        self.get("Title")
    }

    pub fn series(&self) -> Option<&str> {
        self.get("Series")
    }

    /// Issue or chapter number, as written (`12`, `12.5`).
    pub fn number(&self) -> Option<&str> {
        self.get("Number")
    }

    pub fn volume(&self) -> Option<&str> {
        self.get("Volume")
    }

    pub fn year(&self) -> Option<u16> {
        self.get("Year")?.parse().ok().filter(|&y| y > 0)
    }

    pub fn summary(&self) -> Option<&str> {
        self.get("Summary")
    }

    pub fn publisher(&self) -> Option<&str> {
        self.get("Publisher")
    }

    pub fn language(&self) -> Option<&str> {
        self.get("LanguageISO")
    }

    pub fn manga(&self) -> Manga {
        match self.get("Manga").map(str::to_ascii_lowercase).as_deref() {
            Some("yes") => Manga::Yes,
            Some("yesandrighttoleft") => Manga::YesAndRightToLeft,
            Some("no") => Manga::No,
            _ => Manga::Unknown,
        }
    }

    /// Everyone credited, in role order; names in one field are separated
    /// by commas.
    pub fn credits(&self) -> Vec<Credit> {
        let mut credits: Vec<Credit> = Vec::new();
        for role in Role::ALL {
            for name in self
                .get(role.field())
                .into_iter()
                .flat_map(|v| v.split(','))
            {
                let name = name.trim();
                if !name.is_empty() && !credits.iter().any(|c| c.role == role && c.name == name) {
                    credits.push(Credit {
                        name: name.to_owned(),
                        role,
                    });
                }
            }
        }
        credits
    }

    /// The page marked as the front cover in `<Pages>`, by position.
    pub fn front_cover(&self) -> Option<usize> {
        let xml = self.pages_xml.as_deref()?;
        let doc = roxmltree::Document::parse(xml).ok()?;
        doc.root_element()
            .children()
            .filter(|n| n.has_tag_name("Page"))
            .find(|n| {
                n.attribute("Type")
                    .is_some_and(|t| t.eq_ignore_ascii_case("FrontCover"))
            })
            .and_then(|n| n.attribute("Image")?.trim().parse().ok())
    }

    pub(crate) fn bookmarks(&self) -> Vec<(usize, String)> {
        let Some(doc) = self
            .pages_xml
            .as_deref()
            .and_then(|xml| roxmltree::Document::parse(xml).ok())
        else {
            return Vec::new();
        };
        doc.root_element()
            .children()
            .filter(|node| node.has_tag_name("Page"))
            .filter_map(|node| {
                let title = node.attribute("Bookmark")?.trim();
                let page = node.attribute("Image")?.trim().parse().ok()?;
                (!title.is_empty()).then(|| (page, title.to_owned()))
            })
            .collect()
    }

    /// The file, with known fields in schema order and others after them.
    pub fn to_xml(&self) -> String {
        let mut known: Vec<&(String, String)> = self
            .fields
            .iter()
            .filter(|(n, _)| ORDER.contains(&n.as_str()))
            .collect();
        known.sort_by_key(|(n, _)| ORDER.iter().position(|o| o == n));
        let mut out = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ComicInfo xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\n",
        );
        let mut pages_written = false;
        let write_pages = |out: &mut String| {
            if let Some(pages) = &self.pages_xml {
                out.push_str("  ");
                out.push_str(pages);
                out.push('\n');
            }
        };
        for (name, value) in known {
            if !pages_written
                && ORDER.iter().position(|o| o == name) > ORDER.iter().position(|o| *o == "Pages")
            {
                write_pages(&mut out);
                pages_written = true;
            }
            push_element(&mut out, name, value);
        }
        if !pages_written {
            write_pages(&mut out);
        }
        for (name, value) in self
            .fields
            .iter()
            .filter(|(n, _)| !ORDER.contains(&n.as_str()))
        {
            push_element(&mut out, name, value);
        }
        out.push_str("</ComicInfo>\n");
        out
    }
}

fn push_element(out: &mut String, name: &str, value: &str) {
    out.push_str("  <");
    out.push_str(name);
    out.push('>');
    out.push_str(&escape(value));
    out.push_str("</");
    out.push_str(name);
    out.push_str(">\n");
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// Replaces the CBZ's ComicInfo.xml (or adds one at the top level), copying
/// every other entry byte for byte. The file is rewritten beside itself and
/// swapped in only when complete.
pub fn write_comic_info(cbz: &Path, info: &ComicInfo) -> Result<()> {
    let dir = cbz.parent().unwrap_or(Path::new("."));
    let mut src =
        ZipArchive::new(BufReader::new(File::open(cbz).map_err(io(cbz))?)).map_err(damaged(cbz))?;
    let tmp = tempfile::NamedTempFile::new_in(dir).map_err(io(dir))?;
    let mut dst = ZipWriter::new(BufWriter::new(tmp.as_file()));
    let mut target = "ComicInfo.xml".to_owned();
    // Keep an existing file's place, if it's at the top or in the wrapper.
    for i in 0..src.len() {
        let entry = src.by_index_raw(i).map_err(damaged(cbz))?;
        let name = normalise(&entry.name().map_err(damaged(cbz))?);
        if classify(&name) == Kind::ComicInfo && name.matches('/').count() <= 1 {
            target = name;
            break;
        }
    }
    for i in 0..src.len() {
        let entry = src.by_index_raw(i).map_err(damaged(cbz))?;
        let name = normalise(&entry.name().map_err(damaged(cbz))?);
        if classify(&name) == Kind::ComicInfo {
            continue;
        }
        dst.raw_copy_file(entry).map_err(damaged(cbz))?;
    }
    dst.start_file(
        &target,
        SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
    )
    .map_err(damaged(cbz))?;
    dst.write_all(info.to_xml().as_bytes()).map_err(io(cbz))?;
    dst.finish()
        .map_err(damaged(cbz))?
        .flush()
        .map_err(io(cbz))?;
    tmp.as_file().sync_all().map_err(io(cbz))?;
    drop(src);
    tmp.persist(cbz).map_err(|e| Error::Io {
        path: cbz.to_path_buf(),
        source: e.error,
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<ComicInfo xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
  <Series>Berserk</Series>
  <Volume>1</Volume>
  <Title>The Black Swordsman</Title>
  <Year>2003</Year>
  <Writer>Kentaro Miura</Writer>
  <Penciller>Kentaro Miura, Studio Gaga</Penciller>
  <Translator>Duane Johnson</Translator>
  <LanguageISO>en</LanguageISO>
  <Manga>YesAndRightToLeft</Manga>
  <ScanInformation>danke-Empire</ScanInformation>
  <CustomThing>kept</CustomThing>
  <Pages>
    <Page Image="0" Type="InnerCover"/>
    <Page Image="2" Type="FrontCover" ImageWidth="1810"/>
  </Pages>
</ComicInfo>"#;

    #[test]
    fn reads_fields() {
        let info = ComicInfo::parse(SAMPLE).unwrap();
        assert_eq!(info.series(), Some("Berserk"));
        assert_eq!(info.volume(), Some("1"));
        assert_eq!(info.title(), Some("The Black Swordsman"));
        assert_eq!(info.year(), Some(2003));
        assert_eq!(info.language(), Some("en"));
        assert_eq!(info.manga(), Manga::YesAndRightToLeft);
        assert!(info.manga().is_manga());
        assert_eq!(info.front_cover(), Some(2));
        assert_eq!(info.get("CustomThing"), Some("kept"));
    }

    #[test]
    fn splits_credits_by_role() {
        let info = ComicInfo::parse(SAMPLE).unwrap();
        let credits = info.credits();
        let credits: Vec<(&str, Role)> =
            credits.iter().map(|c| (c.name.as_str(), c.role)).collect();
        assert_eq!(
            credits,
            [
                ("Kentaro Miura", Role::Writer),
                ("Kentaro Miura", Role::Penciller),
                ("Studio Gaga", Role::Penciller),
                ("Duane Johnson", Role::Translator),
            ]
        );
    }

    #[test]
    fn round_trips_without_losing_anything() {
        let info = ComicInfo::parse(SAMPLE).unwrap();
        let xml = info.to_xml();
        let again = ComicInfo::parse(&xml).unwrap();
        // Same fields and pages, only reordered, and stable from then on.
        let mut before = info.fields.clone();
        let mut after = again.fields.clone();
        before.sort();
        after.sort();
        assert_eq!(after, before);
        assert_eq!(again.pages_xml, info.pages_xml);
        assert_eq!(again.to_xml(), xml);
        // Schema order: Title before Series, Pages before unknown fields.
        assert!(xml.find("<Title>") < xml.find("<Series>"));
        assert!(xml.find("<Pages>") < xml.find("<CustomThing>"));
        assert!(xml.find("<Manga>") < xml.find("<Pages>"));
    }

    #[test]
    fn edits_and_escapes() {
        let mut info = ComicInfo::new();
        info.set("Series", Some("Tom & Jerry <Classic>"));
        info.set("Volume", Some(" 3 "));
        info.set("Volume", Some("4"));
        info.set("Title", Some("  "));
        let again = ComicInfo::parse(&info.to_xml()).unwrap();
        assert_eq!(again.series(), Some("Tom & Jerry <Classic>"));
        assert_eq!(again.volume(), Some("4"));
        assert_eq!(again.title(), None);
        info.set("Series", None);
        assert_eq!(info.series(), None);
    }

    #[test]
    fn manga_values() {
        let manga = |v: &str| {
            ComicInfo::parse(&format!("<ComicInfo><Manga>{v}</Manga></ComicInfo>"))
                .unwrap()
                .manga()
        };
        assert_eq!(manga("Yes"), Manga::Yes);
        assert_eq!(manga("No"), Manga::No);
        assert_eq!(manga("Unknown"), Manga::Unknown);
        assert_eq!(manga("yesandrighttoleft"), Manga::YesAndRightToLeft);
    }

    #[test]
    fn refuses_other_documents() {
        assert!(ComicInfo::parse("<package/>").is_err());
        assert!(ComicInfo::parse("not xml").is_err());
    }
}
