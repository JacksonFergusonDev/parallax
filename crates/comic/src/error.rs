use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{0} isn't a comic archive (CBZ, CBR or CB7)")]
    NotAnArchive(PathBuf),
    #[error("{path} is damaged: {message}")]
    Damaged { path: PathBuf, message: String },
    #[error("{0} is password-protected")]
    Encrypted(PathBuf),
    #[error("{0} has no pages")]
    NoPages(PathBuf),
    #[error("{path} has more than {limit} files")]
    TooManyEntries { path: PathBuf, limit: usize },
    #[error("{name} in {path} is larger than {limit} bytes")]
    EntryTooLarge {
        path: PathBuf,
        name: String,
        limit: u64,
    },
    #[error("{path} has nothing to bundle: {reason}")]
    NotBundlable { path: PathBuf, reason: String },
    #[error("ComicInfo.xml isn't valid: {0}")]
    ComicInfo(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> Error {
    let path = path.into();
    move |source| Error::Io { path, source }
}

pub(crate) fn damaged(path: impl Into<PathBuf>) -> impl FnOnce(zip::result::ZipError) -> Error {
    let path = path.into();
    move |e| match e {
        zip::result::ZipError::Io(source) => Error::Io { path, source },
        other => Error::Damaged {
            path,
            message: other.to_string(),
        },
    }
}
