//! Preparation settings shared by sending, estimates and update detection.

use erc_comic::prepare::{Metadata, Options, Prepared, Profile, Spreads, prepare};
use erc_convert::Converter;
use erc_device::{Screen, ScreenSource};
use erc_device_kobo::KoboDevice;
use erc_library::{Book, Direction, Format, Library};
use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::send::{COMIC_AUTOCONTRAST, COMIC_CROP, COMIC_DITHER, COMIC_SPREADS, COMIC_UPSCALE};

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
                // Chosen on a Libra 2: very subtly better, for about 4% more space.
                dither: library.setting(COMIC_DITHER)?.as_deref() != Some("off"),
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

    /// A hash of what shapes the prepared file. A guessed screen (from cover
    /// sizes, or assumed) can change as Nickel makes more covers, so only a
    /// known model's screen counts; otherwise only the settings do.
    pub fn fingerprint(&self, book: &Book) -> String {
        let options = self.options(book);
        // Revision 4: colour pages on greyscale screens are stretched for
        // contrast, so every earlier copy may differ.
        let revision = "comic-kepub-4";
        let encoded = if self.source == ScreenSource::Model {
            serde_json::to_vec(&(revision, self.profile, options))
        } else {
            serde_json::to_vec(&(revision, "guessed-screen", options))
        }
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

/// Whether a comic's copy on the Kobo is a KEPUB Parallax prepared, rather
/// than the original archive (a comic added from the Kobo).
pub(crate) fn is_prepared_copy(device_path: &str) -> bool {
    device_path.to_ascii_lowercase().ends_with(".kepub.epub")
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

#[cfg(test)]
mod tests {
    use erc_comic::prepare::Options;

    #[test]
    fn options_added_while_off_keep_fingerprints_of_existing_copies() {
        // The serialised form fingerprints were made from before `dither`.
        let before =
            r#"{"spreads":"split","crop":false,"autocontrast":false,"upscale":false,"rtl":true}"#;
        let options = Options {
            rtl: true,
            ..Options::default()
        };
        assert_eq!(serde_json::to_string(&options).unwrap(), before);
        let dithered = Options {
            dither: true,
            ..options
        };
        assert_ne!(serde_json::to_string(&dithered).unwrap(), before);
    }
}
