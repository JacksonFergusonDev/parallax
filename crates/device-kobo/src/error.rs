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
    #[error("{0} is already on the device")]
    AlreadyOnDevice(String),
    #[error("copy of {path} is incomplete ({written} of {expected} bytes)")]
    IncompleteCopy {
        path: PathBuf,
        expected: u64,
        written: u64,
    },
    #[error("could not eject the Kobo: {0}")]
    Eject(String),
    #[error(
        "Kobo database version {0} is newer than any tested; changes are disabled to protect your device"
    )]
    UntestedDbVersion(u32),
    #[error(
        "the Kobo database is busy or was not closed cleanly ({0} exists); eject and reconnect the Kobo"
    )]
    DatabaseBusy(String),
    #[error("the Kobo database failed its integrity check before any change was made: {0}")]
    CorruptBefore(String),
    #[error(
        "the change was undone because the database failed its integrity check afterwards: {0}"
    )]
    RolledBack(String),
    #[error("can't go back to that backup: {0}")]
    CannotRestore(String),
    #[error("{0}")]
    InvalidCollection(String),
    #[error("Parallax only reads this Kobo: {0}")]
    UnsupportedDevice(String),
    #[error("Kobo database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
