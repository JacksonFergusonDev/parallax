//! Kobo models and their screens, which comics are prepared for.
//!
//! Model ids are the last digits of `.kobo/version` (pgaskin/koboutils
//! `kobo/device.go`). Screen sizes are portrait pixels from Kobo's published
//! specifications, matching Kindle Comic Converter's Kobo profiles. A Kobo
//! that isn't listed here gets its size from its own full-screen cover
//! images, which Nickel draws at screen height, and failing that a common
//! size.

use std::fs;
use std::io::Read;
use std::path::Path;

use erc_device::{Screen, ScreenSource};

pub(crate) struct Model {
    pub id: &'static str,
    pub name: &'static str,
    pub width: u32,
    pub height: u32,
    pub colour: bool,
}

const fn model(id: &'static str, name: &'static str, width: u32, height: u32) -> Model {
    Model {
        id,
        name,
        width,
        height,
        colour: false,
    }
}

const fn colour(id: &'static str, name: &'static str, width: u32, height: u32) -> Model {
    Model {
        id,
        name,
        width,
        height,
        colour: true,
    }
}

pub(crate) const MODELS: &[Model] = &[
    model("310", "Kobo Touch", 600, 800),
    model("320", "Kobo Touch", 600, 800),
    model("330", "Kobo Glo", 758, 1024),
    model("340", "Kobo Mini", 600, 800),
    model("350", "Kobo Aura HD", 1080, 1440),
    model("360", "Kobo Aura", 758, 1024),
    model("370", "Kobo Aura H2O", 1080, 1430),
    model("371", "Kobo Glo HD", 1072, 1448),
    model("372", "Kobo Touch 2.0", 600, 800),
    model("373", "Kobo Aura ONE", 1404, 1872),
    model("374", "Kobo Aura H2O Edition 2", 1080, 1440),
    model("375", "Kobo Aura Edition 2", 758, 1024),
    model("376", "Kobo Clara HD", 1072, 1448),
    model("377", "Kobo Forma", 1440, 1920),
    model("378", "Kobo Aura H2O Edition 2", 1080, 1440),
    model("379", "Kobo Aura Edition 2", 758, 1024),
    model("380", "Kobo Forma", 1440, 1920),
    model("381", "Kobo Aura ONE Limited Edition", 1404, 1872),
    model("382", "Kobo Nia", 758, 1024),
    model("383", "Kobo Sage", 1440, 1920),
    model("384", "Kobo Libra H2O", 1264, 1680),
    model("386", "Kobo Clara 2E", 1072, 1448),
    model("387", "Kobo Elipsa", 1404, 1872),
    model("388", "Kobo Libra 2", 1264, 1680),
    model("389", "Kobo Elipsa 2E", 1404, 1872),
    colour("390", "Kobo Libra Colour", 1264, 1680),
    model("391", "Kobo Clara BW", 1072, 1448),
    colour("393", "Kobo Clara Colour", 1072, 1448),
    // Tolino readers run Kobo's software; the app only reads them.
    colour("690", "Tolino Vision Color", 1264, 1680),
    model("691", "Tolino Shine (5th generation)", 1072, 1448),
    colour("693", "Tolino Shine Color", 1072, 1448),
];

/// The most common Kobo screen, assumed when nothing better is known.
pub const ASSUMED_SCREEN: (u32, u32) = (1072, 1448);

/// How many full-screen covers to measure for an unknown model.
const COVERS_SAMPLED: usize = 24;

pub(crate) fn find(short_id: &str) -> Option<&'static Model> {
    MODELS.iter().find(|m| m.id == short_id)
}

/// The screen of a Kobo: its model's, else measured from the full-screen
/// covers in `.kobo-images`, else [`ASSUMED_SCREEN`].
pub(crate) fn screen(short_id: &str, mount: &Path) -> Screen {
    if let Some(m) = find(short_id) {
        return Screen {
            width: m.width,
            height: m.height,
            colour: m.colour,
            source: ScreenSource::Model,
        };
    }
    let sizes = full_cover_sizes(&mount.join(".kobo-images"), COVERS_SAMPLED);
    if let Some((width, height)) = screen_from_covers(&sizes) {
        return Screen {
            width,
            height,
            colour: false,
            source: ScreenSource::Covers,
        };
    }
    Screen {
        width: ASSUMED_SCREEN.0,
        height: ASSUMED_SCREEN.1,
        colour: false,
        source: ScreenSource::Assumed,
    }
}

