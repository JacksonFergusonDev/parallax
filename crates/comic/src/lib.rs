//! Comic book archives for the library and the Kobo.
//!
//! - [`Cbz`] lists a CBZ's pages in reading order and reads them one at a
//!   time, with its ComicInfo.xml.
//! - [`convert_to_cbz`] turns CBR and CB7 into CBZ through macOS's own
//!   libarchive.
//! - [`ComicInfo`] reads and rewrites ComicInfo.xml without losing fields.
//! - [`parse_file_name`] gets series, volume and chapter from file names.
//! - [`shape`] tells pages, spreads and webtoon strips apart.
//! - [`classify_folder`], [`folder_to_cbz`] and [`chapters_to_cbz`] make one
//!   volume of a folder.
//!
//! Nothing here writes anywhere but the destination it's given, and archive
//! names are never used as paths on disk.

mod bundle;
mod cbz;
mod comicinfo;
mod convert;
mod entries;
mod error;
mod filename;
mod natural;
pub mod prepare;
pub mod shape;

pub use bundle::{Bundled, FolderKind, chapters_to_cbz, classify_folder, folder_to_cbz};
pub use cbz::{Cbz, Format, Page, sniff};
pub use comicinfo::{ComicInfo, Credit, Manga, Role, write_comic_info};
pub use convert::{Converted, convert_to_cbz};
pub use entries::{MAX_ENTRIES, MAX_PAGE_BYTES, SkipReason, Skipped};
pub use error::{Error, Result};
pub use filename::{NameInfo, parse_file_name};
pub use natural::natural_cmp;
