//! Tauri adapter: thin commands over the core crates. No business logic here.

// Tauri commands must take their arguments by value.
#![allow(clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use erc_convert::Kepubify;
use erc_convert::amazon::ChildProcess;
use erc_core::collections::{CollectionChange, FolderGroup};
use erc_core::device::{OnDevice, RemoveOutcome};
use erc_core::duplicates::{DeviceGroup, LibraryGroup};
use erc_core::enhance::{Accepted, DeviceClues, Proposal};
use erc_core::import::DeviceImportOutcome;
use erc_core::ingest::import_file;
use erc_core::rules::PlannedCollection;
use erc_core::rules::Trigger;
use erc_core::send::{SendOutcome, send_library_books, update_on_device};
use erc_covers::{Candidate, CoverQuery};
use erc_device::{Collection, DeviceBook, DeviceInfo};
use erc_device_kobo::{
    BackupInfo, BookCheck, ErcState, KoboDevice, MACOS_VOLUMES, RestoreOutcome, tidy_apple_double,
};
use erc_epub::MetadataEdit;
use erc_library::{
    Book, CUSTOM_COVER_MAX_HEIGHT, CoverQuality, ImportOutcome, Library, Reading, Rule, Snapshot,
    SnapshotRestore,
};
use erc_upscale::CoreMl;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

/// Event carrying [`erc_core::send::Progress`] while books are sent.
const SEND_PROGRESS: &str = "send-progress";
/// Event carrying [`ImportProgress`] while books come in from a device.
const IMPORT_PROGRESS: &str = "import-progress";
/// Event carrying [`ImportProgress`] while series and genres are looked up.
const ENHANCE_PROGRESS: &str = "enhance-progress";
/// Event carrying [`ImportProgress`] while the library is checked.
const CHECK_PROGRESS: &str = "check-progress";
/// Event carrying [`UpscaleProgress`] while a cover is upscaled.
const UPSCALE_PROGRESS: &str = "upscale-progress";
/// The compiled model, relative to the app's resources.
const UPSCALE_MODEL: &str = "models/cover-upscaler.mlmodelc";

#[derive(Clone, Serialize)]
struct ImportProgress {
    index: usize,
    total: usize,
    title: String,
}

struct AppState {
    library: Arc<Mutex<Library>>,
    /// Pre-change backups of device databases.
    backup_root: PathBuf,
    upscaler: Arc<Upscaler>,
}

/// Cover upscaling. One cover at a time: the model fills the GPU.
struct Upscaler {
    /// Where the model is, if this build has it.
    model: Option<PathBuf>,
    /// Loaded on first use, as it takes seconds and a lot of memory.
    engine: Mutex<Option<CoreMl>>,
    cancelled: AtomicBool,
    /// Results waiting for the user's decision.
    previews: PathBuf,
    /// Numbers each result, so the webview never shows a cached earlier one.
    runs: AtomicU64,
}

impl Upscaler {
    fn new(model: Option<PathBuf>, previews: PathBuf) -> Self {
        // Anything here was left undecided when the app last closed.
        let _ = std::fs::remove_dir_all(&previews);
        Self {
            model,
            engine: Mutex::new(None),
            cancelled: AtomicBool::new(false),
            previews,
            runs: AtomicU64::new(0),
        }
    }

    fn after_file(&self, id: i64, run: u64) -> PathBuf {
        self.previews.join(format!("{id}-{run}-after.png"))
    }
}

/// The bundled model, or the one `just upscale-model` built for development.
fn find_upscale_model(app: &AppHandle) -> Option<PathBuf> {
    let bundled = app.path().resource_dir().ok()?.join(UPSCALE_MODEL);
    let built =
        cfg!(debug_assertions).then(|| Path::new(env!("CARGO_MANIFEST_DIR")).join(UPSCALE_MODEL));
    [Some(bundled), built]
        .into_iter()
        .flatten()
        .find(|path| path.is_dir())
}

#[derive(Serialize)]
struct CoverInfo {
    width: u32,
    height: u32,
    quality: CoverQuality,
}

#[derive(Clone, Serialize)]
struct UpscaleProgress {
    id: i64,
    done: usize,
    total: usize,
}

