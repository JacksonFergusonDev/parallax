//! Kindle adapter: finds Kindles for `list_devices`, routes device commands
//! for a Kindle's mount (or `mtp:` key) to `parallax_core::kindle`, and the
//! Send to Kindle email commands. No business logic here.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use parallax_core::kindle::{self as core_kindle, KINDLE_FREE_FROM_PHOTOS, KINDLE_PAGE_NUMBERS};
use parallax_core::kindle_email::{self, EmailSettings};
use parallax_core::send::{Progress, SendOutcome};
use parallax_device::{Capabilities, DeviceInfo, DeviceKind, Transport};
use parallax_device_kindle::detect::{self, Unavailable};
use parallax_device_kindle::{
    Connections, KindleBook, KindleDevice, MTP_PREFIX, MetaCache, models,
};
use parallax_library::Library;
use serde::{Deserialize, Serialize};
use tauri::ipc::{Channel, JavaScriptChannelId};
use tauri::{AppHandle, Manager, State, Webview};

use crate::{AppState, CmdResult, err, settings};

/// Kindles and what's been read from them this session.
#[derive(Default)]
pub struct Kindles {
    pub connections: Connections,
    /// Book headers by Kindle serial, so a scan only reads new files.
    cache: Mutex<HashMap<String, MetaCache>>,
}

impl Kindles {
    fn scan(&self, device: &KindleDevice) -> parallax_device_kindle::Result<Vec<KindleBook>> {
        let mut caches = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let cache = caches.entry(device.serial().to_owned()).or_default();
        device.books(cache)
    }

    fn with_cache<T>(&self, device: &KindleDevice, f: impl FnOnce(&mut MetaCache) -> T) -> T {
        let mut caches = self
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(caches.entry(device.serial().to_owned()).or_default())
    }
}

/// The Kindle at `mount` (a drive) or `mtp:<serial>`, if that's what it is.
pub fn open(kindles: &Kindles, mount: &Path) -> Option<KindleDevice> {
    let key = mount.to_string_lossy();
    if key.starts_with(MTP_PREFIX) {
        return kindles.connections.get(&key);
    }
    if !mount.join(".kobo").is_dir() && detect::is_kindle_drive(mount, true) {
        return detect::open_drive(mount).ok();
    }
    None
}

/// Whether `mount` names a Kindle rather than a Kobo.
pub fn is_kindle(mount: &Path) -> bool {
    mount.to_string_lossy().starts_with(MTP_PREFIX)
        || (!mount.join(".kobo").is_dir() && detect::is_kindle_drive(mount, true))
}

fn unavailable_info(u: &Unavailable) -> DeviceInfo {
    let (model, name) = detect::model_for(&u.serial, u.product.as_deref());
    DeviceInfo {
        kind: DeviceKind::Kindle,
        model: name,
        firmware: String::new(),
        serial: u.serial.clone(),
        mount: PathBuf::from(format!("{MTP_PREFIX}{}", u.serial)),
        transport: Transport::Mtp,
        capabilities: Capabilities {
            native_collections: false,
            series: false,
            reading_progress: false,
            reading_time: false,
        },
        screen: if models::from_serial(&u.serial).is_some() {
            model.screen()
        } else {
            models::UNKNOWN.screen()
        },
        read_only: None,
        tested: false,
        unavailable: Some(u.reason.clone()),
    }
}

/// Kindles on drives under `volumes` and over MTP.
pub fn detect(kindles: &Kindles, volumes: &Path, free_from_photos: bool) -> Vec<DeviceInfo> {
    let mut found: Vec<DeviceInfo> = detect::drives(volumes)
        .iter()
        .map(KindleDevice::info)
        .collect();
    let (devices, unavailable) = kindles.connections.refresh(free_from_photos);
    found.extend(devices.iter().map(KindleDevice::info));
    found.extend(unavailable.iter().map(unavailable_info));
    found
}

pub fn free_from_photos(state: &AppState) -> bool {
    !settings::is(state, KINDLE_FREE_FROM_PHOTOS, "off")
}

