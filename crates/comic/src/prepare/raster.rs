// Spread splitting and chroma detection adapted from Kindle Comic Converter
// v13.0.0, kindlecomicconverter/image.py (GPL-3.0-or-later).
// Copyright 2010 Alex Yatskov, 2011 Stanislav Kosolapov, 2016 Alberto Planas,
// 2012–2014 Ciro Mattia Gonano, 2013–2019 Pawel Jastrzebski.
// https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py
use fast_image_resize::{FilterType, ResizeAlg, ResizeOptions, Resizer, images::Image};
use image::imageops::{crop_imm, rotate270};
use image::{GrayImage, ImageEncoder, RgbImage, codecs::jpeg::JpegEncoder};

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
    // Colour pages stay colour on a colour screen. On a greyscale screen
    // they take the colour-page treatment below.
    let tone = match (is_colour(&rgb), profile.colour) {
        (true, true) => Tone::Colour,
        (true, false) => Tone::ColourOnGrey,
        (false, _) => Tone::Grey {
            stretch: options.autocontrast,
        },
    };
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
        .map(|(rgb, side)| encode(rgb, tone, profile, options, side))
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

/// How a page's tones are carried to the screen.
#[derive(Clone, Copy)]
enum Tone {
    /// A colour page on a colour screen, kept as JPEG.
    Colour,
    /// A colour page on a greyscale screen: see [`super::tone`].
    ColourOnGrey,
    /// A greyscale page, contrast stretched only when the setting asks.
    Grey { stretch: bool },
}

fn encode(
    rgb: RgbImage,
    tone: Tone,
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
    let colour = matches!(tone, Tone::Colour);
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
        let gray = match tone {
            Tone::ColourOnGrey => super::tone::colour_page_to_grey(&rgb, options),
            // Rec. 601 luma, as in the spike's Pillow reference, for neutral output.
            Tone::Grey { stretch } => {
                let mut gray = luma(&rgb);
                if stretch {
                    autocontrast(&mut gray);
                }
                gray
            }
            Tone::Colour => unreachable!("colour pages are encoded as JPEG above"),
        };
        drop(rgb);
        let gray = resized(
            w,
            h,
            gray.into_raw(),
            width,
            height,
            fast_image_resize::PixelType::U8,
        )?;
        let levels = if options.dither {
            dithered_levels(&gray, width as usize)
        } else {
            gray.iter()
                .map(|&value| LEVELS[usize::from(value)])
                .collect()
        };
        png_4bit(width, height, &levels).map_err(|e| e.to_string())?
    };
    Ok(Raster {
        width,
        height,
        bytes,
        jpeg: colour,
        side,
    })
}