/// An upscaled cover waiting to be kept or discarded.
#[derive(Serialize)]
struct UpscalePreview {
    id: i64,
    run: u64,
    before: PathBuf,
    after: PathBuf,
    before_size: (u32, u32),
    after_size: (u32, u32),
}

impl AppState {
    fn library(&self) -> MutexGuard<'_, Library> {
        // A panic while holding the lock can't leave the library inconsistent
        // (every write is a single SQLite statement or transaction).
        self.library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// A library book plus the absolute path of its cover thumbnail.
#[derive(Serialize)]
struct LibraryBook {
    #[serde(flatten)]
    book: Book,
    cover: Option<PathBuf>,
    /// Progress last seen on a device.
    reading: Option<Reading>,
    /// Highlights and notes copied from devices.
    annotations: usize,
}

fn with_cover(lib: &Library, book: Book) -> LibraryBook {
    LibraryBook {
        cover: lib.cover_file(book.id),
        reading: lib.reading(book.id).ok().flatten(),
        annotations: lib.annotation_count(book.id).unwrap_or(0),
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
async fn list_devices() -> CmdResult<Vec<DeviceInfo>> {
    // Removable-media reads can stall. Never perform them on the UI thread.
    tauri::async_runtime::spawn_blocking(|| {
        KoboDevice::detect(Path::new(MACOS_VOLUMES))
            .iter()
            .map(KoboDevice::info)
            .collect()
    })
    .await
    .map_err(err)
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
        let amazon = ChildProcess::current().map_err(err)?;
        let mut lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(paths
            .into_iter()
            .map(|path| ImportResult {
                outcome: import_file(&mut lib, &amazon, &path).map_err(err),
                path,
            })
            .collect())
    })
    .await
    .map_err(err)?
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
    let detail = match ids.as_slice() {
        [id] => lib.book(*id).map_err(err)?.title,
        _ => format!("{} books", ids.len()),
    };
    lib.snapshot("remove", Some(&detail)).map_err(err)?;
    ids.into_iter()
        .try_for_each(|id| lib.remove(id))
        .map_err(err)
}

/// Points the library can go back to, newest first.
#[tauri::command]
fn library_snapshots(state: State<AppState>) -> Vec<Snapshot> {
    state.library().snapshots()
}

#[tauri::command]
fn library_restore(state: State<AppState>, file: String) -> CmdResult<SnapshotRestore> {
    state.library().restore_snapshot(&file).map_err(err)
}

/// Database backups of a Kobo, newest first.
#[tauri::command]
async fn device_backups(state: State<'_, AppState>, mount: PathBuf) -> CmdResult<Vec<BackupInfo>> {
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(KoboDevice::open(&mount).map_err(err)?.backups(&backups))
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn device_restore(
    state: State<'_, AppState>,
    mount: PathBuf,
    file: String,
) -> CmdResult<RestoreOutcome> {
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        kobo.restore_backup(&backups, Path::new(&file)).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Library books whose recorded copy is present on the mounted device.
#[tauri::command]
async fn books_on_device(state: State<'_, AppState>, mount: PathBuf) -> CmdResult<Vec<OnDevice>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::device::books_on_device(&lib, &kobo).map_err(err)
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
struct KoboBook {
    #[serde(flatten)]
    book: DeviceBook,
    cover: Option<PathBuf>,
}

/// Everything on the device, as Nickel lists it (read-only).
#[tauri::command]
async fn device_books(app: AppHandle, mount: PathBuf) -> CmdResult<Vec<KoboBook>> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(err)?
        .join("device-covers");
    tauri::async_runtime::spawn_blocking(move || {
        let device = KoboDevice::open(&mount).map_err(err)?;
        let db = device.open_db().map_err(err)?;
        let images = db.image_ids().map_err(err)?;
        Ok(db
            .books()
            .map_err(err)?
            .into_iter()
            .map(|book| {
                let cover = images
                    .get(&book.id)
                    .and_then(|id| device.cache_cover(id, &cache));
                KoboBook { book, cover }
            })
            .collect())
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
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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

/// Log entries the app shows; the full log stays on the device.
const HISTORY_SHOWN: usize = 200;

#[derive(Serialize)]
struct DeviceHistory {
    #[serde(flatten)]
    state: ErcState,
    /// ContentIDs of ERC-written books changed by something else since.
    changed: Vec<String>,
    /// ContentIDs of ERC-written books no longer on the device.
    missing: Vec<String>,
}

/// Saves a device report (model, firmware, database layout; no titles,
/// serial or reading data) to `path`. Only reads the device.
#[tauri::command]
async fn device_report(mount: PathBuf, path: PathBuf) -> CmdResult<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let report = KoboDevice::open(&mount).map_err(err)?.report();
        let json = serde_json::to_string_pretty(&report).map_err(err)?;
        std::fs::write(&path, json + "\n").map_err(err)
    })
    .await
    .map_err(err)?
}

/// What ERC's `/.erc/` records say about the device.
#[tauri::command]
async fn device_history(mount: PathBuf) -> CmdResult<DeviceHistory> {
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let mut state = kobo.erc_state();
        let checks = state.check(kobo.mount());
        let ids = |want: BookCheck| {
            checks
                .iter()
                .filter(|(_, c)| *c == want)
                .map(|(id, _)| id.clone())
                .collect()
        };
        state.log.truncate(HISTORY_SHOWN);
        Ok(DeviceHistory {
            changed: ids(BookCheck::Changed),
            missing: ids(BookCheck::Missing),
            state,
        })
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
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        // Enabled grouping rules file the new books into existing collections
        // too (no-op otherwise).
        erc_core::rules::run(&lib, &kobo, &backups, Trigger::Automatic).map_err(err)?;
        Ok(outcomes)
    })
    .await
    .map_err(err)?
}

/// A book's highlights, notes and bookmarks, in reading order.
#[tauri::command]
fn annotations_list(state: State<AppState>, id: i64) -> CmdResult<Vec<erc_library::Annotation>> {
    state.library().annotations(id).map_err(err)
}

/// Saves a book's annotations as Markdown or JSON at `path` (chosen by the
/// user in a save dialog).
#[tauri::command]
fn annotations_export(
    state: State<AppState>,
    id: i64,
    format: String,
    path: PathBuf,
) -> CmdResult<usize> {
    let lib = state.library();
    let book = lib.book(id).map_err(err)?;
    let items = lib.annotations(id).map_err(err)?;
    let text = match format.as_str() {
        "json" => erc_core::annotations::json(&book, &items),
        _ => erc_core::annotations::markdown(&book, &items),
    };
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(items.len())
}

/// "Check library": every book's cover, metadata and EPUB structure.
#[tauri::command]
async fn library_check(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Vec<erc_core::health::Finding>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::health::check_library(&lib, |index, total| {
            let _ = app.emit(
                CHECK_PROGRESS,
                ImportProgress {
                    index,
                    total,
                    title: String::new(),
                },
            );
        })
        .map_err(err)
    })
    .await
    .map_err(err)?
}