fn page_numbers(library: &Library) -> bool {
    library
        .setting(KINDLE_PAGE_NUMBERS)
        .ok()
        .flatten()
        .as_deref()
        != Some("off")
}

fn lock(library: &Mutex<Library>) -> std::sync::MutexGuard<'_, Library> {
    library
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// A book in the device view, with its cover cached on this computer.
#[derive(Serialize)]
pub struct KindleBookView {
    #[serde(flatten)]
    entry: parallax_core::device::CatalogBook,
    cover: Option<PathBuf>,
}

pub fn device_books(
    kindles: &Kindles,
    device: &KindleDevice,
    cover_cache: &Path,
) -> CmdResult<Vec<KindleBookView>> {
    let books = kindles.scan(device).map_err(err)?;
    let _ = std::fs::create_dir_all(cover_cache);
    Ok(books
        .iter()
        .zip(core_kindle::catalog(&books))
        .map(|(book, entry)| {
            let cover = book.thumbnail_name().and_then(|name| {
                let file = cover_cache.join(format!("kindle-{name}"));
                if file.is_file() {
                    return Some(file);
                }
                let jpeg = device.thumbnail(book)?;
                std::fs::write(&file, jpeg).ok()?;
                Some(file)
            });
            KindleBookView { entry, cover }
        })
        .collect())
}

pub fn books_on_device(
    kindles: &Kindles,
    library: &Mutex<Library>,
    device: &KindleDevice,
) -> CmdResult<Vec<parallax_core::device::OnDevice>> {
    let books = kindles.scan(device).map_err(err)?;
    core_kindle::books_on_device(&lock(library), device, &books).map_err(err)
}

fn converter() -> CmdResult<parallax_convert::amazon::ChildProcess> {
    Ok(parallax_convert::amazon::ChildProcess::current()
        .map_err(err)?
        .to_azw3())
}

pub fn send(
    library: &Mutex<Library>,
    device: &KindleDevice,
    ids: &[i64],
    update: bool,
    progress: Option<&Channel<Progress>>,
) -> CmdResult<Vec<SendOutcome>> {
    let lib = lock(library);
    let converter = converter()?;
    let options = core_kindle::SendOptions {
        converter: &converter,
        page_numbers: page_numbers(&lib),
    };
    let report = |event| {
        if let Some(channel) = progress {
            let _ = channel.send(event);
        }
    };
    Ok(if update {
        core_kindle::update_on_device(&lib, device, &options, ids, report)
    } else {
        core_kindle::send_library_books(&lib, device, &options, ids, report)
    })
}

pub fn remove(
    library: &Mutex<Library>,
    device: &KindleDevice,
    paths: &[String],
    mut progress: impl FnMut(usize, usize),
) -> Vec<parallax_core::device::RemoveOutcome> {
    let lib = lock(library);
    core_kindle::remove_from_device(Some(&lib), device, paths, &mut progress)
}

pub fn import(
    kindles: &Kindles,
    library: &Mutex<Library>,
    device: &KindleDevice,
    paths: Option<&[String]>,
    progress: impl FnMut(usize, usize, &str),
) -> CmdResult<Vec<parallax_core::import::DeviceImportOutcome>> {
    let books = kindles.scan(device).map_err(err)?;
    let converter = parallax_convert::amazon::ChildProcess::current().map_err(err)?;
    let mut lib = lock(library);
    Ok(core_kindle::import_from_device(
        &mut lib, device, &books, paths, &converter, progress,
    ))
}

pub fn reconcile(
    kindles: &Kindles,
    library: &Mutex<Library>,
    device: &KindleDevice,
) -> CmdResult<core_kindle::Reconciled> {
    let mut lib = lock(library);
    kindles
        .with_cache(device, |cache| {
            core_kindle::reconcile(&mut lib, device, cache)
        })
        .map_err(err)
}

pub fn eject(kindles: &Kindles, device: &KindleDevice) -> CmdResult<()> {
    if device.transport() == Transport::Mtp {
        kindles.connections.eject(device.key()).map_err(err)
    } else {
        device.eject().map_err(err)
    }
}

