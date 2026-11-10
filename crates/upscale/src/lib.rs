//! Cover upscaling and compression clean-up with a neural network.
//!
//! The model takes fixed-size tiles, so a cover is cut into overlapping tiles,
//! each is run through an [`Engine`], and the middles are stitched back
//! together. The overlap gives the network context at tile edges, which keeps
//! seams out of the result. Only [`coreml`] talks to the model.

#[cfg(target_os = "macos")]
mod coreml;
#[cfg(target_os = "macos")]
pub use coreml::CoreMl;

use std::io::Cursor;
use std::sync::atomic::{AtomicBool, Ordering};

use image::imageops::FilterType;
use image::{DynamicImage, ImageFormat, Rgb, RgbImage};

/// Tile side the model was converted with (scripts/convert_upscale_model.py).
pub const TILE: u32 = 288;
/// Context kept around each tile's middle and thrown away afterwards.
const OVERLAP: u32 = 16;
/// Output pixels per input pixel, on each side.
pub const SCALE: u32 = 4;

const MIDDLE: u32 = TILE - 2 * OVERLAP;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cover upscaling isn't available: {0}")]
    Unavailable(String),
    #[error("the upscaling model failed: {0}")]
    Model(String),
    #[error("not a supported image: {0}")]
    BadImage(String),
    #[error("this cover is already full size")]
    AlreadyFullSize,
    #[error("cancelled")]
    Cancelled,
}

pub type Result<T> = std::result::Result<T, Error>;

/// Runs the model on one tile. Input is planar RGB in 0–1 ([`TILE`]² per
/// plane); output is the same at [`SCALE`] times the side.
pub trait Engine {
    fn run(&self, tile: &[f32]) -> Result<Vec<f32>>;
}

/// A cover before and after upscaling, both as PNG.
pub struct Upscaled {
    pub before_png: Vec<u8>,
    pub after_png: Vec<u8>,
    pub before_size: (u32, u32),
    pub after_size: (u32, u32),
}

/// Upscales an encoded cover, ending no taller than `max_height`. Covers
/// already that tall are refused: they gain nothing and take longest.
pub fn upscale_cover(
    engine: &impl Engine,
    cover: &[u8],
    max_height: u32,
    progress: impl FnMut(usize, usize),
    cancelled: &AtomicBool,
) -> Result<Upscaled> {
    let source = image::load_from_memory(cover)
        .map_err(|e| Error::BadImage(e.to_string()))?
        .to_rgb8();
    if source.height() >= max_height {
        return Err(Error::AlreadyFullSize);
    }
    let mut after = upscale(engine, &source, progress, cancelled)?;
    if after.height() > max_height {
        after = DynamicImage::ImageRgb8(after)
            .resize(u32::MAX, max_height, FilterType::Lanczos3)
            .to_rgb8();
    }
    Ok(Upscaled {
        before_png: png(&source)?,
        after_png: png(&after)?,
        before_size: source.dimensions(),
        after_size: after.dimensions(),
    })
}

fn png(image: &RgbImage) -> Result<Vec<u8>> {
    let mut out = Cursor::new(Vec::new());
    image
        .write_to(&mut out, ImageFormat::Png)
        .map_err(|e| Error::BadImage(e.to_string()))?;
    Ok(out.into_inner())
}

/// Number of tiles [`upscale`] runs for an image this size.
pub fn tile_count(width: u32, height: u32) -> usize {
    (width.div_ceil(MIDDLE) * height.div_ceil(MIDDLE)) as usize
}