/// "Check this Kobo": mismatches, duplicate collections, leftover covers.
#[tauri::command]
async fn kobo_health(mount: PathBuf) -> CmdResult<erc_core::device::Health> {
    tauri::async_runtime::spawn_blocking(move || {
        erc_core::device::health(&KoboDevice::open(&mount).map_err(err)?).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Deletes cached covers of books no longer on the device; returns
/// (files, bytes).
#[tauri::command]
async fn clean_images(mount: PathBuf) -> CmdResult<(usize, u64)> {
    tauri::async_runtime::spawn_blocking(move || {
        erc_device_kobo::remove_orphaned_images(&KoboDevice::open(&mount).map_err(err)?)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn merge_collections(state: State<'_, AppState>, mount: PathBuf) -> CmdResult<usize> {
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        erc_core::device::merge_collections(&kobo, &backups).map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
async fn set_reading_status(
    state: State<'_, AppState>,
    mount: PathBuf,
    content_ids: Vec<String>,
    status: erc_device::ReadStatus,
    reset_position: bool,
    reset_time: bool,
) -> CmdResult<usize> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::device::change_reading_status(
            Some(&lib),
            &kobo,
            &backups,
            &content_ids,
            status,
            reset_position,
            reset_time,
        )
        .map_err(err)
    })
    .await
    .map_err(err)?
}

/// What's worth raising about books just added. Holds the library only to
/// list them, not while their files are read.
#[tauri::command]
async fn library_review(
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> CmdResult<erc_core::review::Review> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (books, files, duplicates) = {
            let lib = library
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let books = ids
                .iter()
                .map(|&id| lib.book(id))
                .collect::<erc_library::Result<Vec<_>>>()
                .map_err(err)?;
            let files: Vec<_> = ids.iter().map(|&id| lib.book_files(id)).collect();
            let duplicates = erc_core::duplicates::library_duplicates(&lib).map_err(err)?;
            (books, files, duplicates)
        };
        Ok(erc_core::review::review(&books, &files, duplicates))
    })
    .await
    .map_err(err)?
}

/// Series and genre proposals for `ids`, using a connected Kobo's series and
/// folders too when `mount` is given. Holds the library only to read the
/// books, not during lookups.
#[tauri::command]
async fn enhance_propose(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<i64>,
    mount: Option<PathBuf>,
) -> CmdResult<Vec<Proposal>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (books, options, clues, fetch) = {
            let lib = library
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let books = ids
                .iter()
                .map(|&id| lib.book(id))
                .collect::<erc_library::Result<Vec<_>>>()
                .map_err(err)?;
            let clues = match mount {
                Some(mount) => {
                    DeviceClues::read(&lib, &KoboDevice::open(&mount).map_err(err)?).map_err(err)?
                }
                None => DeviceClues::default(),
            };
            let fetch = erc_core::enhance::fetcher(&lib)?;
            (books, erc_core::enhance::options(&lib), clues, fetch)
        };
        Ok(erc_core::enhance::propose(
            &books,
            &options,
            &fetch,
            &clues,
            |index, total, title| {
                let _ = app.emit(
                    ENHANCE_PROGRESS,
                    ImportProgress {
                        index,
                        total,
                        title: title.to_owned(),
                    },
                );
            },
        ))
    })
    .await
    .map_err(err)?
}

