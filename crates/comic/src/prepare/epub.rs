//! Fixed-layout packaging follows the Parallax Kobo comics spike. The facing
//! halves and backward spine alignment follow KCC v13.0.0 buildOPF (ISC,
//! Ciro Mattia Gonano and Pawel Jastrzebski; see KCC-NOTICES).
use std::borrow::Cow;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufWriter, Cursor, Write};
use std::path::Path;

use image::ImageFormat;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use super::raster::{Raster, Side};
use super::{Error, Metadata, Options, Profile, Spreads};

const STORED: SimpleFileOptions =
    SimpleFileOptions::DEFAULT.compression_method(CompressionMethod::Stored);
const DEFLATED: SimpleFileOptions = SimpleFileOptions::DEFAULT;

pub(super) struct FixedLayout {
    zip: ZipWriter<BufWriter<File>>,
    manifest: String,
    sides: Vec<Option<Side>>,
}

impl FixedLayout {
    pub fn new(path: &Path) -> Result<Self, Error> {
        let mut epub = Self {
            zip: ZipWriter::new(BufWriter::new(File::create(path)?)),
            manifest: String::new(),
            sides: Vec::new(),
        };
        epub.write("mimetype", b"application/epub+zip", STORED)?;
        epub.write("META-INF/container.xml", br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
<rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#, DEFLATED)?;
        Ok(epub)
    }

    fn write(&mut self, path: &str, data: &[u8], options: SimpleFileOptions) -> Result<(), Error> {
        self.zip.start_file(path, options)?;
        self.zip.write_all(data)?;
        Ok(())
    }

    /// The cover is separate artwork. Preserve JPEG and PNG bytes and use a
    /// lossless PNG of the first frame for other supported archive images.
    pub fn add_cover(&mut self, bytes: &[u8], name: &str) -> Result<(), Error> {
        let error = |message: String| Error::Page {
            page: name.to_owned(),
            message,
        };
        crate::shape::image_dimensions(bytes)
            .ok_or_else(|| error("missing or unsupported image dimensions".into()))?;
        let format = image::guess_format(bytes).map_err(|e| error(e.to_string()))?;
        let (data, extension, mime) = match format {
            ImageFormat::Jpeg => (Cow::Borrowed(bytes), "jpg", "image/jpeg"),
            ImageFormat::Png => (Cow::Borrowed(bytes), "png", "image/png"),
            _ => {
                let cover = image::load_from_memory_with_format(bytes, format)
                    .map_err(|e| error(e.to_string()))?;
                let mut png = Cursor::new(Vec::new());
                cover
                    .write_to(&mut png, ImageFormat::Png)
                    .map_err(|e| error(e.to_string()))?;
                (Cow::Owned(png.into_inner()), "png", "image/png")
            }
        };
        let path = format!("images/cover.{extension}");
        let _ = writeln!(
            self.manifest,
            r#"<item id="cover" href="{path}" media-type="{mime}" properties="cover-image"/>"#,
        );
        self.write(&format!("OEBPS/{path}"), &data, STORED)
    }

    pub fn is_empty(&self) -> bool {
        self.sides.is_empty()
    }

    pub fn add_page(&mut self, raster: &Raster) -> Result<(), Error> {
        self.sides.push(raster.side);
        let n = self.sides.len();
        let image = format!("images/p{n:05}.{}", raster.extension());
        let page = format!("pages/p{n:05}.xhtml");
        let _ = writeln!(
            self.manifest,
            r#"<item id="img{n}" href="{image}" media-type="{}"/><item id="p{n}" href="{page}" media-type="application/xhtml+xml"/>"#,
            raster.mime()
        );
        let (w, h) = (raster.width, raster.height);
        let xhtml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Page {n}</title><meta name="viewport" content="width={w}, height={h}"/>
<style>html, body {{margin:0; padding:0; width:{w}px; height:{h}px}} img {{display:block; width:{w}px; height:{h}px}}</style></head>
<body><img src="../{image}" alt=""/></body></html>"#
        );
        self.write(&format!("OEBPS/{page}"), xhtml.as_bytes(), DEFLATED)?;
        self.write(&format!("OEBPS/{image}"), &raster.bytes, STORED)
    }

