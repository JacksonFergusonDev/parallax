//! Tauri adapter: thin commands over the core crates. No business logic here.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use erc_convert::Kepubify;
use erc_core::collections::{CollectionChange, FolderGroup};
use erc_core::device::{OnDevice, RemoveOutcome};
use erc_core::rules::PlannedCollection;
use erc_core::send::{SendOutcome, send_library_books, update_on_device};
use erc_covers::{Candidate, CoverQuery};
use erc_device::{Collection, DeviceBook, DeviceInfo};
use erc_device_kobo::{KoboDevice, MACOS_VOLUMES, tidy_apple_double};
use erc_epub::MetadataEdit;
use erc_library::{Book, ImportOutcome, Library, Rule};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Event carrying [`erc_core::send::Progress`] while books are sent.
const SEND_PROGRESS: &str = "send-progress";

struct AppState {
    library: Arc<Mutex<Library>>,
    /// Pre-change backups of device databases.
    backup_root: PathBuf,
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
fn books_on_device(state: State<AppState>, mount: PathBuf) -> CmdResult<Vec<OnDevice>> {
    let kobo = KoboDevice::open(&mount).map_err(err)?;
    erc_core::device::books_on_device(&state.library(), &kobo).map_err(err)
}

/// Everything on the device, as Nickel lists it (read-only).
#[tauri::command]
async fn device_books(mount: PathBuf) -> CmdResult<Vec<DeviceBook>> {
    tauri::async_runtime::spawn_blocking(move || {
        KoboDevice::open(&mount)
            .and_then(|k| k.open_db())
            .and_then(|db| db.books())
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn remove_from_device(
    state: State<'_, AppState>,
    mount: PathBuf,
    content_ids: Vec<String>,
) -> CmdResult<Vec<RemoveOutcome>> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        erc_core::device::remove_from_device(Some(&lib), &kobo, &backups, &content_ids).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn update_books(
    app: AppHandle,
    state: State<'_, AppState>,
    mount: PathBuf,
    ids: Vec<i64>,
) -> CmdResult<Vec<SendOutcome>> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let converter = Kepubify::locate().map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        update_on_device(&lib, &kobo, &converter, &backups, &ids, |progress| {
            let _ = app.emit(SEND_PROGRESS, progress);
        })
        .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Deletes macOS `._*` files from the device; returns how many.
#[tauri::command]
async fn tidy_device(mount: PathBuf) -> CmdResult<usize> {
    tauri::async_runtime::spawn_blocking(move || {
        KoboDevice::open(&mount)
            .map(|k| tidy_apple_double(&k))
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn send_books(
    app: AppHandle,
    state: State<'_, AppState>,
    mount: PathBuf,
    ids: Vec<i64>,
    collection: Option<String>,
) -> CmdResult<Vec<SendOutcome>> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let converter = Kepubify::locate().map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        let outcomes = send_library_books(&lib, &kobo, &converter, &ids, |progress| {
            let _ = app.emit(SEND_PROGRESS, progress);
        });
        // Collection rows can be written before Nickel imports the books.
        if let Some(name) = collection.filter(|n| !n.trim().is_empty()) {
            let content_ids: Vec<String> = outcomes
                .iter()
                .filter_map(|o| o.result.as_ref().ok().map(|s| s.content_id.clone()))
                .collect();
            if !content_ids.is_empty() {
                erc_core::collections::apply(
                    &kobo,
                    &backups,
                    &[CollectionChange::Add { name, content_ids }],
                )
                .map_err(err)?;
            }
        }
        // Enabled grouping rules file the new books too (no-op otherwise).
        erc_core::rules::run(&lib, &kobo, &backups).map_err(err)?;
        Ok(outcomes)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn list_collections(mount: PathBuf) -> CmdResult<Vec<Collection>> {
    tauri::async_runtime::spawn_blocking(move || {
        KoboDevice::open(&mount)
            .and_then(|k| k.open_db())
            .and_then(|db| db.collections())
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn change_collections(
    state: State<'_, AppState>,
    mount: PathBuf,
    changes: Vec<CollectionChange>,
) -> CmdResult<()> {
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        erc_core::collections::apply(&kobo, &backups, &changes).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn folder_groups(mount: PathBuf) -> CmdResult<Vec<FolderGroup>> {
    tauri::async_runtime::spawn_blocking(move || {
        let books = KoboDevice::open(&mount)
            .and_then(|k| k.open_db())
            .and_then(|db| db.books())
            .map_err(err)?;
        Ok(erc_core::collections::folder_groups(&books))
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
struct Reconciled {
    /// Books whose series was applied.
    series: usize,
    /// Books added to collections by grouping rules.
    grouped: usize,
}

/// On connect: applies series to books Nickel has imported since they were
/// sent, then runs enabled grouping rules. Opens no write session when
/// there's nothing to do.
#[tauri::command]
async fn reconcile_device(state: State<'_, AppState>, mount: PathBuf) -> CmdResult<Reconciled> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        Ok(Reconciled {
            series: erc_core::collections::reconcile_series(&lib, &kobo, &backups).map_err(err)?,
            grouped: erc_core::rules::run(&lib, &kobo, &backups).map_err(err)?,
        })
    })
    .await
    .map_err(err)?
}

#[tauri::command]
fn rules_list(state: State<AppState>) -> CmdResult<Vec<Rule>> {
    state.library().rules().map_err(err)
}

#[tauri::command]
fn rules_save(state: State<AppState>, rule: Rule) -> CmdResult<Rule> {
    state.library().save_rule(&rule).map_err(err)
}

#[tauri::command]
fn rules_delete(state: State<AppState>, id: i64) -> CmdResult<()> {
    state.library().delete_rule(id).map_err(err)
}

/// What enabled rules would add on the device (read-only).
#[tauri::command]
async fn rules_preview(
    state: State<'_, AppState>,
    mount: PathBuf,
) -> CmdResult<Vec<PlannedCollection>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        erc_core::rules::plan(&lib.rules().map_err(err)?, &lib, &kobo).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn rules_apply(state: State<'_, AppState>, mount: PathBuf) -> CmdResult<usize> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library.lock().unwrap_or_else(|e| e.into_inner());
        erc_core::rules::run(&lib, &kobo, &backups).map_err(err)
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
        // Reopens the window where and how big it was last time.
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let library = Library::open(data.join("library"))?;
            app.manage(AppState {
                library: Arc::new(Mutex::new(library)),
                backup_root: data.join("device-backups"),
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
            device_books,
            remove_from_device,
            update_books,
            tidy_device,
            send_books,
            list_collections,
            change_collections,
            folder_groups,
            reconcile_device,
            rules_list,
            rules_save,
            rules_delete,
            rules_preview,
            rules_apply,
            eject_device,
            cover_search,
            cover_set_url,
            cover_set_file,
            cover_reset
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
