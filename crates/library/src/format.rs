//! What kind of file a library book is, and how it reads.

use std::path::{Path, PathBuf};

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};
use serde::{Deserialize, Serialize};

/// The stored file's format; comics are always kept as CBZ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Format {
    Epub,
    Cbz,
}

impl Format {
    pub fn extension(self) -> &'static str {
        match self {
            Format::Epub => "epub",
            Format::Cbz => "cbz",
        }
    }
}

/// What the user thinks of it as, for filtering and defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Book,
    Comic,
    Manga,
}

/// Which way pages turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Direction {
    Ltr,
    Rtl,
}

/// The book file in a book's folder: `book.cbz` for comics, else
/// `book.epub`. Decided from the folder so paths need no database read.
pub(crate) fn stored_file(dir: &Path) -> PathBuf {
    let cbz = dir.join("book.cbz");
    if cbz.is_file() {
        cbz
    } else {
        dir.join("book.epub")
    }
}

/// Whether a book's folder holds its file, in either format.
pub(crate) fn has_stored_file(dir: &Path) -> bool {
    stored_file(dir).is_file()
}

macro_rules! text_enum {
    ($ty:ty { $($variant:ident = $text:literal),+ $(,)? }) => {
        impl $ty {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
        }
        impl ToSql for $ty {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok(ToSqlOutput::from(self.as_str()))
            }
        }
        impl FromSql for $ty {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_str()? {
                    $($text => Ok(Self::$variant),)+
                    other => Err(FromSqlError::Other(format!("unknown value {other:?}").into())),
                }
            }
        }
    };
}

text_enum!(Format { Epub = "epub", Cbz = "cbz" });
text_enum!(Kind { Book = "book", Comic = "comic", Manga = "manga" });
text_enum!(Direction { Ltr = "ltr", Rtl = "rtl" });
