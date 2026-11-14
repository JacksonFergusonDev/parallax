//! The watched folder (Settings › Library › Add books from a folder): a
//! background thread looks every few seconds and hands new books to the main
//! window, which adds them as if they were dropped on it.

use std::path::PathBuf;
use std::time::Duration;

use parallax_core::watch::{WATCH_FOLDER, WATCH_FOLDER_TRASH, Watcher};
use tauri::{AppHandle, Emitter, EventTarget, Manager, State};

use crate::{AppState, CmdResult, err, settings};

/// How often the folder is looked at. Two looks a file holds still across
/// decide it's finished, so a book shows up within about twice this.
const EVERY: Duration = Duration::from_secs(4);
/// The event the main window adds books from.
const FOUND: &str = "watch-folder-found";

fn folder(state: &AppState) -> Option<PathBuf> {
    state
        .library()
        .setting(WATCH_FOLDER)
        .ok()
        .flatten()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
}

/// Starts watching; the thread lives as long as the app.
pub fn start(app: &AppHandle) -> tauri::Result<()> {
    let record = app.path().app_data_dir()?.join("watch-folder.json");
    let app = app.clone();
    std::thread::Builder::new()
        .name("watch-folder".into())
        .spawn(move || {
            let mut watcher = Watcher::load(record);
            loop {
                std::thread::sleep(EVERY);
                let Some(folder) = folder(&app.state()) else {
                    continue;
                };
                let found = watcher.scan(&folder);
                if !found.is_empty() {
                    let _ = app.emit_to(EventTarget::webview_window("main"), FOUND, found);
                }
            }
        })?;
    Ok(())
}

/// Moves files the main window added from the watched folder to the Trash,
/// when Settings asks for that. Only files directly in that folder.
#[tauri::command]
pub async fn watch_folder_added(state: State<'_, AppState>, paths: Vec<PathBuf>) -> CmdResult<()> {
    if !settings::is(&state, WATCH_FOLDER_TRASH, "on") {
        return Ok(());
    }
    let Some(folder) = folder(&state) else {
        return Ok(());
    };
    let inside: Vec<PathBuf> = paths
        .into_iter()
        .filter(|p| p.parent() == Some(folder.as_path()) && p.is_file())
        .collect();
    if inside.is_empty() {
        return Ok(());
    }
    tauri::async_runtime::spawn_blocking(move || trash::delete_all(&inside).map_err(err))
        .await
        .map_err(err)?
}
