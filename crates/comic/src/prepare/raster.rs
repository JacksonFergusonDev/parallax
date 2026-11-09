// Spread splitting and chroma detection adapted from Kindle Comic Converter
// v13.0.0, kindlecomicconverter/image.py (GPL-3.0-or-later).
// Copyright 2010 Alex Yatskov, 2011 Stanislav Kosolapov, 2016 Alberto Planas,
// 2012–2014 Ciro Mattia Gonano, 2013–2019 Pawel Jastrzebski.
// https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use image::imageops::{crop_imm, rotate270};
use image::{GrayImage, RgbImage, codecs::jpeg::JpegEncoder};

use super::{Options, Profile, Spreads};
use crate::shape::{PageShape, classify};

#[derive(Clone, Copy)]
pub(super) enum Side {
    Left,
    Right,
    Center,
}

pub(super) struct Raster {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
    pub jpeg: bool,
    pub side: Option<Side>,
}

impl Raster {
    pub fn extension(&self) -> &'static str {
        if self.jpeg { "jpg" } else { "png" }
    }

    pub fn mime(&self) -> &'static str {
        if self.jpeg { "image/jpeg" } else { "image/png" }
    }
}

pub(super) fn process(
    bytes: &[u8],
    profile: Profile,
    options: Options,
) -> Result<Vec<Raster>, String> {
    let rgb = decode(bytes)?;
    let colour = profile.colour && is_colour(&rgb);
    // Shape and split come from the page as drawn. Margins are trimmed from
    // each resulting part, so uneven margins can't move the split off the
    // spread's gutter.
    let trim = |rgb: RgbImage| if options.crop { crop_margins(rgb) } else { rgb };
    let (w, h) = rgb.dimensions();
    let parts = match classify(w, h) {
        PageShape::Wide => vec![(rotate270(&trim(rgb)), Some(Side::Center))],
        PageShape::Spread if options.spreads != Spreads::KeepWhole => {
            let half = w / 2;
            let mut halves = [
                (
                    trim(crop_imm(&rgb, 0, 0, half, h).to_image()),
                    Some(Side::Left),
                ),
                (
                    trim(crop_imm(&rgb, half, 0, w - half, h).to_image()),
                    Some(Side::Right),
                ),
            ];
            if options.rtl {
                halves.reverse();
            }
            if options.spreads == Spreads::WholeThenHalves {
                let mut pages = vec![(trim(rgb), Some(Side::Center))];
                pages.extend(halves);
                pages
            } else {
                Vec::from(halves)
            }
        }
        _ => vec![(trim(rgb), None)],
    };
    parts
        .into_iter()
        .map(|(rgb, side)| encode(rgb, colour, profile, options, side))
        .collect()
}

fn decode(bytes: &[u8]) -> Result<RgbImage, String> {
    let decoded = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    Ok(if decoded.color().has_alpha() {
        let rgba = decoded.into_rgba8();
        RgbImage::from_fn(rgba.width(), rgba.height(), |x, y| {
            let [r, g, b, a] = rgba.get_pixel(x, y).0;
            let alpha = u16::from(a);
            image::Rgb([r, g, b].map(|channel| {
                let value = u16::from(channel) * alpha + 255 * (255 - alpha);
                u8::try_from(value.div_ceil(255)).unwrap_or(255)
            }))
        })
    } else {
        decoded.into_rgb8()
    })
}

fn encode(
    rgb: RgbImage,
    colour: bool,
    profile: Profile,
    options: Options,
    side: Option<Side>,
) -> Result<Raster, String> {
    let (w, h) = rgb.dimensions();
    let scale =
        (f64::from(profile.width) / f64::from(w)).min(f64::from(profile.height) / f64::from(h));
    let scale = if options.upscale {
        scale
    } else {
        scale.min(1.0)
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fit = |n: u32| (f64::from(n) * scale).round().max(1.0) as u32;
    let (width, height) = (fit(w), fit(h));
    let bytes = if colour {
        let rgb = resized(
            w,
            h,
            rgb.into_raw(),
            width,
            height,
            fast_image_resize::PixelType::U8x3,
        )?;
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 85)
            .encode(&rgb, width, height, image::ExtendedColorType::Rgb8)
            .map_err(|e| e.to_string())?;
        bytes
    } else {
        // Rec. 601 luma, as in the spike's Pillow reference, for neutral output.
        let mut gray = luma(&rgb);
        drop(rgb);
        if options.autocontrast {
            autocontrast(&mut gray);
        }
        let gray = resized(
            w,
            h,
            gray.into_raw(),
            width,
            height,
            fast_image_resize::PixelType::U8,
        )?;
        png_4bit(width, height, &gray).map_err(|e| e.to_string())?
    };
    Ok(Raster {
        width,
        height,
        bytes,
        jpeg: colour,
        side,
    })
}