/// Saves accepted series and genres; returns the updated books.
#[tauri::command]
fn enhance_apply(state: State<AppState>, accepted: Vec<Accepted>) -> CmdResult<Vec<LibraryBook>> {
    let mut lib = state.library();
    let books = erc_core::enhance::apply(&mut lib, &accepted).map_err(err)?;
    Ok(books.into_iter().map(|b| with_cover(&lib, b)).collect())
}

/// Settings the app may read and change.
const SETTINGS: [&str; 2] = [
    erc_core::enhance::GOOGLE_BOOKS_KEY,
    erc_core::review::REVIEW_ON_ADD,
];

#[tauri::command]
fn setting_get(state: State<AppState>, key: String) -> CmdResult<Option<String>> {
    if !SETTINGS.contains(&key.as_str()) {
        return Err(format!("unknown setting {key}"));
    }
    state.library().setting(&key).map_err(err)
}

#[tauri::command]
fn setting_set(state: State<AppState>, key: String, value: Option<String>) -> CmdResult<()> {
    if !SETTINGS.contains(&key.as_str()) {
        return Err(format!("unknown setting {key}"));
    }
    let value = value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    state
        .library()
        .set_setting(&key, value.as_deref())
        .map_err(err)
}

/// Probable duplicates in the library.
#[tauri::command]
async fn library_duplicates(state: State<'_, AppState>) -> CmdResult<Vec<LibraryGroup>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::duplicates::library_duplicates(&lib).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Probable duplicates on the device. Only reads it.
#[tauri::command]
async fn device_duplicates(
    state: State<'_, AppState>,
    mount: PathBuf,
) -> CmdResult<Vec<DeviceGroup>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::duplicates::device_duplicates(&lib, &kobo).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Remembers that a group isn't duplicates: in the library, or on the
/// device at `mount`.
#[tauri::command]
async fn dismiss_duplicates(
    state: State<'_, AppState>,
    key: String,
    mount: Option<PathBuf>,
) -> CmdResult<()> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let scope = match mount {
            Some(mount) => erc_core::duplicates::device_scope(
                &KoboDevice::open(&mount).map_err(err)?.version().serial,
            ),
            None => erc_core::duplicates::LIBRARY_SCOPE.to_owned(),
        };
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lib.dismiss_duplicates(&scope, &key).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Adds books from the device to the library (all when `content_ids` is
/// `None`). Only reads the device.
#[tauri::command]
async fn import_from_device(
    app: AppHandle,
    state: State<'_, AppState>,
    mount: PathBuf,
    content_ids: Option<Vec<String>>,
) -> CmdResult<Vec<DeviceImportOutcome>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(err)?;
        let mut lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::import::import_from_device(
            &mut lib,
            &kobo,
            content_ids.as_deref(),
            |index, total, title| {
                let _ = app.emit(
                    IMPORT_PROGRESS,
                    ImportProgress {
                        index,
                        total,
                        title: title.to_owned(),
                    },
                );
            },
        )
        .map_err(err)
    })
    .await
    .map_err(err)?
}

