//! The data directory shared by the desktop app and CLI.

use std::fs;
use std::io::{self, ErrorKind};
use std::path::Path;

pub const BUNDLE_ID: &str = "dev.parallax.app";
const LEGACY_BUNDLE_ID: &str = "dev.erc.companion";

/// Moves the whole directory before either client opens SQLite. The move
/// keeps the library, backups, and saved window state together. If both
/// directories exist, the current one takes precedence.
pub fn migrate(dir: &Path) -> io::Result<()> {
    match fs::symlink_metadata(dir) {
        Ok(_) => return Ok(()),
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }

    let legacy = dir.with_file_name(LEGACY_BUNDLE_ID);
    match fs::rename(&legacy, dir) {
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(()),
        Err(_) if dir.is_dir() => Ok(()), // The other client migrated it first.
        result => result,
    }
}
