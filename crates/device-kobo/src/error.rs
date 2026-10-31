use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0} is not a Kobo (no .kobo/version)")]
    NotAKobo(PathBuf),
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("malformed .kobo/version: {0:?}")]
    BadVersionFile(String),
    #[error("Kobo database version {0} is older than the oldest supported ({min})", min = crate::db::MIN_SUPPORTED_DB_VERSION)]
    UnsupportedDbVersion(u32),
    #[error("Kobo database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