/// What a Kindle report holds: no titles, serial or reading data.
#[derive(Serialize)]
struct KindleReport {
    format: u32,
    model: String,
    firmware: String,
    transport: Transport,
    books: usize,
    extensions: std::collections::BTreeMap<String, usize>,
    sidecar_folders: usize,
    has_clippings: bool,
    has_thumbnails_folder: bool,
    has_collections_json: bool,
}

pub fn report(kindles: &Kindles, device: &KindleDevice, path: &Path) -> CmdResult<()> {
    let books = kindles.scan(device).map_err(err)?;
    let storage = device.storage();
    let mut extensions = std::collections::BTreeMap::new();
    for b in &books {
        *extensions.entry(b.extension()).or_insert(0) += 1;
    }
    let report = KindleReport {
        format: 1,
        model: device.model_name().to_owned(),
        firmware: device.info().firmware,
        transport: device.transport(),
        books: books.len(),
        extensions,
        sidecar_folders: books
            .iter()
            .filter(|b| b.reading != parallax_device_kindle::krds::ReadingData::default())
            .count(),
        has_clippings: storage
            .exists(parallax_device_kindle::CLIPPINGS_FILE)
            .unwrap_or(false),
        has_thumbnails_folder: storage
            .is_dir(parallax_device_kindle::thumbnails::THUMBNAILS_DIR)
            .unwrap_or(false),
        has_collections_json: storage
            .exists(parallax_device_kindle::collections::COLLECTIONS_FILE)
            .unwrap_or(false),
    };
    let json = serde_json::to_string_pretty(&report).map_err(err)?;
    std::fs::write(path, json + "\n").map_err(err)
}

/// Email settings for the Settings window, with whether a password is saved.
#[derive(Serialize)]
pub struct EmailSettingsView {
    #[serde(flatten)]
    settings: EmailSettings,
    password_saved: bool,
}

#[tauri::command]
pub fn kindle_email_settings(state: State<AppState>) -> EmailSettingsView {
    let settings = EmailSettings::load(&state.library());
    EmailSettingsView {
        password_saved: kindle_email::has_password(settings.user.trim()),
        settings,
    }
}

#[derive(Deserialize)]
pub struct EmailSettingsUpdate {
    #[serde(flatten)]
    settings: EmailSettings,
    /// A new password; `None` keeps the saved one, empty removes it.
    password: Option<String>,
}

#[tauri::command]
pub async fn kindle_email_settings_set(
    state: State<'_, AppState>,
    update: EmailSettingsUpdate,
) -> CmdResult<()> {
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = lock(&library);
        let old_user = EmailSettings::load(&lib).user;
        update.settings.save(&lib).map_err(err)?;
        let user = update.settings.user.trim().to_owned();
        if let Some(password) = update.password {
            kindle_email::set_password(&user, &password)?;
        }
        // The password belongs to the old account name; don't leave it behind.
        if !old_user.trim().is_empty() && old_user.trim() != user {
            let _ = kindle_email::set_password(old_user.trim(), "");
        }
        Ok(())
    })
    .await
    .map_err(err)?
}

/// Emails library books to the Kindle address in Settings.
#[tauri::command]
pub async fn kindle_email_send(
    webview: Webview,
    progress: Option<JavaScriptChannelId>,
    state: State<'_, AppState>,
    ids: Vec<i64>,
) -> CmdResult<Vec<SendOutcome>> {
    let progress: Option<Channel<Progress>> = progress.map(|id| id.channel_on(webview));
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let lib = lock(&library);
        let settings = EmailSettings::load(&lib);
        kindle_email::send_books(&lib, &settings, &ids, |event| {
            if let Some(channel) = &progress {
                let _ = channel.send(event);
            }
        })
    })
    .await
    .map_err(err)?
}

/// Where Kindle covers are cached on this computer.
pub fn cover_cache(app: &AppHandle) -> CmdResult<PathBuf> {
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(err)?
        .join("device-covers"))
}

pub type Shared = Arc<Kindles>;
