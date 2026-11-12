//! New versions of Parallax. Each release publishes `latest.json`, signed
//! update archives and their signatures (`.github/workflows/release.yml`);
//! the updater plugin checks the latest one against the public key in
//! `tauri.conf.json`, so only a build signed with the release key installs.
//!
//! Parallax checks a few seconds after launch and then daily, unless
//! `updates_auto` is `off`, and **Parallax › Check for Updates…** checks now.
//! It never installs on its own: the main window shows a banner, and the
//! update installs and Parallax restarts only when the user says so. With
//! `updates_download` on, an automatic check downloads the update first, so
//! the banner offers a restart and installing is quick.

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, State};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{AppState, CmdResult, err, menu, settings};

/// `off` to stop checking for updates on launch and every day.
pub const AUTO: &str = "updates_auto";
/// `on` to download updates an automatic check finds before offering them.
pub const DOWNLOAD: &str = "updates_download";
/// The version the user chose to skip. A newer one is still offered.
pub const SKIPPED: &str = "updates_skipped";

/// The event the main window gets when an update is found.
const AVAILABLE: &str = "update-available";
/// The event the main window gets as an update downloads.
const PROGRESS: &str = "update-progress";
const MAIN: &str = "main";
const FIRST_CHECK: Duration = Duration::from_secs(8);
const CHECK_EVERY: Duration = Duration::from_hours(24);

/// The update found by the last check, until it's installed or skipped.
#[derive(Default)]
pub struct Pending(Mutex<Option<Found>>);

/// An update, and its archive once it's downloaded ahead of installing.
pub struct Found {
    update: Update,
    downloaded: Option<Vec<u8>>,
}

impl Found {
    fn available(&self) -> Available {
        Available {
            ready: self.downloaded.is_some(),
            ..Available::from(&self.update)
        }
    }
}

impl Pending {
    fn lock(&self) -> MutexGuard<'_, Option<Found>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// What the main window shows about an update.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub version: String,
    pub current: String,
    /// The release notes, in Markdown.
    pub notes: Option<String>,
    /// Downloaded already, so installing only needs a restart.
    pub ready: bool,
}

impl From<&Update> for Available {
    fn from(update: &Update) -> Self {
        Self {
            version: update.version.clone(),
            current: update.current_version.clone(),
            notes: update.body.clone().filter(|b| !b.trim().is_empty()),
            ready: false,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Progress {
    downloaded: u64,
    total: Option<u64>,
}

/// Checks after launch and then daily, while `updates_auto` allows. A debug
/// build never checks on its own: its version isn't a release.
pub fn start(app: &AppHandle) {
    if cfg!(debug_assertions) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CHECK).await;
        loop {
            if setting(&app, AUTO).as_deref() != Some("off") {
                check(&app, false).await;
            }
            tokio::time::sleep(CHECK_EVERY).await;
        }
    });
}

fn setting(app: &AppHandle, key: &str) -> Option<String> {
    app.state::<AppState>()
        .library()
        .setting(key)
        .ok()
        .flatten()
}

/// Whether an automatic check should stay quiet about `version`.
fn skipped(skipped: Option<&str>, version: &str, manual: bool) -> bool {
    !manual && skipped == Some(version)
}

/// Whether an automatic check fetches the update before offering it.
fn downloads_first(download: Option<&str>, manual: bool) -> bool {
    !manual && download == Some("on")
}

/// Checks for a new version. A manual check (the menu item) brings the
/// library forward and says when Parallax is up to date or the check failed;
/// an automatic one only speaks up when there's something new it wasn't told
/// to skip, and with `updates_download` on, only once it's downloaded.
pub async fn check(app: &AppHandle, manual: bool) {
    let found = match app.updater() {
        Ok(updater) => updater.check().await,
        Err(error) => Err(error),
    };
    match found {
        Ok(Some(update)) => {
            if skipped(setting(app, SKIPPED).as_deref(), &update.version, manual) {
                return;
            }
            let mut downloaded = None;
            if downloads_first(setting(app, DOWNLOAD).as_deref(), manual) {
                let pending = app.state::<Pending>();
                let already = pending.lock().as_ref().is_some_and(|found| {
                    found.downloaded.is_some() && found.update.version == update.version
                });
                if already {
                    return;
                }
                // A failed download still offers the update, to download on request.
                downloaded = update
                    .download(|_, _| {}, || {})
                    .await
                    .inspect_err(|error| eprintln!("Couldn't download the update: {error}"))
                    .ok();
            }
            let found = Found { update, downloaded };
            let available = found.available();
            *app.state::<Pending>().lock() = Some(found);
            if manual {
                let _ = menu::show_library(app);
            }
            let _ = app.emit_to(EventTarget::webview_window(MAIN), AVAILABLE, available);
        }
        Ok(None) if manual => {
            let version = app.package_info().version.to_string();
            app.dialog()
                .message(format!(
                    "Version {version} is the newest version of Parallax."
                ))
                .title("Parallax is up to date")
                .kind(MessageDialogKind::Info)
                .show(|_| {});
        }
        Ok(None) => {}
        Err(error) if manual => {
            app.dialog()
                .message(format!(
                    "Parallax couldn't reach the update server. Check your internet connection and try again.\n\n{error}"
                ))
                .title("Couldn't check for updates")
                .kind(MessageDialogKind::Warning)
                .show(|_| {});
        }
        Err(error) => eprintln!("Couldn't check for updates: {error}"),
    }
}

/// The update waiting for the user, for a main window that opened after the
/// check found it.
#[tauri::command]
pub fn update_available(pending: State<Pending>) -> Option<Available> {
    pending.lock().as_ref().map(Found::available)
}

/// Installs the waiting update, downloading it first unless that's done,
/// then restarts Parallax.
#[tauri::command]
pub async fn update_install(app: AppHandle, pending: State<'_, Pending>) -> CmdResult<()> {
    let (update, ready) = pending
        .lock()
        .as_ref()
        .map(|found| (found.update.clone(), found.downloaded.clone()))
        .ok_or("There's no update waiting to install.")?;
    if let Some(bytes) = ready {
        update.install(bytes).map_err(err)?;
        app.restart()
    }
    let mut downloaded = 0u64;
    update
        .download_and_install(
            |chunk, total| {
                downloaded += chunk as u64;
                let _ = app.emit_to(
                    EventTarget::webview_window(MAIN),
                    PROGRESS,
                    Progress { downloaded, total },
                );
            },
            || {},
        )
        .await
        .map_err(err)?;
    app.restart()
}

/// Stops offering `version`. A later version is still offered.
#[tauri::command]
pub fn update_skip(
    app: AppHandle,
    state: State<AppState>,
    pending: State<Pending>,
    version: String,
) -> CmdResult<()> {
    settings::store(&app, &state, SKIPPED, Some(version))?;
    pending.lock().take();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{downloads_first, skipped};

    #[test]
    fn only_automatic_checks_download_and_only_when_asked() {
        assert!(downloads_first(Some("on"), false));
        assert!(!downloads_first(Some("on"), true));
        assert!(!downloads_first(None, false));
    }

    #[test]
    fn only_automatic_checks_respect_a_skipped_version() {
        assert!(skipped(Some("0.1.1"), "0.1.1", false));
        assert!(!skipped(Some("0.1.1"), "0.1.1", true));
        assert!(!skipped(Some("0.1.1"), "0.1.2", false));
        assert!(!skipped(None, "0.1.1", false));
    }
}