fn luma(rgb: &RgbImage) -> GrayImage {
    let (w, h) = rgb.dimensions();
    GrayImage::from_fn(w, h, |x, y| {
        let [red, green, blue] = rgb.get_pixel(x, y).0.map(u32::from);
        image::Luma([
            u8::try_from((299 * red + 587 * green + 114 * blue + 500) / 1000).unwrap_or(255),
        ])
    })
}

fn resized(
    w: u32,
    h: u32,
    pixels: Vec<u8>,
    width: u32,
    height: u32,
    pixel: fast_image_resize::PixelType,
) -> Result<Vec<u8>, String> {
    if (w, h) == (width, height) {
        return Ok(pixels);
    }
    let src = Image::from_vec_u8(w, h, pixels, pixel).map_err(|e| e.to_string())?;
    let mut dst = Image::new(width, height, pixel);
    Resizer::new()
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
        )
        .map_err(|e| e.to_string())?;
    Ok(dst.into_vec())
}

fn png_4bit(width: u32, height: u32, gray: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let row_bytes = width.div_ceil(2) as usize;
    let mut packed = vec![0; row_bytes * height as usize];
    for (y, row) in gray.chunks_exact(width as usize).enumerate() {
        for (x, pixel) in row.iter().enumerate() {
            let level = u8::try_from((u16::from(*pixel) + 8) / 17).unwrap_or(15);
            packed[y * row_bytes + x / 2] |= if x % 2 == 0 { level << 4 } else { level };
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Four);
    encoder.set_compression(png::Compression::Fast);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&packed)?;
    writer.finish()?;
    Ok(bytes)
}

/// KCC's chroma histograms. Keep solid colours while still treating neutral
/// pages on a colour device as greyscale.
fn is_colour(rgb: &RgbImage) -> bool {
    let mut cb = [0_u32; 256];
    let mut cr = [0_u32; 256];
    for pixel in rgb.pixels() {
        let [red, green, blue] = pixel.0.map(i32::from);
        let u = (128 + (-43 * red - 85 * green + 128 * blue + 128).div_euclid(256)).clamp(0, 255);
        let v = (128 + (128 * red - 107 * green - 21 * blue + 128).div_euclid(256)).clamp(0, 255);
        cb[usize::try_from(u).unwrap_or_default()] += 1;
        cr[usize::try_from(v).unwrap_or_default()] += 1;
    }
    let count = u64::from(rgb.width()) * u64::from(rgb.height());
    for (trim, threshold) in [(0, 22), (2, 10), (30, 4)] {
        let tail = count * trim / 1_000;
        let bounds = |bins: &[u32; 256]| {
            let mut running = 0;
            let lo = bins
                .iter()
                .position(|&n| {
                    running += u64::from(n);
                    running > tail
                })
                .unwrap_or(128);
            let mut running = 0;
            let hi = bins
                .iter()
                .rposition(|&n| {
                    running += u64::from(n);
                    running > tail
                })
                .unwrap_or(128);
            (lo, hi)
        };
        let chroma = [bounds(&cb), bounds(&cr)];
        if chroma.iter().any(|&(lo, hi)| lo > 128 || hi < 128) {
            return true;
        }
        if chroma.iter().all(|&(lo, hi)| hi.saturating_sub(lo) < 7) {
            return false;
        }
        if chroma
            .iter()
            .any(|&(lo, hi)| lo <= 128 - threshold || hi >= 128 + threshold)
        {
            return true;
        }
    }
    false
}