#[derive(Serialize)]
struct SavedStats {
    device_serial: String,
    taken_at: String,
    stats: serde_json::Value,
}

/// Reading statistics: read fresh from the Kobo at `mount` (and saved), or
/// the last ones saved when no Kobo is connected. Only reads the device.
#[tauri::command]
async fn reading_stats(
    state: State<'_, AppState>,
    mount: Option<PathBuf>,
) -> CmdResult<Option<SavedStats>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(mount) = mount {
            let kobo = KoboDevice::open(&mount).map_err(err)?;
            let stats = erc_core::stats::reading_stats(&kobo, Some(&lib)).map_err(err)?;
            let json = serde_json::to_string(&stats).map_err(err)?;
            lib.save_reading_stats(&kobo.version().serial, &json)
                .map_err(err)?;
        }
        let Some((device_serial, taken_at, json)) = lib.latest_reading_stats().map_err(err)? else {
            return Ok(None);
        };
        Ok(Some(SavedStats {
            device_serial,
            taken_at,
            stats: serde_json::from_str(&json).map_err(err)?,
        }))
    })
    .await
    .map_err(err)?
}

/// Genre tags and Kobo collections not using standard names (reads only).
/// `mount` adds the connected Kobo's collections.
#[tauri::command]
async fn genres_plan(
    state: State<'_, AppState>,
    mount: Option<PathBuf>,
) -> CmdResult<erc_core::genres::TidyPlan> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = mount
            .map(|m| KoboDevice::open(&m))
            .transpose()
            .map_err(err)?;
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::genres::plan(&lib, kobo.as_ref()).map_err(err)
    })
    .await
    .map_err(err)?
}

/// Renames the chosen genre tags and Kobo collections (backed up first).
#[tauri::command]
async fn genres_apply(
    state: State<'_, AppState>,
    mount: Option<PathBuf>,
    choice: erc_core::genres::TidyChoice,
) -> CmdResult<erc_core::genres::TidyResult> {
    let library = state.library.clone();
    let backups = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = mount
            .map(|m| KoboDevice::open(&m))
            .transpose()
            .map_err(err)?;
        let mut lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::genres::apply_tidy(&mut lib, kobo.as_ref(), &backups, &choice).map_err(err)
    })
    .await
    .map_err(err)?
}

/// calibre's current library folder, if there is one.
#[tauri::command]
fn calibre_default_library() -> Option<PathBuf> {
    erc_core::calibre::default_library()
}

/// What importing the calibre library at `path` would do (reads only).
#[tauri::command]
async fn calibre_preview(
    state: State<'_, AppState>,
    path: PathBuf,
) -> CmdResult<erc_core::calibre::CalibrePreview> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let books = erc_core::calibre::read_library(&path).map_err(err)?;
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(erc_core::calibre::preview(&lib, &books))
    })
    .await
    .map_err(err)?
}

