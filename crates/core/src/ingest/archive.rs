//! Stream book bundles through the ZIP and native libarchive readers used for comics.

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;

use compress_tools::{ArchiveContents, ArchiveIterator, ArchiveIteratorBuilder};
use parallax_comic::Format;

const MAX_ENTRIES: usize = 10_000;
const MAX_BOOK_BYTES: u64 = 1024 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 8 * MAX_BOOK_BYTES;

pub fn is_book_archive(path: &Path) -> bool {
    matches!(extension(path).as_str(), "zip" | "rar" | "7z")
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn book_name(name: &str) -> Option<&str> {
    let parts = || name.split(['/', '\\']);
    if name.starts_with(['/', '\\'])
        || name.contains(':')
        || name.chars().any(char::is_control)
        || parts().any(|part| {
            (part.starts_with('.') && part != ".") || part.eq_ignore_ascii_case("__MACOSX")
        })
    {
        return None;
    }
    let base = parts().next_back()?;
    let ext = extension(Path::new(base));
    (base.len() <= 255
        && (matches!(ext.as_str(), "epub" | "kepub" | "cbz" | "cbr" | "cb7")
            || parallax_convert::amazon::EXTENSIONS.contains(&ext.as_str())))
    .then_some(base)
}

pub(super) fn visit(
    path: &Path,
    on_book: &mut impl FnMut(&str),
    import: impl FnMut(&str, &Path),
) -> io::Result<()> {
    if !fs::metadata(path)?.is_file() {
        return Err(io::Error::other("Choose a ZIP, RAR or 7z file."));
    }
    let mut source = BufReader::new(File::open(path)?);
    let Some(format) = Format::from_header(source.fill_buf()?) else {
        return Err(io::Error::other(
            "This file isn't a ZIP, RAR or 7z archive.",
        ));
    };
    let mut staging = Staging {
        on_book,
        import,
        books: 0,
        bytes: 0,
    };
    match format {
        Format::Zip => visit_zip(source, &mut staging),
        Format::Rar | Format::SevenZip => {
            visit_native(NativeSource { source, format }, &mut staging)
        }
    }?;
    if staging.books == 0 {
        return Err(io::Error::other(
            "The archive contains no supported book files.",
        ));
    }
    Ok(())
}

fn visit_zip(
    source: BufReader<File>,
    staging: &mut Staging<impl FnMut(&str), impl FnMut(&str, &Path)>,
) -> io::Result<()> {
    let mut zip = zip::ZipArchive::new(source).map_err(io::Error::other)?;
    check_entry_count(zip.len())?;
    for i in 0..zip.len() {
        let entry = zip.by_index_data(i).map_err(io::Error::other)?;
        // Some ZIP writers store permissions without a Unix file type.
        if !entry.is_file() || !matches!(entry.unix_mode().unwrap_or(0) & 0o170_000, 0 | 0o100_000)
        {
            continue;
        }
        let name = entry.name().map_err(io::Error::other)?.into_owned();
        if let Some(base) = book_name(&name) {
            staging.book(&name, base, entry.size(), |out, limit| {
                let mut entry = zip.by_index(i).map_err(io::Error::other)?;
                let bytes = io::copy(&mut entry.by_ref().take(limit), out)?;
                // At the limit, check for EOF to verify the CRC and size.
                let extra = if bytes == limit {
                    entry.read(&mut [0])? as u64
                } else {
                    0
                };
                Ok(bytes + extra)
            })?;
        }
    }
    Ok(())
}

fn visit_native(
    source: NativeSource,
    staging: &mut Staging<impl FnMut(&str), impl FnMut(&str, &Path)>,
) -> io::Result<()> {
    let mut entries = ArchiveIteratorBuilder::new(source)
        .mtree_format(false)
        .build()
        .map_err(io::Error::other)?;
    let mut count = 0;
    while let Some(header) = entries.next_header() {
        count += 1;
        check_entry_count(count)?;
        let (name, stat) = match header {
            ArchiveContents::StartOfEntry(name, stat) => (name, stat),
            ArchiveContents::Err(error) => return Err(io::Error::other(error)),
            _ => unreachable!("next_header only yields headers and errors"),
        };
        // Advance by header to skip unrelated entries without reading their
        // data into Rust, including nested bundles.
        if u64::from(stat.st_mode) & 0o170_000 != 0o100_000 {
            continue;
        }
        if let Some(base) = book_name(&name) {
            staging.book(
                &name,
                base,
                u64::try_from(stat.st_size).unwrap_or_default(),
                |out, limit| copy_native_entry(&mut entries, out, limit),
            )?;
        }
    }
    entries.close().map_err(io::Error::other)
}

fn copy_native_entry(
    entries: &mut ArchiveIterator<NativeSource>,
    out: &mut File,
    limit: u64,
) -> io::Result<u64> {
    let mut bytes = 0;
    for item in entries {
        match item {
            ArchiveContents::DataChunk(chunk) => {
                bytes += chunk.len() as u64;
                if bytes > limit {
                    return Ok(bytes);
                }
                out.write_all(&chunk)?;
            }
            ArchiveContents::EndOfEntry => return Ok(bytes),
            ArchiveContents::Err(error) => return Err(io::Error::other(error)),
            ArchiveContents::StartOfEntry(..) => break,
        }
    }
    Err(io::Error::other("Incomplete archive entry."))
}

struct NativeSource {
    source: BufReader<File>,
    format: Format,
}

impl Read for NativeSource {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.source.read(buffer)
    }
}

