//! Tauri adapter: thin commands over the core crates. No business logic here.

use std::path::{Path, PathBuf};

use erc_convert::Kepubify;
use erc_core::send::{SendOutcome, send_books};
use erc_device::DeviceInfo;
use erc_device_kobo::{KoboDevice, MACOS_VOLUMES};
use tauri::{AppHandle, Emitter};

/// Event carrying [`erc_core::send::Progress`] while books are sent.
const SEND_PROGRESS: &str = "send-progress";

#[tauri::command]
fn app_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[tauri::command]
fn list_devices() -> Vec<DeviceInfo> {
    KoboDevice::detect(Path::new(MACOS_VOLUMES))
        .iter()
        .map(KoboDevice::info)
        .collect()
}

#[tauri::command]
async fn send_to_device(
    app: AppHandle,
    mount: PathBuf,
    files: Vec<PathBuf>,
) -> Result<Vec<SendOutcome>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let kobo = KoboDevice::open(&mount).map_err(|e| e.to_string())?;
        let converter = Kepubify::locate().map_err(|e| e.to_string())?;
        Ok(send_books(&kobo, &converter, &files, |progress| {
            let _ = app.emit(SEND_PROGRESS, progress);
        }))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn eject_device(mount: PathBuf) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        KoboDevice::open(&mount)
            .and_then(|kobo| kobo.eject())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            app_version,
            list_devices,
            send_to_device,
            eject_device
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
