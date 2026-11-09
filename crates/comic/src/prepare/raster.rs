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
    cover: bool,
) -> Result<(Vec<Raster>, Option<Raster>), String> {
    let decoded = image::load_from_memory(bytes).map_err(|e| e.to_string())?;
    let mut rgb = if decoded.color().has_alpha() {
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
    };
    let colour = profile.colour && is_colour(&rgb);
    let cover = cover
        .then(|| encode(rgb.clone(), colour, profile, options, None))
        .transpose()?;
    if options.crop {
        rgb = crop_margins(rgb);
    }
    let (w, h) = rgb.dimensions();
    let parts = match classify(w, h) {
        PageShape::Wide => vec![(rotate270(&rgb), Some(Side::Center))],
        PageShape::Spread if options.spreads != Spreads::KeepWhole => {
            let half = w / 2;
            let mut halves = [
                (crop_imm(&rgb, 0, 0, half, h).to_image(), Some(Side::Left)),
                (
                    crop_imm(&rgb, half, 0, w - half, h).to_image(),
                    Some(Side::Right),
                ),
            ];
            if options.rtl {
                halves.reverse();
            }
            if options.spreads == Spreads::WholeThenHalves {
                let mut pages = vec![(rgb, Some(Side::Center))];
                pages.extend(halves);
                pages
            } else {
                Vec::from(halves)
            }
        }
        _ => vec![(rgb, None)],
    };
    let pages = parts
        .into_iter()
        .map(|(rgb, side)| encode(rgb, colour, profile, options, side))
        .collect::<Result<_, _>>()?;
    Ok((pages, cover))
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

fn autocontrast(gray: &mut GrayImage) {
    let (min, max) = gray.as_raw().iter().fold((255, 0), |(min, max), &value| {
        (min.min(value), max.max(value))
    });
    let range = u16::from(max - min);
    if range < 159 {
        return;
    }
    for pixel in gray.pixels_mut() {
        pixel.0[0] = u8::try_from(u16::from(pixel.0[0] - min) * 255 / range).unwrap_or(255);
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