/// Nickel scales each full-screen cover to fit the screen, so the tallest
/// gives the screen's height and the widest its width. Covers are rarely as
/// wide as the screen, so the width comes from a known model with that
/// height, or the usual 3:4 shape, unless a cover was wider still.
fn screen_from_covers(sizes: &[(u32, u32)]) -> Option<(u32, u32)> {
    let height = sizes.iter().map(|&(_, h)| h).max()?;
    let widest = sizes.iter().map(|&(w, _)| w).max().unwrap_or(0);
    if height < 600 {
        return None;
    }
    let width = MODELS
        .iter()
        .find(|m| m.height == height)
        .map_or(height * 3 / 4, |m| m.width);
    Some((width.max(widest), height))
}

/// Sizes of up to `limit` `N3_FULL` covers under `root`.
fn full_cover_sizes(root: &Path, limit: usize) -> Vec<(u32, u32)> {
    let mut sizes = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if sizes.len() >= limit {
                return sizes;
            }
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_dir()) {
                stack.push(path);
            } else if entry
                .file_name()
                .to_string_lossy()
                .ends_with(" - N3_FULL.parsed")
                && let Some(size) = image_size(&path)
            {
                sizes.push(size);
            }
        }
    }
    sizes
}

/// Width and height of a JPEG or PNG from its header.
pub(crate) fn image_size(path: &Path) -> Option<(u32, u32)> {
    let mut head = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut head)
        .ok()?;
    if head.starts_with(b"\x89PNG\r\n\x1a\n") && head.len() >= 24 {
        let be = |b: &[u8]| u32::from_be_bytes([b[0], b[1], b[2], b[3]]);
        return Some((be(&head[16..20]), be(&head[20..24])));
    }
    if !head.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    // Walk the JPEG segments to the first start-of-frame marker.
    let mut i = 2;
    while i + 9 < head.len() {
        if head[i] != 0xff {
            return None;
        }
        let marker = head[i + 1];
        let len = usize::from(u16::from_be_bytes([head[i + 2], head[i + 3]]));
        if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            let h = u16::from_be_bytes([head[i + 5], head[i + 6]]);
            let w = u16::from_be_bytes([head[i + 7], head[i + 8]]);
            return Some((w.into(), h.into()));
        }
        i += 2 + len;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_ids_are_unique_and_screens_portrait() {
        for (i, m) in MODELS.iter().enumerate() {
            assert!(
                MODELS[i + 1..].iter().all(|o| o.id != m.id),
                "{} listed twice",
                m.id
            );
            assert!(m.height > m.width, "{} isn't portrait", m.name);
        }
    }

    #[test]
    fn known_models_use_their_screen() {
        let s = screen("388", Path::new("/nonexistent"));
        assert_eq!((s.width, s.height, s.colour), (1264, 1680, false));
        assert_eq!(s.source, ScreenSource::Model);
        assert!(screen("393", Path::new("/nonexistent")).colour);
    }

    #[test]
    fn covers_give_the_screen_of_an_unknown_model() {
        // The Libra 2 report's N3_FULL sizes.
        let libra2 = [(1118, 1680), (1103, 1680), (1176, 1680)];
        assert_eq!(screen_from_covers(&libra2), Some((1264, 1680)));
        // A height no model has: the usual 3:4 shape.
        assert_eq!(screen_from_covers(&[(900, 1600)]), Some((1200, 1600)));
        // A cover wider than that shape wins.
        assert_eq!(screen_from_covers(&[(1250, 1600)]), Some((1250, 1600)));
        assert_eq!(screen_from_covers(&[]), None);
        assert_eq!(screen_from_covers(&[(100, 150)]), None);
    }

    #[test]
    fn unknown_model_without_covers_is_assumed() {
        let dir = tempfile::tempdir().unwrap();
        let s = screen("999", dir.path());
        assert_eq!((s.width, s.height), ASSUMED_SCREEN);
        assert_eq!(s.source, ScreenSource::Assumed);
    }

    #[test]
    fn measures_covers_on_disk() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join(".kobo-images/12/34");
        fs::create_dir_all(&nested).unwrap();
        let png = |w: u32, h: u32| {
            let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
            b.extend_from_slice(&w.to_be_bytes());
            b.extend_from_slice(&h.to_be_bytes());
            b
        };
        fs::write(nested.join("a - N3_FULL.parsed"), png(1350, 1872)).unwrap();
        fs::write(nested.join("a - N3_LIBRARY_FULL.parsed"), png(355, 2000)).unwrap();
        let s = screen("999", dir.path());
        assert_eq!((s.width, s.height), (1404, 1872));
        assert_eq!(s.source, ScreenSource::Covers);
    }
}