/// Imports the calibre library at `path`; only reads it.
#[tauri::command]
async fn calibre_import(
    app: AppHandle,
    state: State<'_, AppState>,
    path: PathBuf,
) -> CmdResult<Vec<erc_core::calibre::CalibreImportOutcome>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let books = erc_core::calibre::read_library(&path).map_err(err)?;
        let amazon = ChildProcess::current().map_err(err)?;
        let mut lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(erc_core::calibre::import(
            &mut lib,
            &amazon,
            &books,
            |index, total, title| {
                let _ = app.emit(
                    IMPORT_PROGRESS,
                    ImportProgress {
                        index,
                        total,
                        title: title.to_owned(),
                    },
                );
            },
        ))
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
    /// Library books whose reading progress changed.
    reading: usize,
    /// Highlights, notes and bookmarks new, changed or removed on the device.
    annotations: usize,
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
        let mut lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Ok(Reconciled {
            series: erc_core::collections::reconcile_series(&lib, &kobo, &backups).map_err(err)?,
            grouped: erc_core::rules::run(&lib, &kobo, &backups, Trigger::Automatic)
                .map_err(err)?,
            reading: erc_core::device::sync_reading(&lib, &kobo).map_err(err)?,
            annotations: erc_core::device::sync_annotations(&mut lib, &kobo).map_err(err)?,
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
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        erc_core::rules::run(&lib, &kobo, &backups, Trigger::User).map_err(err)
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
        isbn: book.isbn().map(str::to_owned),
        asin: book.asin().map(str::to_owned),
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
        let mut lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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

/// Whether this build can upscale covers.
#[tauri::command]
fn cover_upscale_available(state: State<AppState>) -> bool {
    cfg!(target_os = "macos") && state.upscaler.model.is_some()
}

/// The size and rating of the cover sent to devices. Read on demand rather
/// than stored: it takes about a millisecond, and can't go stale.
#[tauri::command]
async fn cover_info(state: State<'_, AppState>, id: i64) -> CmdResult<Option<CoverInfo>> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        lib.cover_size(id).map(|(width, height)| CoverInfo {
            width,
            height,
            quality: CoverQuality::of_height(height),
        })
    })
    .await
    .map_err(err)
}

/// How many tiles upscaling these books' covers will run, for estimating the
/// time left. Covers that are already full size cost nothing.
#[tauri::command]
async fn cover_upscale_tiles(state: State<'_, AppState>, ids: Vec<i64>) -> CmdResult<usize> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        ids.iter()
            .filter_map(|&id| lib.cover_size(id))
            .filter(|&(_, height)| height < CUSTOM_COVER_MAX_HEIGHT)
            .map(|(width, height)| erc_upscale::tile_count(width, height))
            .sum()
    })
    .await
    .map_err(err)
}

/// Upscales a book's cover without changing the book. `None` means the cover
/// is already full size. A long job: the UI cancels with `cover_upscale_cancel`.
#[tauri::command]
async fn cover_upscale_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> CmdResult<Option<UpscalePreview>> {
    let library = state.library.clone();
    let upscaler = state.upscaler.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let model = upscaler
            .model
            .as_ref()
            .ok_or("cover upscaling isn't available")?;
        let cover = library
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cover_source(id)
            .map_err(err)?
            .ok_or("this book has no cover")?;
        let mut engine = upscaler
            .engine
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Reset only now: a job cancelled just before this one has finished,
        // since it held the lock.
        upscaler.cancelled.store(false, Ordering::Relaxed);
        if engine.is_none() {
            *engine = Some(CoreMl::load(model).map_err(err)?);
        }
        let Some(engine) = engine.as_ref() else {
            return Err("cover upscaling isn't available".to_owned());
        };
        let up = erc_upscale::upscale_cover(
            engine,
            &cover,
            CUSTOM_COVER_MAX_HEIGHT,
            |done, total| {
                let _ = app.emit(UPSCALE_PROGRESS, UpscaleProgress { id, done, total });
            },
            &upscaler.cancelled,
        );
        let up = match up {
            Err(erc_upscale::Error::AlreadyFullSize) => return Ok(None),
            other => other.map_err(err)?,
        };
        let run = upscaler.runs.fetch_add(1, Ordering::Relaxed);
        std::fs::create_dir_all(&upscaler.previews).map_err(err)?;
        let before = upscaler.previews.join(format!("{id}-{run}-before.png"));
        let after = upscaler.after_file(id, run);
        std::fs::write(&before, &up.before_png).map_err(err)?;
        std::fs::write(&after, &up.after_png).map_err(err)?;
        Ok(Some(UpscalePreview {
            id,
            run,
            before,
            after,
            before_size: up.before_size,
            after_size: up.after_size,
        }))
    })
    .await
    .map_err(err)?
}

