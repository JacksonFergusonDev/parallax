//! Tauri adapter: thin commands over the core crates. No business logic here.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use erc_convert::Kepubify;
use erc_core::send::{SendOutcome, send_library_books};
use erc_covers::{Candidate, CoverQuery};
use erc_device::DeviceInfo;
use erc_device_kobo::{KoboDevice, MACOS_VOLUMES};
use erc_epub::MetadataEdit;
use erc_library::{Book, ImportOutcome, Library};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Event carrying [`erc_core::send::Progress`] while books are sent.
const SEND_PROGRESS: &str = "send-progress";

struct AppState {
    library: Arc<Mutex<Library>>,
}

impl AppState {
    fn library(&self) -> MutexGuard<'_, Library> {
        // A panic while holding the lock can't leave the library inconsistent
        // (every write is a single SQLite statement or transaction).
        self.library.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// A library book plus the absolute path of its cover thumbnail.
#[derive(Serialize)]
struct LibraryBook {
    #[serde(flatten)]
    book: Book,
    cover: Option<PathBuf>,
}

fn with_cover(lib: &Library, book: Book) -> LibraryBook {
    LibraryBook {
        cover: lib.cover_file(book.id),
        book,
    }
}

#[derive(Serialize)]
struct ImportResult {
    path: PathBuf,
    outcome: Result<ImportOutcome, String>,
}

type CmdResult<T> = Result<T, String>;

fn err(e: impl ToString) -> String {
    e.to_string()
}

#[tauri::command]
fn list_devices() -> Vec<DeviceInfo> {
    KoboDevice::detect(Path::new(MACOS_VOLUMES))
        .iter()
        .map(KoboDevice::info)
        .collect()
}

#[tauri::command]
fn library_list(state: State<AppState>) -> CmdResult<Vec<LibraryBook>> {
    let lib = state.library();
    Ok(lib
        .books()
        .map_err(err)?
        .into_iter()
        .map(|b| with_cover(&lib, b))
        .collect())
}

#[tauri::command]
async fn library_import(
    state: State<'_, AppState>,
    paths: Vec<PathBuf>,
) -> CmdResult<Vec<ImportResult>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut lib = library.lock().unwrap_or_else(|e| e.into_inner());
        paths
            .into_iter()
            .map(|path| ImportResult {
                outcome: lib.import(&path).map_err(err),
                path,
            })
            .collect()
    })
    .await
    .map_err(err)
}

#[tauri::command]
fn library_update(state: State<AppState>, id: i64, edit: MetadataEdit) -> CmdResult<LibraryBook> {
    let mut lib = state.library();
    let book = lib.update(id, &edit).map_err(err)?;
    Ok(with_cover(&lib, book))
}

#[tauri::command]
fn library_remove(state: State<AppState>, ids: Vec<i64>) -> CmdResult<()> {
    let mut lib = state.library();
    ids.into_iter()
        .try_for_each(|id| lib.remove(id))
        .map_err(err)
}

/// Library books whose recorded copy is present on the mounted device.
#[tauri::command]
fn books_on_device(state: State<AppState>, mount: PathBuf) -> CmdResult<Vec<i64>> {
    let kobo = KoboDevice::open(&mount).map_err(err)?;
    let copies = state
        .library()
        .device_copies(&kobo.version().serial)
        .map_err(err)?;
    Ok(copies
        .into_iter()
        .filter(|c| mount.join(&c.device_path).is_file())
        .map(|c| c.book_id)
        .collect())
}

#[tauri::command]
async fn send_books(
    app: AppHandle,
    state: State<'_, AppState>,
    mount: PathBuf,
    ids: Vec<i64>,
) -> CmdResult<Vec<SendOutcome>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let converter = Kepubify::locate().map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        Ok(send_library_books(
            &lib,
            &kobo,
            &converter,
            &ids,
            |progress| {
                let _ = app.emit(SEND_PROGRESS, progress);
            },
        ))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn eject_device(mount: PathBuf) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        KoboDevice::open(&mount)
            .and_then(|kobo| kobo.eject())
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Cover candidates from online providers for a library book.
#[tauri::command]
async fn cover_search(state: State<'_, AppState>, id: i64) -> CmdResult<Vec<Candidate>> {
    let book = state.library().book(id).map_err(err)?;
    let query = CoverQuery {
        title: book.title.clone(),
        author: book.authors.first().cloned(),
        isbn: book
            .identifiers
            .iter()
            .find(|i| i.scheme.as_deref() == Some("isbn"))
            .map(|i| i.value.clone()),
    };
    tauri::async_runtime::spawn_blocking(move || erc_covers::search(&query))
        .await
        .map_err(err)
}

#[tauri::command]
async fn cover_set_url(state: State<'_, AppState>, id: i64, url: String) -> CmdResult<LibraryBook> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let image = erc_covers::download(&url).map_err(err)?;
        let mut lib = library.lock().unwrap_or_else(|e| e.into_inner());
        let book = lib.set_cover(id, &image).map_err(err)?;
        Ok(with_cover(&lib, book))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn cover_set_file(state: State<AppState>, id: i64, path: PathBuf) -> CmdResult<LibraryBook> {
    let image = std::fs::read(&path).map_err(err)?;
    let mut lib = state.library();
    let book = lib.set_cover(id, &image).map_err(err)?;
    Ok(with_cover(&lib, book))
}

#[tauri::command]
fn cover_reset(state: State<AppState>, id: i64) -> CmdResult<LibraryBook> {
    let mut lib = state.library();
    let book = lib.reset_cover(id).map_err(err)?;
    Ok(with_cover(&lib, book))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let root = app.path().app_data_dir()?.join("library");
            let library = Library::open(root)?;
            app.manage(AppState {
                library: Arc::new(Mutex::new(library)),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            library_list,
            library_import,
            library_update,
            library_remove,
            books_on_device,
            send_books,
            eject_device,
            cover_search,
            cover_set_url,
            cover_set_file,
            cover_reset
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