    pub fn finish(
        mut self,
        book: &Metadata<'_>,
        chapters: &[crate::Chapter],
        profile: Profile,
        options: Options,
    ) -> Result<usize, Error> {
        let title = xml(book.title);
        let mut contents = String::new();
        if chapters.is_empty() || chapters[0].page > 0 {
            let _ = write!(
                contents,
                r#"<li><a href="pages/p00001.xhtml">{title}</a></li>"#
            );
        }
        for chapter in chapters {
            let _ = write!(
                contents,
                r#"<li><a href="pages/p{:05}.xhtml">{}</a></li>"#,
                chapter.page + 1,
                xml(&chapter.title)
            );
        }
        let nav = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head><title>{title}</title></head>
<body><nav epub:type="toc"><ol>{contents}</ol></nav></body>
</html>"#
        );
        self.write("OEBPS/nav.xhtml", nav.as_bytes(), DEFLATED)?;
        let landscape = profile.landscape_spreads && options.spreads != Spreads::KeepWhole;
        let spread = if landscape { "landscape" } else { "none" };
        let direction = if options.rtl { "rtl" } else { "ltr" };
        let metadata = metadata(book);
        let spine = spine(&self.sides, landscape, options.rtl);
        let opf = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid" prefix="rendition: http://www.idpf.org/vocab/rendition/#">
<metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
{metadata}
<meta property="rendition:layout">pre-paginated</meta>
<meta property="rendition:spread">{spread}</meta>
<meta name="cover" content="cover"/>
</metadata>
<manifest><item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
{}
</manifest>
<spine page-progression-direction="{direction}">{spine}</spine>
</package>"#,
            self.manifest
        );
        self.write("OEBPS/content.opf", opf.as_bytes(), DEFLATED)?;
        self.zip.finish()?.flush()?;
        Ok(self.sides.len())
    }
}

fn metadata(book: &Metadata<'_>) -> String {
    let mut fields = format!(
        r#"<dc:identifier id="uid">{}</dc:identifier><dc:title>{}</dc:title><dc:language>{}</dc:language><meta property="dcterms:modified">{}</meta>"#,
        xml(book.identifier),
        xml(book.title),
        xml(book.language.unwrap_or("und")),
        xml(book.modified),
    );
    for author in book.authors {
        let _ = writeln!(fields, "<dc:creator>{}</dc:creator>", xml(author));
    }
    if let Some(description) = book.description {
        let _ = writeln!(
            fields,
            "<dc:description>{}</dc:description>",
            xml(description)
        );
    }
    if let Some(source) = book.source {
        let _ = writeln!(fields, "<dc:source>{}</dc:source>", xml(source));
    }
    for (i, (name, role)) in book.credits.iter().enumerate() {
        let _ = writeln!(
            fields,
            r##"<dc:contributor id="credit{i}">{}</dc:contributor><meta refines="#credit{i}" property="role" scheme="marc:relators">{}</meta>"##,
            xml(name),
            xml(role)
        );
    }
    if let Some(series) = book.series {
        let series = xml(series);
        let _ = writeln!(
            fields,
            r##"<meta property="belongs-to-collection" id="series">{series}</meta><meta refines="#series" property="collection-type">series</meta><meta name="calibre:series" content="{series}"/>"##
        );
        if let Some(index) = book.series_index {
            let index = xml(index);
            let _ = writeln!(
                fields,
                r##"<meta refines="#series" property="group-position">{index}</meta><meta name="calibre:series_index" content="{index}"/>"##
            );
        }
    }
    fields
}

fn spine(sides: &[Option<Side>], landscape: bool, rtl: bool) -> String {
    let first = if rtl { Side::Right } else { Side::Left };
    let mut aligned = vec![first; sides.len()];
    if landscape {
        let mut next = first;
        for (i, side) in sides.iter().enumerate() {
            let side = side.unwrap_or(next);
            aligned[i] = side;
            next = opposite(side, first);
        }
        // Work back from explicit halves so an odd run of ordinary pages
        // cannot bump the first half of the following spread to the other side.
        let mut previous = None;
        for (i, side) in sides.iter().enumerate().rev() {
            if side.is_some() {
                previous = Some(opposite(first, first));
            } else if let Some(side) = previous {
                aligned[i] = side;
                previous = Some(opposite(side, first));
            }
        }
    }
    let mut spine = String::new();
    for (i, side) in aligned.into_iter().enumerate() {
        let property = if landscape {
            match side {
                Side::Left => r#" properties="rendition:page-spread-left""#,
                Side::Right => r#" properties="rendition:page-spread-right""#,
                Side::Center => r#" properties="rendition:page-spread-center""#,
            }
        } else {
            ""
        };
        let _ = writeln!(spine, r#"<itemref idref="p{}"{property}/>"#, i + 1);
    }
    spine
}

fn opposite(side: Side, first: Side) -> Side {
    match side {
        Side::Left => Side::Right,
        Side::Right => Side::Left,
        Side::Center => first,
    }
}

fn xml(value: &str) -> String {
    let mut out = String::new();
    for c in value.chars() {
        let escaped = match c {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            '\'' => "&apos;",
            '\t' | '\n' | '\r' => {
                out.push(c);
                continue;
            }
            c if c.is_control() || matches!(c, '\u{fffe}' | '\u{ffff}') => "",
            c => {
                out.push(c);
                continue;
            }
        };
        out.push_str(escaped);
    }
    out
}