/// Makes a preview the book's cover.
#[tauri::command]
fn cover_upscale_apply(state: State<AppState>, id: i64, run: u64) -> CmdResult<LibraryBook> {
    let image = std::fs::read(state.upscaler.after_file(id, run)).map_err(err)?;
    let mut lib = state.library();
    let book = lib.set_upscaled_cover(id, &image).map_err(err)?;
    Ok(with_cover(&lib, book))
}

/// Stops the running job and discards every undecided preview.
#[tauri::command]
fn cover_upscale_cancel(state: State<AppState>) {
    state.upscaler.cancelled.store(true, Ordering::Relaxed);
    let _ = std::fs::remove_dir_all(&state.upscaler.previews);
}

/// The dominant colour of each cover, for sorting by colour; `None` for a
/// cover that can't be read. Only reads covers the app itself stores.
#[tauri::command]
async fn cover_colors(app: AppHandle, paths: Vec<PathBuf>) -> CmdResult<Vec<Option<[u8; 3]>>> {
    let roots = [
        app.path().app_data_dir().map_err(err)?.join("library"),
        app.path()
            .app_cache_dir()
            .map_err(err)?
            .join("device-covers"),
    ]
    .map(|root| root.canonicalize().unwrap_or(root));
    tauri::async_runtime::spawn_blocking(move || {
        // Anything outside those folders becomes a path that reads as nothing.
        let allowed: Vec<PathBuf> = paths
            .iter()
            .map(|path| {
                path.canonicalize()
                    .ok()
                    .filter(|path| roots.iter().any(|root| path.starts_with(root)))
                    .unwrap_or_default()
            })
            .collect();
        erc_library::dominant_colors(&allowed)
    })
    .await
    .map_err(err)
}

/// The frontend calls this after its initial layout and covers are ready.
#[tauri::command]
fn show_main_window(window: tauri::WebviewWindow) -> tauri::Result<()> {
    window.show()?;
    window.set_focus()
}

/// Saved window geometry. Renamed from the plugin's default when the default
/// window grew, so existing installs get the new size once.
const WINDOW_STATE_FILE: &str = ".window-state-v2.json";

/// A roomy first window for a screen of this logical size: most of the
/// screen, without sprawling across a large display.
fn first_window_size(screen_width: f64, screen_height: f64) -> (f64, f64) {
    (
        (screen_width * 0.9).min(1680.0),
        (screen_height * 0.9).min(1080.0),
    )
}

