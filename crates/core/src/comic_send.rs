//! Preparation settings shared by sending, estimates and update detection.

use erc_comic::prepare::{Metadata, Options, Prepared, Profile, Spreads, prepare};
use erc_convert::Converter;
use erc_device::{Screen, ScreenSource};
use erc_device_kobo::KoboDevice;
use erc_library::{Book, Direction, Format, Library};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::send::{COMIC_AUTOCONTRAST, COMIC_CROP, COMIC_SPREADS, COMIC_UPSCALE};

pub(crate) struct ComicTarget {
    profile: Profile,
    options: Options,
    source: ScreenSource,
}

impl ComicTarget {
    pub fn new(library: &Library, device: &KoboDevice) -> erc_library::Result<Self> {
        let Screen {
            width,
            height,
            colour,
            source,
        } = device.version().screen(device.mount());
        let on = |key| library.setting(key).map(|s| s.as_deref() == Some("on"));
        Ok(Self {
            profile: Profile {
                width,
                height,
                colour,
                // The portrait output is the device-tested baseline. Enable
                // landscape for individual models when verified on hardware.
                landscape_spreads: false,
            },
            options: Options {
                spreads: match library.setting(COMIC_SPREADS)?.as_deref() {
                    Some("keep_whole") => Spreads::KeepWhole,
                    Some("whole_then_halves") => Spreads::WholeThenHalves,
                    _ => Spreads::Split,
                },
                crop: on(COMIC_CROP)?,
                autocontrast: on(COMIC_AUTOCONTRAST)?,
                upscale: on(COMIC_UPSCALE)?,
                rtl: false,
            },
            source,
        })
    }

    fn options(&self, book: &Book) -> Options {
        Options {
            rtl: book.direction == Direction::Rtl,
            ..self.options
        }
    }

    pub fn fingerprint(&self, book: &Book) -> String {
        let encoded = serde_json::to_vec(&("comic-kepub-2", self.profile, self.options(book)))
            .expect("comic settings only contain integers, booleans and enums");
        Sha256::digest(encoded)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    pub fn prepare(
        &self,
        library: &Library,
        book: &Book,
        converter: &dyn Converter,
        progress: impl FnMut(usize, usize),
    ) -> Result<Prepared, String> {
        let identifier = format!("urn:parallax:comic:{}", book.id);
        let modified = chrono::DateTime::parse_from_rfc3339(&book.modified_at)
            .map_or_else(|_| chrono::Utc::now(), |time| time.to_utc())
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
        let credits: Vec<_> = book
            .contributors
            .iter()
            .flatten()
            .map(|credit| (credit.name.clone(), credit.role.clone()))
            .collect();
        let custom_cover = if book.custom_cover {
            library.cover_source(book.id).map_err(|e| e.to_string())?
        } else {
            None
        };
        let metadata = Metadata {
            identifier: &identifier,
            title: &book.title,
            authors: &book.authors,
            credits: &credits,
            series: book.series.as_deref(),
            series_index: book.series_index.as_deref(),
            language: book.language.as_deref(),
            modified: &modified,
            custom_cover: custom_cover.as_deref(),
        };
        prepare(
            &library.book_file(book.id),
            &metadata,
            self.profile,
            self.options(book),
            converter,
            progress,
        )
        .map_err(|e| e.to_string())
    }

    fn estimate(&self, book: &Book) -> u64 {
        let pages = book.pages.unwrap_or_default();
        // The spike's compressed greyscale pages averaged ~1/4 byte per
        // screen pixel. Colour and whole-plus-halves raise the estimate.
        let area = u64::from(self.profile.width) * u64::from(self.profile.height);
        let per_page = area / if self.profile.colour { 2 } else { 4 } + 1_024;
        let pages = pages.saturating_add(1); // cover
        let pages = match self.options.spreads {
            Spreads::KeepWhole => pages,
            Spreads::Split => pages.saturating_add(pages / 8),
            Spreads::WholeThenHalves => pages.saturating_add(pages / 4),
        };
        pages.saturating_mul(per_page)
    }
}

#[derive(Debug, Serialize)]
pub struct TransferEstimate {
    pub bytes: u64,
    pub free: u64,
    pub width: u32,
    pub height: u32,
    pub source: ScreenSource,
}

/// Approximate transfer size when the batch contains comics. The final
/// output is measured again just before each write.
pub fn estimate(
    library: &Library,
    device: &KoboDevice,
    ids: &[i64],
) -> Result<Option<TransferEstimate>, String> {
    let books = ids
        .iter()
        .map(|&id| library.book(id))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    if !books.iter().any(|b| b.format == Format::Cbz) {
        return Ok(None);
    }
    let target = ComicTarget::new(library, device).map_err(|e| e.to_string())?;
    let bytes = books
        .iter()
        .map(|b| {
            if b.format == Format::Cbz {
                target.estimate(b)
            } else {
                b.file_size
            }
        })
        .fold(0, u64::saturating_add);
    Ok(Some(TransferEstimate {
        bytes,
        free: device.free_bytes().map_err(|e| e.to_string())?,
        width: target.profile.width,
        height: target.profile.height,
        source: target.source,
    }))
}

pub(crate) fn check_space(device: &KoboDevice, path: &std::path::Path) -> Result<u64, String> {
    let bytes = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let free = device.free_bytes().map_err(|e| e.to_string())?;
    if bytes > free {
        return Err(format!(
            "Not enough free space on the Kobo ({bytes} bytes needed, {free} available)"
        ));
    }
    Ok(bytes)
}
