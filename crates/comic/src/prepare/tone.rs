//! Colour pages on a greyscale screen. Colour art separates shapes by hue
//! and is painted mostly in mid-tones, so plain luma turns it into a narrow,
//! washed-out band of grey. Each step below was chosen in Libra 2
//! comparisons of five colour Berserk pages.
use image::{GrayImage, RgbImage};

use super::raster::{autocontrast, luma};

/// How much colour difference is added to luminance, as a share of the
/// contrast luminance alone loses. 0.5 was invisible on the device and 1.0
/// darkened brown skin too far; 0.75 was chosen.
const COLOUR_CONTRAST: f32 = 0.75;

/// Colour-aware greys, then a contrast stretch, then darker mid-tones on
/// mostly light pages.
pub(super) fn colour_page_to_grey(rgb: &RgbImage) -> GrayImage {
    let mut gray = colour_aware(rgb, COLOUR_CONTRAST);
    autocontrast(&mut gray);
    darken_light_midtones(&mut gray);
    gray
}

/// A simplified form of Grundland and Dodgson's "Decolorize" (Pattern
/// Recognition, 2007). Pairs of nearby pixels, picked with Gaussian
/// distances, show which chroma direction carries the contrast that luma
/// loses. Chroma along that direction is added to luma, oriented to
/// reinforce existing light and dark differences, and the page's mean
/// brightness is kept.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    reason = "page dimensions and pixel counts are far below f32 and isize limits, and values are clamped before narrowing"
)]
fn colour_aware(rgb: &RgbImage, effect: f32) -> GrayImage {
    let (width, height) = (rgb.width() as usize, rgb.height() as usize);
    if width == 0 || height == 0 {
        return luma(rgb);
    }
    // Luma, red–green and yellow–blue, from 0–1 channels.
    let ypq: Vec<[f32; 3]> = rgb
        .pixels()
        .map(|p| {
            let [red, green, blue] = p.0.map(|c| f32::from(c) / 255.0);
            [
                0.299 * red + 0.587 * green + 0.114 * blue,
                red - green,
                f32::midpoint(red, green) - blue,
            ]
        })
        .collect();
    let colour_distance = |a: [u8; 3], b: [u8; 3]| {
        let sum: f32 = (0..3)
            .map(|i| ((f32::from(a[i]) - f32::from(b[i])) / 255.0).powi(2))
            .sum();
        (sum / 3.0).sqrt()
    };
    let sigma = ((2.0 / std::f32::consts::PI) * (2.0 * width.min(height) as f32).sqrt()).sqrt();
    // A fixed seed keeps output, and so update fingerprints, reproducible.
    let mut seed = 0x2545_F491_4F6C_DD1D_u64;
    let mut random = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 40) as f32 / (1_u64 << 24) as f32
    };
    let mut axis = [0.0_f32; 2];
    let mut lost = 0.0_f32;
    let mut chroma_differences = Vec::new();
    for y in (0..height).step_by(3) {
        for x in (0..width).step_by(3) {
            let (u1, u2) = (random().max(1e-9), random());
            let radius = sigma * (-2.0 * u1.ln()).sqrt();
            let angle = std::f32::consts::TAU * u2;
            let nx = x as isize + (radius * angle.cos()).round() as isize;
            let ny = y as isize + (radius * angle.sin()).round() as isize;
            if nx < 0 || ny < 0 || nx as usize >= width || ny as usize >= height {
                continue;
            }
            let (here, there) = (y * width + x, ny as usize * width + nx as usize);
            let (near, far) = (ypq[here], ypq[there]);
            let luma_difference = near[0] - far[0];
            let colour = colour_distance(
                rgb.as_raw()[here * 3..here * 3 + 3]
                    .try_into()
                    .unwrap_or([0; 3]),
                rgb.as_raw()[there * 3..there * 3 + 3]
                    .try_into()
                    .unwrap_or([0; 3]),
            );
            let loss = colour - luma_difference.abs();
            if loss <= 0.0 {
                continue;
            }
            let orientation = if luma_difference.abs() > 1e-4 {
                luma_difference.signum()
            } else {
                1.0
            };
            let (dp, dq) = (near[1] - far[1], near[2] - far[2]);
            axis[0] += orientation * loss * dp;
            axis[1] += orientation * loss * dq;
            lost += loss;
            chroma_differences.push((dp, dq));
        }
    }
    let norm = axis[0].hypot(axis[1]);
    if norm < 1e-6 {
        return luma(rgb);
    }
    let (p, q) = (axis[0] / norm, axis[1] / norm);
    let given: f32 = chroma_differences
        .iter()
        .map(|(dp, dq)| (p * dp + q * dq).abs())
        .sum();
    let scale = if given > 0.0 {
        effect * lost / given
    } else {
        0.0
    };
    let mut grey: Vec<f32> = ypq
        .iter()
        .map(|t| t[0] + scale * (p * t[1] + q * t[2]))
        .collect();
    let count = grey.len() as f32;
    let shift = ypq.iter().map(|t| t[0]).sum::<f32>() / count - grey.iter().sum::<f32>() / count;
    for value in &mut grey {
        *value = (*value + shift).clamp(0.0, 1.0);
    }
    GrayImage::from_raw(
        rgb.width(),
        rgb.height(),
        grey.into_iter()
            .map(|v| (v * 255.0).round() as u8)
            .collect(),
    )
    .unwrap_or_else(|| luma(rgb))
}