/// Sizes and centres the window on first launch; later launches restore the
/// saved geometry instead.
fn size_first_window(app: &tauri::App) -> tauri::Result<()> {
    if app
        .path()
        .app_config_dir()?
        .join(WINDOW_STATE_FILE)
        .exists()
    {
        return Ok(());
    }
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let monitor = match window.current_monitor()? {
        Some(monitor) => Some(monitor),
        None => window.primary_monitor()?,
    };
    if let Some(monitor) = monitor {
        let screen = monitor.size().to_logical::<f64>(monitor.scale_factor());
        let (width, height) = first_window_size(screen.width, screen.height);
        window.set_size(tauri::LogicalSize::new(width, height))?;
        window.center()?;
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Reopens the window where and how big it was last time.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_filename(WINDOW_STATE_FILE)
                .with_state_flags(
                    tauri_plugin_window_state::StateFlags::all()
                        - tauri_plugin_window_state::StateFlags::VISIBLE,
                )
                .build(),
        )
        .setup(|app| {
            let data = app.path().app_data_dir()?;
            let library = Library::open(data.join("library"))?;
            let previews = app.path().app_cache_dir()?.join("upscale-previews");
            let model = find_upscale_model(app.handle());
            app.manage(AppState {
                library: Arc::new(Mutex::new(library)),
                backup_root: data.join("device-backups"),
                upscaler: Arc::new(Upscaler::new(model, previews)),
            });
            size_first_window(app)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            show_main_window,
            list_devices,
            library_list,
            library_import,
            library_review,
            library_update,
            library_remove,
            library_snapshots,
            library_restore,
            device_backups,
            device_restore,
            books_on_device,
            device_books,
            remove_from_device,
            update_books,
            tidy_device,
            device_history,
            send_books,
            import_from_device,
            calibre_default_library,
            reading_stats,
            genres_plan,
            genres_apply,
            calibre_preview,
            calibre_import,
            library_duplicates,
            enhance_propose,
            kobo_health,
            library_check,
            annotations_list,
            annotations_export,
            device_report,
            clean_images,
            merge_collections,
            set_reading_status,
            enhance_apply,
            setting_get,
            setting_set,
            device_duplicates,
            dismiss_duplicates,
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
            cover_reset,
            cover_info,
            cover_upscale_available,
            cover_upscale_tiles,
            cover_upscale_preview,
            cover_upscale_apply,
            cover_upscale_cancel,
            cover_colors
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::first_window_size;

    #[test]
    fn first_window_fills_most_of_a_laptop_screen() {
        assert_eq!(first_window_size(1500.0, 1000.0), (1350.0, 900.0));
    }

    #[test]
    fn first_window_stops_growing_on_large_displays() {
        assert_eq!(first_window_size(3840.0, 2160.0), (1680.0, 1080.0));
    }

    #[test]
    fn every_command_is_declared_and_granted_to_the_main_window() {
        fn names(text: &str, open: &str, close: char) -> Vec<String> {
            let (_, rest) = text.split_once(open).unwrap();
            let (list, _) = rest.split_once(close).unwrap();
            list.split(',')
                .map(|name| name.trim().trim_matches('"').to_string())
                .filter(|name| !name.is_empty())
                .collect()
        }

        let handled = names(include_str!("lib.rs"), "generate_handler![\n", ']');
        let declared = names(include_str!("../build.rs"), "&[&str] = &[\n", ']');
        assert_eq!(handled, declared, "build.rs differs from generate_handler!");

        let capability: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json")).unwrap();
        let granted = capability["permissions"].as_array().unwrap();
        for command in handled {
            let permission = format!("allow-{}", command.replace('_', "-"));
            assert!(
                granted.iter().any(|p| p == permission.as_str()),
                "{permission} isn't granted to the main window"
            );
        }
    }

    /// A stub handler, because the real commands run on the Wry runtime. The
    /// ACL it sits behind is the app's own.
    mod acl {
        use std::sync::atomic::{AtomicBool, Ordering};

        use serde_json::Value;
        use tauri::ipc::{CallbackFn, InvokeBody};
        use tauri::test::{INVOKE_KEY, get_ipc_response, mock_builder};
        use tauri::webview::InvokeRequest;
        use tauri::{WebviewUrl, WebviewWindowBuilder};

        static RAN: AtomicBool = AtomicBool::new(false);

        fn call(label: &str) -> Result<tauri::ipc::InvokeResponseBody, Value> {
            let app = mock_builder()
                .invoke_handler(|invoke| {
                    RAN.store(true, Ordering::SeqCst);
                    invoke.resolver.resolve(());
                    true
                })
                .build(tauri::generate_context!(test = true))
                .unwrap();
            let window = WebviewWindowBuilder::new(&app, label, WebviewUrl::default())
                .build()
                .unwrap();
            get_ipc_response(
                &window,
                InvokeRequest {
                    cmd: "device_restore".into(),
                    callback: CallbackFn(0),
                    error: CallbackFn(1),
                    url: "tauri://localhost".parse().unwrap(),
                    body: InvokeBody::default(),
                    headers: tauri::http::HeaderMap::default(),
                    invoke_key: INVOKE_KEY.to_string(),
                },
            )
        }

        #[test]
        fn only_the_main_window_reaches_device_commands() {
            let refused = call("reader-1").unwrap_err();
            assert!(refused.to_string().contains("not allowed"), "{refused}");
            assert!(!RAN.load(Ordering::SeqCst), "the command ran");

            assert!(call("main").is_ok());
            assert!(RAN.load(Ordering::SeqCst), "the command didn't run");
        }
    }
}
