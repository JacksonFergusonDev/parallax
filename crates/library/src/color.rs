//! The most prominent colour of a cover, for sorting a shelf by colour.

use std::path::Path;

use image::imageops::FilterType;

/// Covers shrink to this before counting; plenty for a dominant colour.
const SAMPLE: u32 = 48;
/// Bits kept per channel when grouping similar colours.
const BITS: u32 = 3;
/// How much a fully saturated pixel outweighs a grey one, so a red cover with
/// a white border reads as red.
const VIVID_WEIGHT: f64 = 6.0;

#[derive(Clone, Copy, Default)]
struct Bucket {
    weight: f64,
    sum: [f64; 3],
}

/// The dominant colour of pixels given as RGB triples, or `None` without any.
pub fn dominant_rgb(pixels: impl Iterator<Item = [u8; 3]>) -> Option<[u8; 3]> {
    let mut buckets = vec![Bucket::default(); 1 << (3 * BITS)];
    for [r, g, b] in pixels {
        let shift = 8 - BITS;
        let index = (usize::from(r >> shift) << (2 * BITS))
            | (usize::from(g >> shift) << BITS)
            | usize::from(b >> shift);
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let chroma = f64::from(max - min) / 255.0;
        let weight = 1.0 + VIVID_WEIGHT * chroma;
        let bucket = &mut buckets[index];
        bucket.weight += weight;
        for (sum, channel) in bucket.sum.iter_mut().zip([r, g, b]) {
            *sum += weight * f64::from(channel);
        }
    }
    let best = buckets
        .iter()
        .max_by(|a, b| a.weight.total_cmp(&b.weight))
        .filter(|b| b.weight > 0.0)?;
    // Means of u8 values, so they always fit.
    Some(best.sum.map(|sum| (sum / best.weight).round() as u8))
}

/// The dominant colour of an image file; `None` if it can't be read.
pub fn dominant_color(path: &Path) -> Option<[u8; 3]> {
    let image = image::open(path).ok()?;
    let small = image.resize(SAMPLE, SAMPLE, FilterType::Triangle).to_rgb8();
    dominant_rgb(small.pixels().map(|p| p.0))
}

/// [`dominant_color`] for many files, spread across the CPU's cores. Results
/// are in the order of `paths`.
pub fn dominant_colors<P: AsRef<Path> + Sync>(paths: &[P]) -> Vec<Option<[u8; 3]>> {
    let threads = std::thread::available_parallelism().map_or(1, usize::from);
    let chunk = paths.len().div_ceil(threads).max(1);
    std::thread::scope(|scope| {
        let workers: Vec<_> = paths
            .chunks(chunk)
            .map(|part| {
                scope.spawn(move || {
                    part.iter()
                        .map(|path| dominant_color(path.as_ref()))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        workers
            .into_iter()
            .flat_map(|worker| {
                worker
                    .join()
                    .unwrap_or_else(|e| std::panic::resume_unwind(e))
            })
            .collect()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: [u8; 3], b: [u8; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| x.abs_diff(y) <= 2)
    }

    #[test]
    fn solid_colour_is_itself() {
        let got = dominant_rgb(std::iter::repeat_n([180, 83, 42], 100)).unwrap();
        assert!(close(got, [180, 83, 42]), "{got:?}");
    }

    #[test]
    fn vivid_colour_beats_a_larger_white_border() {
        let white = std::iter::repeat_n([250, 250, 250], 70);
        let red = std::iter::repeat_n([200, 30, 30], 30);
        let got = dominant_rgb(white.chain(red)).unwrap();
        assert!(close(got, [200, 30, 30]), "{got:?}");
    }

    #[test]
    fn grey_cover_stays_grey() {
        let got = dominant_rgb(std::iter::repeat_n([120, 120, 120], 50)).unwrap();
        assert!(close(got, [120, 120, 120]), "{got:?}");
    }

    #[test]
    fn no_pixels_no_colour() {
        assert_eq!(dominant_rgb(std::iter::empty()), None);
    }

    #[test]
    fn reads_an_image_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("cover.png");
        image::RgbImage::from_pixel(60, 90, image::Rgb([30, 60, 200]))
            .save(&path)
            .unwrap();
        let got = dominant_color(&path).unwrap();
        assert!(close(got, [30, 60, 200]), "{got:?}");
    }

    #[test]
    fn many_files_keep_their_order() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = Vec::new();
        let mut expected = Vec::new();
        for i in 0..37u8 {
            let rgb = [i * 6, 200 - i * 3, 40];
            let path = dir.path().join(format!("{i}.png"));
            image::RgbImage::from_pixel(8, 8, image::Rgb(rgb))
                .save(&path)
                .unwrap();
            paths.push(path);
            expected.push(rgb);
        }
        paths.insert(5, dir.path().join("missing.png"));
        let got = dominant_colors(&paths);
        assert_eq!(got.len(), paths.len());
        assert_eq!(got[5], None);
        let found: Vec<_> = got.into_iter().flatten().collect();
        assert_eq!(found.len(), expected.len());
        for (a, b) in found.into_iter().zip(expected) {
            assert!(close(a, b), "{a:?} vs {b:?}");
        }
    }

    #[test]
    fn no_paths_no_colours() {
        assert!(dominant_colors::<&Path>(&[]).is_empty());
    }

    #[test]
    fn unreadable_file_has_no_colour() {
        assert_eq!(dominant_color(Path::new("/no/such/cover.jpg")), None);
    }
}
