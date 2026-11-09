use std::io::Cursor;
use std::path::Path;

use image::imageops::FilterType;
use parallax_comic::{Cbz, parse_file_name};
use parallax_library::{Book, Kind};
use parallax_metadata::comics::{self, Catalog, Match, MatchError, Query};
use parallax_metadata::{Fetch, FetchError};

pub(super) fn lookup(
    fetch: &dyn Fetch,
    book: &Book,
    file: Option<&Path>,
    metron_token: Option<&str>,
    selected_issue: Option<u64>,
) -> Result<Option<Match>, MatchError> {
    let name = parse_file_name(&book.original_name);
    let mut query = Query {
        series: book.series.as_deref().unwrap_or(&name.series),
        authors: &book.authors,
        isbn: book.isbn(),
        publisher: book.publisher.as_deref(),
        ..Query::default()
    };
    if book.kind == Kind::Manga {
        query.mangainfo_url = identifier(book, Catalog::Mangainfo.scheme());
        // ComicInfo's Volume or an explicit filename volume is useful.
        // A bare series index might instead be a chapter number.
        let archive = file.and_then(|path| Cbz::open(path).ok());
        query.volume = archive
            .as_ref()
            .and_then(Cbz::comic_info)
            .and_then(parallax_comic::ComicInfo::volume)
            .or(name.volume.as_deref());
        return Ok(comics::mangainfo(fetch, &query)?);
    }
    let Some(token) = metron_token else {
        return Err(FetchError::Http(
            "add a Metron API token in Settings → Comics to look up issues".into(),
        )
        .into());
    };
    let archive = file.and_then(|path| Cbz::open(path).ok());
    let info = archive.as_ref().and_then(Cbz::comic_info);
    let stored_index = book.series_index.as_deref();
    let embedded_volume = info.and_then(parallax_comic::ComicInfo::volume);
    // Older imports put ComicInfo's Volume in the library index. For Metron it
    // identifies the run, while Number is the issue within that run.
    let run = embedded_volume.or(name.volume.as_deref());
    query.run = run.filter(|v| v.parse::<u32>().is_ok_and(|number| number > 0));
    query.issue = stored_index
        .filter(|index| Some(*index) != run)
        .or(info.and_then(parallax_comic::ComicInfo::number))
        .or(name.number.as_deref());
    query.year = book.published.as_deref().and_then(|date| date.get(..4));
    let web = info.and_then(|info| info.get("Web")).unwrap_or_default();
    query.metron_id = selected_issue
        .or_else(|| identifier(book, "metron").and_then(|id| id.parse().ok()))
        .or_else(|| number_after(web, "https://metron.cloud/api/issue/"));
    query.comic_vine_id = identifier(book, "comicvine")
        .and_then(|id| id.parse().ok())
        .or_else(|| number_after(web, "https://comicvine.gamespot.com/"))
        .or_else(|| number_after(web, "https://comicvine.com/"));
    let barcode = info
        .and_then(|info| info.get("GTIN"))
        .or_else(|| book.isbn());
    query.upc = barcode
        .filter(|code| matches!(code.len(), 12 | 17) && code.bytes().all(|c| c.is_ascii_digit()));

    comics::metron(fetch, token, &query, || {
        let mut cbz = Cbz::open(file?).ok()?;
        let bytes = cbz.read_page(cbz.cover_index()).ok()?;
        cover_hash(&bytes)
    })
}

fn identifier<'a>(book: &'a Book, scheme: &str) -> Option<&'a str> {
    book.identifiers
        .iter()
        .find(|id| id.scheme.as_deref() == Some(scheme))
        .map(|id| id.value.as_str())
}

fn number_after(web: &str, prefix: &str) -> Option<u64> {
    web.split_whitespace().find_map(|link| {
        let path = link.strip_prefix(prefix)?;
        if prefix.contains("comicvine") {
            return path
                .split('/')
                .find_map(|part| part.strip_prefix("4000-")?.parse().ok());
        }
        path.split('/').next()?.parse().ok()
    })
}

/// Metron uses ImageHash's 64-bit pHash. Compare the same low-frequency DCT
/// coefficients after resampling, allowing small differences between decoders.
fn cover_hash(bytes: &[u8]) -> Option<u64> {
    parallax_comic::shape::image_dimensions(bytes)?;
    let gray = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?
        .into_luma8();
    let size = 32_u32;
    let sample = image::imageops::resize(&gray, size, size, FilterType::Lanczos3);
    let basis: Vec<Vec<_>> = (0..8)
        .map(|frequency| {
            (0..size)
                .map(|x| {
                    (std::f64::consts::PI * f64::from(frequency) * (f64::from(x) + 0.5)
                        / f64::from(size))
                    .cos()
                })
                .collect()
        })
        .collect();
    let mut coefficients = [0.0; 64];
    for (k, vertical) in basis.iter().enumerate() {
        for (l, horizontal) in basis.iter().enumerate() {
            coefficients[k * 8 + l] = sample
                .rows()
                .zip(vertical)
                .map(|(row, y)| {
                    row.zip(horizontal)
                        .map(|(pixel, x)| f64::from(pixel.0[0]) * x)
                        .sum::<f64>()
                        * y
                })
                .sum::<f64>();
        }
    }
    let mut sorted = coefficients;
    sorted.sort_by(f64::total_cmp);
    let median = f64::midpoint(sorted[31], sorted[32]);
    Some(
        coefficients
            .iter()
            .fold(0, |hash, &value| (hash << 1) | u64::from(value > median)),
    )
}