/// Upscales an image tile by tile, calling `progress(done, total)` after each
/// and stopping between tiles once `cancelled` is set.
pub fn upscale(
    engine: &impl Engine,
    image: &RgbImage,
    mut progress: impl FnMut(usize, usize),
    cancelled: &AtomicBool,
) -> Result<RgbImage> {
    let (width, height) = image.dimensions();
    let total = tile_count(width, height);
    let mut out = RgbImage::new(width * SCALE, height * SCALE);
    let mut done = 0;
    for top in (0..height).step_by(MIDDLE as usize) {
        for left in (0..width).step_by(MIDDLE as usize) {
            if cancelled.load(Ordering::Relaxed) {
                return Err(Error::Cancelled);
            }
            let tile = engine.run(&cut_tile(image, left, top))?;
            let wanted = (TILE * SCALE) as usize;
            if tile.len() != 3 * wanted * wanted {
                return Err(Error::Model(format!(
                    "expected a {wanted}-pixel tile, got {} values",
                    tile.len()
                )));
            }
            let middle = (MIDDLE.min(width - left), MIDDLE.min(height - top));
            paste_middle(&mut out, &tile, left, top, middle);
            done += 1;
            progress(done, total);
        }
    }
    Ok(out)
}

/// The tile whose middle starts at (`left`, `top`), mirrored past the edges.
fn cut_tile(image: &RgbImage, left: u32, top: u32) -> Vec<f32> {
    let (width, height) = image.dimensions();
    let plane = (TILE * TILE) as usize;
    let mut tile = vec![0.0; 3 * plane];
    for y in 0..TILE {
        let source_y = mirror(i64::from(top) + i64::from(y) - i64::from(OVERLAP), height);
        for x in 0..TILE {
            let source_x = mirror(i64::from(left) + i64::from(x) - i64::from(OVERLAP), width);
            let Rgb(pixel) = *image.get_pixel(source_x, source_y);
            for (channel, value) in pixel.into_iter().enumerate() {
                tile[channel * plane + (y * TILE + x) as usize] = f32::from(value) / 255.0;
            }
        }
    }
    tile
}

/// Folds a coordinate back into `0..len`, bouncing off both ends.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "folded into 0..len, and len came from a u32"
)]
fn mirror(position: i64, len: u32) -> u32 {
    let len = i64::from(len);
    if len == 1 {
        return 0;
    }
    let period = 2 * (len - 1);
    let folded = position.rem_euclid(period);
    (if folded < len {
        folded
    } else {
        period - folded
    }) as u32
}

/// Copies a tile's middle into `out`; `middle` is its size in input pixels.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped to 0..=255 first"
)]
fn paste_middle(out: &mut RgbImage, tile: &[f32], left: u32, top: u32, middle: (u32, u32)) {
    let side = TILE * SCALE;
    let plane = (side * side) as usize;
    for y in 0..middle.1 * SCALE {
        for x in 0..middle.0 * SCALE {
            let at = ((y + OVERLAP * SCALE) * side + x + OVERLAP * SCALE) as usize;
            let value =
                |channel: usize| (tile[channel * plane + at].clamp(0.0, 1.0) * 255.0).round() as u8;
            out.put_pixel(
                left * SCALE + x,
                top * SCALE + y,
                Rgb([value(0), value(1), value(2)]),
            );
        }
    }
}

/// Stands in for the Core ML model where there is no Core ML.
#[cfg(not(target_os = "macos"))]
pub struct CoreMl;

#[cfg(not(target_os = "macos"))]
impl CoreMl {
    pub fn load(_path: &std::path::Path) -> Result<Self> {
        Err(Error::Unavailable("it needs macOS".into()))
    }
}