/// A cover sized to fit the screen, never enlarged, and on a greyscale
/// screen turned to grey the way colour pages are. Nickel builds its library
/// thumbnails and sleep-screen cover from this image with plain luma, so
/// colour art gets the same washed-out band of grey as an untreated colour
/// page. The output is 8-bit, not 16 dithered levels: Nickel rescales the
/// cover for each of those uses, and rescaling dither turns it into noise.
///
/// PNG stays PNG and anything else becomes JPEG. `None` means the cover
/// already fits and needs no grey treatment, so it is best left as it is.
pub(super) fn fit_cover(
    bytes: &[u8],
    profile: Profile,
    options: Options,
) -> Result<Option<Vec<u8>>, String> {
    let rgb = decode(bytes)?;
    let (w, h) = rgb.dimensions();
    let scale = (f64::from(profile.width) / f64::from(w))
        .min(f64::from(profile.height) / f64::from(h))
        .min(1.0);
    let colour = is_colour(&rgb);
    if scale >= 1.0 && (profile.colour || !colour) {
        return Ok(None);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let fit = |n: u32| (f64::from(n) * scale).round().max(1.0) as u32;
    let (width, height) = (fit(w), fit(h));
    // Resized before the grey treatment: covers are often several times the
    // screen's size, and the treatment's work grows with the pixel count.
    let rgb = resized(
        w,
        h,
        rgb.into_raw(),
        width,
        height,
        fast_image_resize::PixelType::U8x3,
    )?;
    let rgb = RgbImage::from_raw(width, height, rgb).expect("three bytes per pixel");
    let (pixels, layout) = if profile.colour {
        (rgb.into_raw(), image::ExtendedColorType::Rgb8)
    } else if colour {
        (
            super::tone::colour_page_to_grey(&rgb, options).into_raw(),
            image::ExtendedColorType::L8,
        )
    } else {
        (luma(&rgb).into_raw(), image::ExtendedColorType::L8)
    };
    let mut out = Vec::new();
    if image::guess_format(bytes).ok() == Some(image::ImageFormat::Png) {
        image::codecs::png::PngEncoder::new(&mut out)
            .write_image(&pixels, width, height, layout)
            .map_err(|e| e.to_string())?;
    } else {
        JpegEncoder::new_with_quality(&mut out, 90)
            .encode(&pixels, width, height, layout)
            .map_err(|e| e.to_string())?;
    }
    Ok(Some(out))
}

/// Rec. 601 luma. Written over the raw bytes, without per-pixel
/// coordinates, so it compiles to vector code (NEON on Apple silicon).
pub(super) fn luma(rgb: &RgbImage) -> GrayImage {
    let (w, h) = rgb.dimensions();
    let values = rgb
        .as_raw()
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[red, green, blue]| {
            let sum = 299 * u32::from(red) + 587 * u32::from(green) + 114 * u32::from(blue);
            // At most 255 for any input, so the narrowing never truncates.
            #[allow(clippy::cast_possible_truncation)]
            let value = ((sum + 500) / 1000) as u8;
            value
        })
        .collect();
    GrayImage::from_raw(w, h, values).expect("one grey byte per pixel")
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

/// The nearest of the 16 even greys (0, 17, …, 255) the Kobo shows, for
/// each 8-bit grey.
const LEVELS: [u8; 256] = {
    let mut table = [0; 256];
    let mut value = 0;
    while value < 256 {
        // At most 15, so the narrowing never truncates.
        #[allow(clippy::cast_possible_truncation)]
        let level = ((value + 8) / 17) as u8;
        table[value] = level;
        value += 1;
    }
    table
};

/// The nearest level to a grey that error diffusion may have pushed
/// outside 0–255.
fn nearest_level(value: i32) -> u8 {
    // Clamped to 0–255 first, so the conversion can't fail.
    LEVELS[usize::try_from(value.clamp(0, 255)).unwrap_or_default()]
}

/// Floyd–Steinberg error diffusion to 16 levels, on the stored grey values
/// as Pillow (and so KCC) does, scanning rows alternately left and right
/// so the error doesn't drift into diagonal streaks. Integer arithmetic
/// throughout: errors are carried in sixteenths of a grey value, which is
/// exact for the 7, 3, 5 and 1 sixteenths the method spreads.
fn dithered_levels(gray: &[u8], width: usize) -> Vec<u8> {
    let mut levels = vec![0; gray.len()];
    if width == 0 {
        return levels;
    }
    // Error carried into this row and the next, padded by one on each side.
    let mut this_row = vec![0_i32; width + 2];
    let mut next_row = vec![0_i32; width + 2];
    for (y, (row, out)) in gray
        .chunks_exact(width)
        .zip(levels.chunks_exact_mut(width))
        .enumerate()
    {
        let mut step = |x: usize, ahead: usize, behind: usize| {
            // Rounded to the nearest whole grey; >> floors negative values too.
            let wanted = i32::from(row[x]) + ((this_row[x + 1] + 8) >> 4);
            let level = nearest_level(wanted);
            out[x] = level;
            let error = wanted - i32::from(level) * 17;
            this_row[ahead] += error * 7;
            next_row[behind] += error * 3;
            next_row[x + 1] += error * 5;
            next_row[ahead] += error;
        };
        // Padded indices: x + 2 is the next pixel to the right, x the left.
        if y % 2 == 0 {
            for x in 0..width {
                step(x, x + 2, x);
            }
        } else {
            for x in (0..width).rev() {
                step(x, x, x + 2);
            }
        }
        std::mem::swap(&mut this_row, &mut next_row);
        next_row.fill(0);
    }
    levels
}

/// Packs 4-bit levels two to a byte as a greyscale PNG.
fn png_4bit(width: u32, height: u32, levels: &[u8]) -> Result<Vec<u8>, png::EncodingError> {
    let row_bytes = width.div_ceil(2) as usize;
    let mut packed = vec![0; row_bytes * height as usize];
    for (row, out) in levels
        .chunks_exact(width as usize)
        .zip(packed.chunks_exact_mut(row_bytes))
    {
        // Pairs of pixels become one byte, high nibble first.
        let (pairs, odd) = row.as_chunks::<2>();
        for (byte, &[first, second]) in out.iter_mut().zip(pairs) {
            *byte = first << 4 | second;
        }
        if let (Some(&last), Some(byte)) = (odd.first(), out.last_mut()) {
            *byte = last << 4;
        }
    }
    let mut bytes = Vec::new();
    let mut encoder = png::Encoder::new(&mut bytes, width, height);
    encoder.set_color(png::ColorType::Grayscale);
    encoder.set_depth(png::BitDepth::Four);
    // Measured on Berserk v01: PNG's row filters work on packed bytes and
    // hurt 4-bit pages, so no filter at Balanced was 24% smaller than Fast
    // with filters. High saved only 2% more at four times the encode time.
    encoder.set_compression(png::Compression::Balanced);
    encoder.set_filter(png::Filter::NoFilter);
    let mut writer = encoder.write_header()?;
    writer.write_image_data(&packed)?;
    writer.finish()?;
    Ok(bytes)
}

/// KCC's chroma histograms. Keep solid colours while still treating neutral
/// pages as greyscale.
///
/// Checks every second pixel of every second row: the decision rests on the
/// share of pixels in each chroma histogram tail, which a regular sample
/// keeps. On 1,403 Berserk pages this decided every page as the full check
/// did, for a quarter of the work.
fn is_colour(rgb: &RgbImage) -> bool {
    const STEP: usize = 2;
    let mut cb = [0_u32; 256];
    let mut cr = [0_u32; 256];
    let mut count = 0_u64;
    let width = rgb.width() as usize;
    if width == 0 {
        return false;
    }
    for row in rgb.as_raw().chunks_exact(width * 3).step_by(STEP) {
        for pixel in row.as_chunks::<3>().0.iter().step_by(STEP) {
            count += 1;
            let [red, green, blue] = pixel.map(i32::from);
            let u = (128 + ((-43 * red - 85 * green + 128 * blue + 128) >> 8)).clamp(0, 255);
            let v = (128 + ((128 * red - 107 * green - 21 * blue + 128) >> 8)).clamp(0, 255);
            cb[usize::try_from(u).unwrap_or_default()] += 1;
            cr[usize::try_from(v).unwrap_or_default()] += 1;
        }
    }
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
pub(super) fn autocontrast(gray: &mut GrayImage) {
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
    // Whole-row scans over the raw bytes, which vectorise.
    let content = |channel: &u8| *channel < 240;
    for (y, row) in (0_u32..).zip(rgb.as_raw().chunks_exact(w as usize * 3)) {
        if let (Some(first), Some(last)) =
            (row.iter().position(content), row.iter().rposition(content))
        {
            top = top.min(y);
            bottom = y + 1;
            // Byte offsets to pixel columns; widths fit in u32.
            #[allow(clippy::cast_possible_truncation)]
            let (first, last) = ((first / 3) as u32, (last / 3) as u32);
            left = left.min(first);
            right = right.max(last + 1);
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

    fn png_bytes(image: &RgbImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        image
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn level_range(raster: &Raster) -> (u8, u8) {
        let page = image::load_from_memory(&raster.bytes).unwrap().into_luma8();
        let values = page.as_raw();
        (*values.iter().min().unwrap(), *values.iter().max().unwrap())
    }

    fn screen(colour: bool) -> Profile {
        Profile {
            width: 300,
            height: 400,
            colour,
            landscape_spreads: false,
        }
    }

    fn jpeg_bytes(image: &RgbImage) -> Vec<u8> {
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 90)
            .encode_image(image)
            .unwrap();
        bytes
    }

    fn rainbow(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            image::Rgb([
                u8::try_from(x * 255 / width).unwrap(),
                u8::try_from(y * 255 / height).unwrap(),
                160,
            ])
        })
    }

    #[test]
    fn large_colour_covers_fit_a_greyscale_screen_in_grey() {
        let cover = jpeg_bytes(&rainbow(900, 1500));
        let fitted = fit_cover(&cover, screen(false), Options::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            image::guess_format(&fitted).unwrap(),
            image::ImageFormat::Jpeg
        );
        let fitted = image::load_from_memory(&fitted).unwrap();
        // Limited by height: 900 × 400/1500 = 240.
        assert_eq!((fitted.width(), fitted.height()), (240, 400));
        assert_eq!(fitted.color(), image::ColorType::L8);
    }

    #[test]
    fn covers_on_a_colour_screen_are_only_resized() {
        let large = png_bytes(&rainbow(600, 800));
        let fitted = fit_cover(&large, screen(true), Options::default())
            .unwrap()
            .unwrap();
        assert_eq!(
            image::guess_format(&fitted).unwrap(),
            image::ImageFormat::Png
        );
        let fitted = image::load_from_memory(&fitted).unwrap();
        assert_eq!((fitted.width(), fitted.height()), (300, 400));
        assert_eq!(fitted.color(), image::ColorType::Rgb8);
        // A colour cover that already fits is left as it is.
        let small = png_bytes(&rainbow(200, 300));
        assert!(
            fit_cover(&small, screen(true), Options::default())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn small_covers_are_left_alone_unless_colour_on_grey() {
        let grey = png_bytes(&RgbImage::from_pixel(200, 300, image::Rgb([90, 90, 90])));
        assert!(
            fit_cover(&grey, screen(false), Options::default())
                .unwrap()
                .is_none()
        );
        let colour = png_bytes(&rainbow(200, 300));
        let fitted = fit_cover(&colour, screen(false), Options::default())
            .unwrap()
            .unwrap();
        let fitted = image::load_from_memory(&fitted).unwrap();
        assert_eq!((fitted.width(), fitted.height()), (200, 300));
        assert_eq!(fitted.color(), image::ColorType::L8);
        assert!(fit_cover(b"not an image", screen(false), Options::default()).is_err());
    }

    #[test]
    fn colour_pages_on_a_greyscale_screen_get_contrast() {
        let profile = Profile {
            width: 200,
            height: 300,
            colour: false,
            landscape_spreads: false,
        };
        // Washed-out pastels: soft pink fading into pale blue.
        let pastel = RgbImage::from_fn(200, 300, |_, y| {
            let t = u8::try_from(y * 120 / 300).unwrap();
            image::Rgb([230 - t, 200 - t / 3, 195 + t / 2])
        });
        let pages = process(&png_bytes(&pastel), profile, Options::default()).unwrap();
        assert_eq!(level_range(&pages[0]), (0, 255), "stretched to full range");

        // The same tones in grey are left alone unless the setting asks.
        let grey = RgbImage::from_fn(200, 300, |_, y| {
            let v = u8::try_from(170 + y * 50 / 300).unwrap();
            image::Rgb([v; 3])
        });
        let pages = process(&png_bytes(&grey), profile, Options::default()).unwrap();
        let (lo, hi) = level_range(&pages[0]);
        assert!(lo >= 153 && hi <= 238, "unchanged: {lo}–{hi}");
        let stretched = Options {
            autocontrast: true,
            ..Options::default()
        };
        let pages = process(&png_bytes(&grey), profile, stretched).unwrap();
        assert_eq!(level_range(&pages[0]), (0, 255));
    }

    #[test]
    fn rounding_picks_the_nearest_of_16_greys() {
        assert_eq!(nearest_level(0), 0);
        assert_eq!(nearest_level(8), 0);
        assert_eq!(nearest_level(9), 1);
        assert_eq!(nearest_level(255), 15);
        assert_eq!(nearest_level(300), 15);
        assert_eq!(nearest_level(-40), 0);
        for level in 0..16_u8 {
            assert_eq!(nearest_level(i32::from(level) * 17), level);
        }
    }

    #[test]
    fn dithering_keeps_the_average_tone_between_levels() {
        // A flat grey of 8 sits halfway between level 0 (0) and level 1 (17).
        let (width, height) = (64, 64);
        let gray = vec![8_u8; width * height];
        let plain: Vec<u8> = gray.iter().map(|&v| LEVELS[usize::from(v)]).collect();
        assert!(plain.iter().all(|&l| l == 0), "rounding loses the tone");
        let dithered = dithered_levels(&gray, width);
        let mean =
            dithered.iter().map(|&l| f64::from(l) * 17.0).sum::<f64>() / dithered.len() as f64;
        assert!((mean - 8.0).abs() < 0.5, "mean {mean}");
        // Exact levels pass through untouched.
        let exact = vec![34_u8; width * height];
        assert!(dithered_levels(&exact, width).iter().all(|&l| l == 2));
    }

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