/// Stretches the greys between the darkest and lightest 0.5% of pixels to
/// the full range, like Pillow's `autocontrast(cutoff=0.5)`, so a few specks
/// of black or white don't stop faded scans from gaining contrast. Nearly
/// uniform pages (blank or a flat tint) are left alone rather than turning
/// their noise into strong patterns.
fn autocontrast(gray: &mut GrayImage) {
    let mut histogram = [0_u64; 256];
    for &value in gray.as_raw() {
        histogram[usize::from(value)] += 1;
    }
    let tail = gray.as_raw().len() as u64 * 5 / 1_000;
    let mut running = 0;
    let lo = histogram
        .iter()
        .position(|&n| {
            running += n;
            running > tail
        })
        .unwrap_or(0);
    let mut running = 0;
    let hi = histogram
        .iter()
        .rposition(|&n| {
            running += n;
            running > tail
        })
        .unwrap_or(255);
    if hi < lo + 32 || (lo, hi) == (0, 255) {
        return;
    }
    // Both are histogram positions, so below 256.
    let lo = u32::try_from(lo).unwrap_or(0);
    let range = u32::try_from(hi).unwrap_or(255) - lo;
    let table: Vec<u8> = (0..=255_u32)
        .map(|v| u8::try_from(v.saturating_sub(lo) * 255 / range).unwrap_or(255))
        .collect();
    for pixel in gray.pixels_mut() {
        pixel.0[0] = table[usize::from(pixel.0[0])];
    }
}

/// Optional conservative crop for white margins. Content near an edge
/// prevents cropping that edge, and any crop is capped at 10% per side.
fn crop_margins(rgb: RgbImage) -> RgbImage {
    let (w, h) = rgb.dimensions();
    let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
    for (x, y, p) in rgb.enumerate_pixels() {
        if p.0.iter().any(|&channel| channel < 240) {
            left = left.min(x);
            top = top.min(y);
            right = right.max(x + 1);
            bottom = bottom.max(y + 1);
        }
    }
    if left >= right || top >= bottom {
        return rgb;
    }
    let (x, y) = (left.min(w / 10), top.min(h / 10));
    let (right, bottom) = (right.max(w - w / 10), bottom.max(h - h / 10));
    if (x, y, right, bottom) == (0, 0, w, h) {
        rgb
    } else {
        crop_imm(&rgb, x, y, right - x, bottom - y).to_image()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autocontrast_ignores_specks_and_flat_pages() {
        // A faded page (greys 80–180) with a few pure black and white specks.
        let mut faded = GrayImage::from_fn(100, 100, |x, _| {
            image::Luma([u8::try_from(80 + x).unwrap()])
        });
        faded.put_pixel(0, 0, image::Luma([0]));
        faded.put_pixel(1, 0, image::Luma([255]));
        autocontrast(&mut faded);
        let values = faded.as_raw();
        assert!(values.iter().filter(|&&v| v < 10).count() > 50);
        assert!(values.iter().filter(|&&v| v > 245).count() > 50);

        let mut flat = GrayImage::from_fn(100, 100, |x, _| {
            image::Luma([u8::try_from(120 + x % 10).unwrap()])
        });
        let before = flat.clone();
        autocontrast(&mut flat);
        assert_eq!(flat, before);
    }

    #[test]
    fn spreads_split_at_the_gutter_before_margins_are_trimmed() {
        // A 300 × 200 spread: a wide white margin on the left only, with
        // dark art up to the gutter on both sides.
        let spread = RgbImage::from_fn(300, 200, |x, _| {
            image::Rgb(if x < 30 { [255; 3] } else { [30; 3] })
        });
        let mut bytes = Vec::new();
        spread
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        let profile = Profile {
            width: 1000,
            height: 1000,
            colour: false,
            landscape_spreads: false,
        };
        let options = Options {
            crop: true,
            ..Options::default()
        };
        let pages = process(&bytes, profile, options).unwrap();
        // Each half is 150 px wide. The left loses at most 10% of that to its
        // margin; the right has none and keeps it all.
        let sizes: Vec<_> = pages.iter().map(|p| (p.width, p.height)).collect();
        assert_eq!(sizes, [(135, 200), (150, 200)]);
    }
}