impl Seek for NativeSource {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        // RAR streams forward. A tail probe would let libarchive mistake an
        // uncompressed EPUB or CBZ near the end for the outer archive.
        if self.format == Format::Rar && matches!(position, SeekFrom::End(_)) {
            return Err(io::ErrorKind::Unsupported.into());
        }
        self.source.seek(position)
    }
}

struct Staging<'a, Progress, Import> {
    on_book: &'a mut Progress,
    import: Import,
    books: usize,
    bytes: u64,
}

impl<Progress: FnMut(&str), Import: FnMut(&str, &Path)> Staging<'_, Progress, Import> {
    fn book(
        &mut self,
        name: &str,
        base: &str,
        declared_size: u64,
        extract: impl FnOnce(&mut File, u64) -> io::Result<u64>,
    ) -> io::Result<()> {
        check_expansion(
            base,
            declared_size,
            self.bytes.saturating_add(declared_size),
        )?;
        (self.on_book)(base);
        // Keep the basename for metadata fallback, but give each book a new
        // private directory. No archive directory, link or mode is restored.
        let work = tempfile::tempdir()?;
        let staged = work.path().join(base);
        let mut out = File::create_new(&staged)?;
        let limit = MAX_BOOK_BYTES.min(MAX_BUNDLE_BYTES - self.bytes);
        let bytes = extract(&mut out, limit)
            .map_err(|error| io::Error::other(format!("Couldn't extract {name}: {error}")))?;
        check_expansion(base, bytes, self.bytes + bytes)?;
        drop(out);
        (self.import)(name, &staged);
        self.books += 1;
        self.bytes += bytes;
        Ok(())
    }
}

fn check_entry_count(count: usize) -> io::Result<()> {
    if count > MAX_ENTRIES {
        return Err(io::Error::other(
            "The archive has more than 10,000 entries.",
        ));
    }
    Ok(())
}

fn check_expansion(name: &str, book_bytes: u64, total_bytes: u64) -> io::Result<()> {
    if book_bytes > MAX_BOOK_BYTES {
        return Err(io::Error::other(format!("{name} expands beyond 1 GiB.")));
    }
    if total_bytes > MAX_BUNDLE_BYTES {
        return Err(io::Error::other(
            "The books in this archive expand beyond 8 GiB.",
        ));
    }
    Ok(())
}