#[cfg(not(target_os = "macos"))]
impl Engine for CoreMl {
    fn run(&self, _tile: &[f32]) -> Result<Vec<f32>> {
        Err(Error::Unavailable("it needs macOS".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enlarges by repeating pixels, so output is predictable from the input.
    struct Nearest;

    impl Engine for Nearest {
        fn run(&self, tile: &[f32]) -> Result<Vec<f32>> {
            let (side, big) = (TILE as usize, (TILE * SCALE) as usize);
            let mut out = vec![0.0; 3 * big * big];
            for channel in 0..3 {
                for y in 0..big {
                    for x in 0..big {
                        let from = y / SCALE as usize * side + x / SCALE as usize;
                        out[channel * big * big + y * big + x] = tile[channel * side * side + from];
                    }
                }
            }
            Ok(out)
        }
    }

    /// A different colour at every position, to catch misplaced tiles.
    fn gradient(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            Rgb([
                (x % 251) as u8,
                (y % 241) as u8,
                ((x * 7 + y * 13) % 239) as u8,
            ])
        })
    }

    fn encoded(image: &RgbImage) -> Vec<u8> {
        png(image).unwrap()
    }

    #[test]
    fn stitched_tiles_match_the_image_exactly() {
        // Spans several tiles in both directions, with partial tiles at the edge.
        let image = gradient(MIDDLE * 2 + 37, MIDDLE + 11);
        let out = upscale(&Nearest, &image, |_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(out.dimensions(), (image.width() * 4, image.height() * 4));
        for (x, y, pixel) in out.enumerate_pixels() {
            assert_eq!(pixel, image.get_pixel(x / 4, y / 4), "at {x},{y}");
        }
    }

    #[test]
    fn images_smaller_than_the_overlap_work() {
        let image = gradient(5, 1);
        let out = upscale(&Nearest, &image, |_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(out.dimensions(), (20, 4));
    }

    #[test]
    fn mirror_bounces_off_both_ends() {
        let folded: Vec<_> = (-4..8).map(|i| mirror(i, 4)).collect();
        assert_eq!(folded, [2, 3, 2, 1, 0, 1, 2, 3, 2, 1, 0, 1]);
        assert_eq!(mirror(-9, 1), 0);
    }

    #[test]
    fn progress_counts_every_tile() {
        let image = gradient(MIDDLE + 1, MIDDLE * 2);
        let mut seen = vec![];
        upscale(
            &Nearest,
            &image,
            |done, total| seen.push((done, total)),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(seen, [(1, 4), (2, 4), (3, 4), (4, 4)]);
        assert_eq!(tile_count(image.width(), image.height()), 4);
    }

    #[test]
    fn cancelling_stops_before_the_next_tile() {
        let image = gradient(MIDDLE * 2, MIDDLE);
        let cancelled = AtomicBool::new(false);
        let mut tiles = 0;
        let result = upscale(
            &Nearest,
            &image,
            |_, _| {
                tiles += 1;
                cancelled.store(true, Ordering::Relaxed);
            },
            &cancelled,
        );
        assert!(matches!(result, Err(Error::Cancelled)));
        assert_eq!(tiles, 1);
    }

    #[test]
    fn a_wrongly_sized_tile_is_an_error() {
        struct Broken;
        impl Engine for Broken {
            fn run(&self, _: &[f32]) -> Result<Vec<f32>> {
                Ok(vec![0.0; 3])
            }
        }
        let result = upscale(&Broken, &gradient(8, 8), |_, _| {}, &AtomicBool::new(false));
        assert!(matches!(result, Err(Error::Model(_))));
    }

    #[test]
    fn covers_end_no_taller_than_the_limit() {
        let cover = encoded(&gradient(100, 500));
        let up = upscale_cover(&Nearest, &cover, 1680, |_, _| {}, &AtomicBool::new(false)).unwrap();
        assert_eq!(up.before_size, (100, 500));
        assert_eq!(up.after_size, (336, 1680));
        let after = image::load_from_memory(&up.after_png).unwrap();
        assert_eq!((after.width(), after.height()), (336, 1680));
    }

    #[test]
    fn full_size_covers_are_refused() {
        let cover = encoded(&gradient(10, 40));
        let result = upscale_cover(&Nearest, &cover, 40, |_, _| {}, &AtomicBool::new(false));
        assert!(matches!(result, Err(Error::AlreadyFullSize)));
    }

    #[test]
    fn undecodable_covers_are_an_error() {
        let result = upscale_cover(&Nearest, b"nope", 40, |_, _| {}, &AtomicBool::new(false));
        assert!(matches!(result, Err(Error::BadImage(_))));
    }
}
