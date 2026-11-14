//! A folder whose new books are added to the library on their own, like
//! calibre's auto-add folder. Files are added once their size and modified
//! time hold still between two looks, so a download still being written is
//! left until it's finished. What has been handed over is remembered in a
//! small file, so restarting Parallax doesn't add a file again.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

/// The folder to watch; unset watches nothing.
pub const WATCH_FOLDER: &str = "watch_folder";
/// `on` to move files to the Trash once they're in the library.
pub const WATCH_FOLDER_TRASH: &str = "watch_folder_trash";

/// Endings browsers and download tools give files still being written.
const PARTIAL: [&str; 5] = ["crdownload", "part", "download", "partial", "tmp"];

/// Whether `path` is a book or bundle the library can add, and not a hidden
/// or half-downloaded file.
pub fn is_watched_file(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
        return false;
    };
    if name.starts_with('.') {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if PARTIAL.contains(&ext.as_str()) {
        return false;
    }
    matches!(
        ext.as_str(),
        "epub" | "kepub" | "cbz" | "cbr" | "cb7" | "zip" | "rar" | "7z"
    ) || parallax_convert::amazon::EXTENSIONS.contains(&ext.as_str())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    size: u64,
    modified: u64,
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::metadata(path).ok()?;
    if !meta.is_file() {
        return None;
    }
    let modified = meta
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(Stamp {
        size: meta.len(),
        modified,
    })
}

/// Watches one folder at a time.
#[derive(Debug, Default)]
pub struct Watcher {
    state_file: PathBuf,
    /// Files already handed over, as they were then.
    seen: HashMap<PathBuf, Stamp>,
    /// Files seen once, waiting to hold still.
    waiting: HashMap<PathBuf, Stamp>,
}

impl Watcher {
    /// A watcher remembering what it handed over in `state_file`.
    pub fn load(state_file: impl Into<PathBuf>) -> Self {
        let state_file = state_file.into();
        let seen = fs::read(&state_file)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Vec<(PathBuf, Stamp)>>(&bytes).ok())
            .map_or_else(HashMap::new, |list| list.into_iter().collect());
        Self {
            state_file,
            seen,
            waiting: HashMap::new(),
        }
    }

    /// New, finished books in `folder` (not its subfolders), each returned
    /// once. A file changed since it was handed over counts as new.
    pub fn scan(&mut self, folder: &Path) -> Vec<PathBuf> {
        let Ok(entries) = fs::read_dir(folder) else {
            return Vec::new();
        };
        let mut now = HashMap::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if is_watched_file(&path)
                && let Some(stamp) = stamp(&path)
            {
                now.insert(path, stamp);
            }
        }
        let mut ready = Vec::new();
        let mut waiting = HashMap::new();
        for (path, stamp) in &now {
            if self.seen.get(path) == Some(stamp) {
                continue;
            }
            if self.waiting.get(path) == Some(stamp) {
                ready.push(path.clone());
            } else {
                waiting.insert(path.clone(), *stamp);
            }
        }
        self.waiting = waiting;
        // Forget files that left this folder, so the record stays small.
        let before = self.seen.len();
        self.seen
            .retain(|path, _| !path.starts_with(folder) || now.contains_key(path));
        for path in &ready {
            self.seen.insert(path.clone(), now[path]);
        }
        if !ready.is_empty() || self.seen.len() != before {
            self.save();
        }
        ready.sort();
        ready
    }

    fn save(&self) {
        let list: Vec<_> = self.seen.iter().collect();
        if let Ok(json) = serde_json::to_vec(&list) {
            if let Some(dir) = self.state_file.parent() {
                let _ = fs::create_dir_all(dir);
            }
            let _ = fs::write(&self.state_file, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_books_that_are_finished() {
        assert!(is_watched_file(Path::new("/d/Dune.epub")));
        assert!(is_watched_file(Path::new("/d/Saga.CBZ")));
        assert!(is_watched_file(Path::new("/d/Bundle.zip")));
        assert!(is_watched_file(Path::new("/d/Old.mobi")));
        assert!(!is_watched_file(Path::new("/d/Dune.epub.crdownload")));
        assert!(!is_watched_file(Path::new("/d/.hidden.epub")));
        assert!(!is_watched_file(Path::new("/d/notes.txt")));
    }

    #[test]
    fn hands_over_each_finished_file_once() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("in");
        fs::create_dir(&folder).unwrap();
        let state = dir.path().join("state.json");
        let book = folder.join("Dune.epub");
        fs::write(&book, b"one").unwrap();
        fs::write(folder.join("Next.epub.part"), b"half").unwrap();

        let mut watcher = Watcher::load(&state);
        assert!(watcher.scan(&folder).is_empty(), "waits for it to settle");
        assert_eq!(watcher.scan(&folder), std::slice::from_ref(&book));
        assert!(watcher.scan(&folder).is_empty());

        // Remembered across restarts.
        let mut again = Watcher::load(&state);
        again.scan(&folder);
        assert!(again.scan(&folder).is_empty());

        // Still being written: waits until it stops changing.
        let other = folder.join("Saga.cbz");
        fs::write(&other, b"a").unwrap();
        again.scan(&folder);
        fs::write(&other, b"ab").unwrap();
        assert!(again.scan(&folder).is_empty());
        assert_eq!(again.scan(&folder), [other]);
    }
}