/// Darkening mid-tones strengthens the darker parts of a mostly light page
/// but muddies a dark one. No darkening at a mean of 0.45 or darker, rising
/// to gamma 1.4 at 0.65 or lighter.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "pixel counts are far below f32 limits, and values are clamped to 0–255 before narrowing"
)]
fn darken_light_midtones(gray: &mut GrayImage) {
    let values = gray.as_raw();
    if values.is_empty() {
        return;
    }
    let mean = values.iter().map(|&v| f64::from(v)).sum::<f64>() / values.len() as f64 / 255.0;
    let gamma = 1.0 + 0.4 * ((mean - 0.45) / 0.20).clamp(0.0, 1.0);
    if gamma <= 1.0 {
        return;
    }
    let table: Vec<u8> = (0..=255_u8)
        .map(|v| {
            ((f64::from(v) / 255.0).powf(gamma) * 255.0)
                .round()
                .clamp(0.0, 255.0) as u8
        })
        .collect();
    for pixel in gray.pixels_mut() {
        pixel.0[0] = table[usize::from(pixel.0[0])];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mean_where(gray: &GrayImage, keep: impl Fn(u32) -> bool) -> f64 {
        let (sum, n) = gray
            .enumerate_pixels()
            .filter(|(x, _, _)| keep(*x))
            .fold((0.0, 0.0), |(s, n), (_, _, p)| {
                (s + f64::from(p.0[0]), n + 1.0)
            });
        sum / n
    }

    /// Stripes of grey and red at the same luma, 8 pixels wide.
    fn equal_luma_stripes() -> RgbImage {
        RgbImage::from_fn(240, 240, |x, _| {
            image::Rgb(if (x / 8) % 2 == 0 {
                [127, 127, 127]
            } else {
                [220, 90, 75]
            })
        })
    }

    #[test]
    fn colours_luma_merges_are_told_apart() {
        let stripes = equal_luma_stripes();
        let red = |x: u32| (x / 8) % 2 == 1;
        let plain = luma(&stripes);
        let plain_gap = (mean_where(&plain, red) - mean_where(&plain, |x| !red(x))).abs();
        assert!(plain_gap < 2.0, "luma alone: {plain_gap}");
        let aware = colour_aware(&stripes, COLOUR_CONTRAST);
        let gap = (mean_where(&aware, red) - mean_where(&aware, |x| !red(x))).abs();
        assert!(
            gap > 17.0,
            "more than one of the Kobo's grey steps apart: {gap}"
        );
        // Mean brightness is kept.
        let mean = |g: &GrayImage| mean_where(g, |_| true);
        assert!((mean(&aware) - mean(&plain)).abs() < 2.0);
    }

    #[test]
    fn neutral_pages_are_unchanged_by_the_colour_term() {
        let grey = RgbImage::from_fn(120, 120, |x, y| {
            image::Rgb([u8::try_from((x + y) % 256).unwrap(); 3])
        });
        assert_eq!(colour_aware(&grey, COLOUR_CONTRAST), luma(&grey));
    }

    #[test]
    fn only_mostly_light_pages_get_darker_midtones() {
        // Mostly paper with a mid-grey band: mean well above 0.65.
        let mut light = GrayImage::from_fn(100, 100, |x, _| {
            image::Luma([if x < 20 { 128 } else { 240 }])
        });
        darken_light_midtones(&mut light);
        assert!(light.get_pixel(0, 0).0[0] < 100, "mid-grey darkened");
        // Mostly dark: left alone.
        let mut dark = GrayImage::from_fn(100, 100, |x, _| {
            image::Luma([if x < 20 { 128 } else { 40 }])
        });
        let before = dark.clone();
        darken_light_midtones(&mut dark);
        assert_eq!(dark, before);
    }
}
