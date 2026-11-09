//! Page shapes: ordinary pages, double-page spreads, pages too wide to
//! split, and the long strips of webtoons. Thresholds are Kindle Comic
//! Converter's (`image.py`, ISC/GPL-3.0-or-later): a page is a spread when it
//! is more than 1.16 times as wide as tall, and is rotated rather than split
//! from 1.8.

use std::io::Cursor;

use image::ImageReader;

/// Wider than this (width / height) is a double-page spread.
pub const SPREAD_ASPECT: f64 = 1.16;
/// From this width / height, splitting would leave unreadable slivers.
pub const WIDE_ASPECT: f64 = 1.8;
/// From this height / width, a page is part of a vertical strip.
pub const STRIP_ASPECT: f64 = 2.5;
/// Largest page accepted, in pixels (a 1,000 × 100,000 strip is 100 MP).
pub const MAX_PIXELS: u64 = 200_000_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageShape {
    Page,
    /// Two facing pages in one image.
    Spread,
    /// Wider than a spread; rotate rather than split.
    Wide,
    /// A tall slice of a vertical (webtoon) strip.
    Strip,
}

pub fn classify(width: u32, height: u32) -> PageShape {
    if width == 0 || height == 0 {
        return PageShape::Page;
    }
    let aspect = f64::from(width) / f64::from(height);
    if aspect >= WIDE_ASPECT {
        PageShape::Wide
    } else if aspect > SPREAD_ASPECT {
        PageShape::Spread
    } else if 1.0 / aspect >= STRIP_ASPECT {
        PageShape::Strip
    } else {
        PageShape::Page
    }
}

/// Width and height from the image header, or `None` when it isn't an
/// image the app reads or its size is zero or absurd.
pub fn image_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let (w, h) = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()?;
    (w > 0 && h > 0 && u64::from(w) * u64::from(h) <= MAX_PIXELS).then_some((w, h))
}

/// How many pages of each shape a comic has.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Shapes {
    pub pages: usize,
    pub spreads: usize,
    pub wide: usize,
    pub strips: usize,
    /// Pages whose size couldn't be read.
    pub unreadable: usize,
}

impl Shapes {
    pub fn count(sizes: impl IntoIterator<Item = Option<(u32, u32)>>) -> Self {
        let mut s = Self::default();
        for size in sizes {
            s.pages += 1;
            match size.map(|(w, h)| classify(w, h)) {
                None => s.unreadable += 1,
                Some(PageShape::Spread) => s.spreads += 1,
                Some(PageShape::Wide) => s.wide += 1,
                Some(PageShape::Strip) => s.strips += 1,
                Some(PageShape::Page) => {}
            }
        }
        s
    }

    /// Mostly long strips: a webtoon, best read scrolling.
    pub fn is_webtoon(&self) -> bool {
        self.pages > 0 && self.strips * 2 > self.pages
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_by_aspect() {
        assert_eq!(classify(1810, 2560), PageShape::Page);
        assert_eq!(classify(3620, 2560), PageShape::Spread);
        assert_eq!(classify(1000, 1000), PageShape::Page);
        assert_eq!(classify(800, 12_000), PageShape::Strip);
        assert_eq!(classify(0, 100), PageShape::Page);
    }

    #[test]
    fn thresholds_are_exclusive_then_inclusive() {
        // Just either side of 1.16 and 1.8.
        assert_eq!(classify(1160, 1000), PageShape::Page);
        assert_eq!(classify(1161, 1000), PageShape::Spread);
        assert_eq!(classify(1799, 1000), PageShape::Spread);
        assert_eq!(classify(1800, 1000), PageShape::Wide);
        assert_eq!(classify(1000, 2499), PageShape::Page);
        assert_eq!(classify(1000, 2500), PageShape::Strip);
    }

    #[test]
    fn reads_sizes_from_headers_only() {
        let mut png = Vec::new();
        image::GrayImage::new(3, 5)
            .write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .unwrap();
        assert_eq!(image_dimensions(&png), Some((3, 5)));
        assert_eq!(image_dimensions(b"not an image"), None);
        // A PNG header claiming 0 height.
        let mut zero = png.clone();
        zero[20..24].copy_from_slice(&0u32.to_be_bytes());
        assert_eq!(image_dimensions(&zero), None);
    }

    #[test]
    fn counts_and_spots_webtoons() {
        let s = Shapes::count([
            Some((1810, 2560)),
            Some((3620, 2560)),
            Some((5000, 2000)),
            None,
        ]);
        assert_eq!((s.pages, s.spreads, s.wide, s.unreadable), (4, 1, 1, 1));
        assert!(!s.is_webtoon());
        let strip = Shapes::count([Some((800, 12_000)), Some((800, 9_000)), Some((800, 1_000))]);
        assert!(strip.is_webtoon());
    }
}
