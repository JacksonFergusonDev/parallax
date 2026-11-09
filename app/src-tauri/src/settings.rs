//! The Settings window (Parallax › Settings…, ⌘,) and the app-wide settings
//! it changes. Values live in the library's settings table, except the backup
//! policy, which sits beside the backups so every write session applies it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use parallax_core::send::{
    COMIC_AUTOCONTRAST, COMIC_COLOUR_CONTRAST, COMIC_COLOUR_SEPARATION, COMIC_CROP, COMIC_DITHER,
    COMIC_MIDTONE_DARKENING, COMIC_SPREADS, COMIC_STRENGTHS, COMIC_UPSCALE,
};
use parallax_device_kobo::{BackupPolicy, backups_size, delete_all_backups};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, EventTarget, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::reader::{self, ReaderSettings};
use crate::{AppState, CmdResult, about, err};

pub const THEME: &str = "theme";
/// Which view the main window opens on: the library unless this says otherwise.
pub const OPEN_VIEW: &str = "open_view";
/// The view the main window last showed, for `open_view = last`.
pub const LAST_VIEW: &str = "last_view";
/// `one` to read in a single window, replacing the book in it.
pub const READER_WINDOWS: &str = "reader_windows";
/// `off` to leave series alone when a Kobo connects.
pub const KOBO_SERIES: &str = "kobo_series";
/// `off` to stop grouping rules running on their own after sends and on connect.
pub const KOBO_RULES: &str = "kobo_rules";
/// `on` to eject the Kobo once books are sent.
pub const EJECT_AFTER_SEND: &str = "eject_after_send";

/// The event main and Settings windows get when a setting changes.
const CHANGED: &str = "setting-changed";
/// The windows that may hear about settings: never a reader.
const TRUSTED: [&str; 2] = ["main", SETTINGS_LABEL];
const SETTINGS_LABEL: &str = "settings";

enum Allowed {
    /// Free text up to this many characters.
    Text(usize),
    /// One of these; no value means the default.
    OneOf(&'static [&'static str]),
}

/// Settings the app may read and change, and what each may be.
const SETTINGS: [(&str, Allowed); 19] = [
    (parallax_core::enhance::GOOGLE_BOOKS_KEY, Allowed::Text(200)),
    (parallax_core::enhance::METRON_TOKEN, Allowed::Text(256)),
    (
        parallax_core::review::REVIEW_ON_ADD,
        Allowed::OneOf(&["off"]),
    ),
    (
        THEME,
        Allowed::OneOf(&["light", "dark", "sepia", "contrast"]),
    ),
    (OPEN_VIEW, Allowed::OneOf(&["reading", "last"])),
    (LAST_VIEW, Allowed::OneOf(&["library", "device", "reading"])),
    (READER_WINDOWS, Allowed::OneOf(&["one"])),
    (KOBO_SERIES, Allowed::OneOf(&["off"])),
    (KOBO_RULES, Allowed::OneOf(&["off"])),
    (EJECT_AFTER_SEND, Allowed::OneOf(&["on"])),
    (parallax_library::COMICS_KIND, Allowed::OneOf(&["comic"])),
    (
        COMIC_SPREADS,
        Allowed::OneOf(&["keep_whole", "whole_then_halves"]),
    ),
    (COMIC_CROP, Allowed::OneOf(&["on"])),
    (COMIC_AUTOCONTRAST, Allowed::OneOf(&["on"])),
    (COMIC_UPSCALE, Allowed::OneOf(&["on"])),
    (COMIC_DITHER, Allowed::OneOf(&["off"])),
    (COMIC_COLOUR_SEPARATION, Allowed::OneOf(&COMIC_STRENGTHS)),
    (COMIC_COLOUR_CONTRAST, Allowed::OneOf(&["off"])),
    (COMIC_MIDTONE_DARKENING, Allowed::OneOf(&COMIC_STRENGTHS)),
];

/// Settings that are preferences, which Reset puts back. Credentials stay.
const PREFERENCES: [&str; 17] = [
    parallax_core::review::REVIEW_ON_ADD,
    THEME,
    OPEN_VIEW,
    LAST_VIEW,
    READER_WINDOWS,
    KOBO_SERIES,
    KOBO_RULES,
    EJECT_AFTER_SEND,
    parallax_library::COMICS_KIND,
    COMIC_SPREADS,
    COMIC_CROP,
    COMIC_AUTOCONTRAST,
    COMIC_UPSCALE,
    COMIC_DITHER,
    COMIC_COLOUR_SEPARATION,
    COMIC_COLOUR_CONTRAST,
    COMIC_MIDTONE_DARKENING,
];

/// `value` trimmed, with empty meaning the default, if `key` may hold it.
fn checked(key: &str, value: Option<String>) -> CmdResult<Option<String>> {
    let (_, allowed) = SETTINGS
        .iter()
        .find(|(k, _)| *k == key)
        .ok_or_else(|| format!("unknown setting {key}"))?;
    let value = value.map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    match (allowed, &value) {
        (_, None) => Ok(None),
        (Allowed::Text(max), Some(v)) if v.chars().count() <= *max => Ok(value),
        (Allowed::OneOf(values), Some(v)) if values.contains(&v.as_str()) => Ok(value),
        _ => Err(format!("{key} can't be set to that")),
    }
}

/// The whole app's appearance. Readers follow it unless they choose otherwise.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppTheme {
    /// Light or dark as macOS is.
    #[default]
    System,
    Light,
    Dark,
    Sepia,
    /// Stronger text and borders, light or dark as macOS is.
    Contrast,
}

impl AppTheme {
    fn parse(value: Option<&str>) -> Self {
        match value {
            Some("light") => Self::Light,
            Some("dark") => Self::Dark,
            Some("sepia") => Self::Sepia,
            Some("contrast") => Self::Contrast,
            _ => Self::System,
        }
    }

    /// The saved theme.
    pub fn saved(state: &AppState) -> Self {
        Self::parse(state.library().setting(THEME).ok().flatten().as_deref())
    }

    /// The window appearance macOS draws title bars and controls in, which
    /// is also what `prefers-color-scheme` reports to every window.
    fn native(self) -> Option<tauri::Theme> {
        match self {
            Self::Light | Self::Sepia => Some(tauri::Theme::Light),
            Self::Dark => Some(tauri::Theme::Dark),
            Self::System | Self::Contrast => None,
        }
    }
}

/// Puts the saved theme on the app's windows.
pub fn apply_theme(app: &AppHandle, theme: AppTheme) {
    app.set_theme(theme.native());
}

/// Tells the main and Settings windows, and sends theme changes to About and readers.
fn announce(app: &AppHandle, key: &str, value: Option<&str>) {
    #[derive(Clone, Serialize)]
    struct Changed<'a> {
        key: &'a str,
        value: Option<&'a str>,
    }
    for label in TRUSTED {
        let _ = app.emit_to(
            EventTarget::webview_window(label),
            CHANGED,
            Changed { key, value },
        );
    }
    if key == THEME {
        let theme = AppTheme::parse(value);
        apply_theme(app, theme);
        about::push_theme(app, theme);
        reader::push_app_theme(app, theme);
    }
}

#[tauri::command]
pub fn setting_get(state: State<AppState>, key: String) -> CmdResult<Option<String>> {
    checked(&key, None)?;
    state
        .library()
        .setting(&key)
        .map(|value| visible_setting(&key, value))
        .map_err(err)
}

fn visible_setting(key: &str, value: Option<String>) -> Option<String> {
    if key == parallax_core::enhance::METRON_TOKEN {
        value.map(|_| "configured".to_owned())
    } else {
        value
    }
}

#[tauri::command]
pub fn setting_set(
    app: AppHandle,
    state: State<AppState>,
    key: String,
    value: Option<String>,
) -> CmdResult<()> {
    let value = checked(&key, value)?;
    state
        .library()
        .set_setting(&key, value.as_deref())
        .map_err(err)?;
    announce(&app, &key, visible_setting(&key, value).as_deref());
    Ok(())
}

/// Every setting, for the Settings window to start from.
#[tauri::command]
pub fn settings_all(state: State<AppState>) -> CmdResult<BTreeMap<String, Option<String>>> {
    let lib = state.library();
    SETTINGS
        .iter()
        .map(|(key, _)| {
            Ok((
                (*key).to_owned(),
                visible_setting(key, lib.setting(key).map_err(err)?),
            ))
        })
        .collect()
}

/// Puts every preference back to its default: the reader's appearance and
/// the backup policy too. Credentials stay.
#[tauri::command]
pub async fn settings_reset(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    {
        let lib = state.library();
        for key in PREFERENCES {
            lib.set_setting(key, None).map_err(err)?;
        }
    }
    for key in PREFERENCES {
        announce(&app, key, None);
    }
    reader::store_settings(&app, &state, ReaderSettings::default(), None).await?;
    let root = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || BackupPolicy::default().save(&root))
        .await
        .map_err(err)?
        .map_err(err)
}

#[derive(Serialize)]
pub struct BackupSettings {
    policy: BackupPolicy,
    /// What the backup folder takes now.
    used_bytes: u64,
}

fn backup_settings_at(root: PathBuf) -> BackupSettings {
    BackupSettings {
        policy: BackupPolicy::load(&root),
        used_bytes: backups_size(&root),
    }
}

#[tauri::command]
pub async fn backup_settings(state: State<'_, AppState>) -> CmdResult<BackupSettings> {
    let root = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || backup_settings_at(root))
        .await
        .map_err(err)
}

/// Saves the policy and applies it now, which may delete older backups.
#[tauri::command]
pub async fn backup_settings_set(
    state: State<'_, AppState>,
    policy: BackupPolicy,
) -> CmdResult<BackupSettings> {
    if policy.max_bytes.is_some_and(|max| max < 1_000_000) {
        return Err("the backup limit must be at least 1 MB".to_owned());
    }
    let root = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        policy.save(&root).map_err(err)?;
        Ok(backup_settings_at(root))
    })
    .await
    .map_err(err)?
}

/// Deletes every backup. The Settings window confirms first.
#[tauri::command]
pub async fn backup_delete_all(state: State<'_, AppState>) -> CmdResult<BackupSettings> {
    let root = state.backup_root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        delete_all_backups(&root).map_err(err)?;
        Ok(backup_settings_at(root))
    })
    .await
    .map_err(err)?
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Folder {
    Library,
    Backups,
    Data,
}

/// Opens one of the app's own folders in Finder.
#[tauri::command]
pub fn show_folder(app: AppHandle, state: State<AppState>, folder: Folder) -> CmdResult<()> {
    let data = app.path().app_data_dir().map_err(err)?;
    let path = match folder {
        Folder::Library => data.join("library"),
        Folder::Backups => state.backup_root.clone(),
        Folder::Data => data,
    };
    std::fs::create_dir_all(&path).map_err(err)?;
    std::process::Command::new("open")
        .arg(&path)
        .status()
        .map_err(err)?;
    Ok(())
}

/// The appearance new and open books use.
#[tauri::command]
pub fn reader_settings_get(state: State<AppState>) -> ReaderSettings {
    reader::saved_settings(&state.library())
}

#[tauri::command]
pub async fn reader_settings_set(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: ReaderSettings,
) -> CmdResult<()> {
    reader::store_settings(&app, &state, settings, None).await
}

/// Opens the Settings window, or brings it forward.
pub fn open_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.unminimize()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, SETTINGS_LABEL, WebviewUrl::App("settings.html".into()))
        .title("Settings")
        .inner_size(680.0, 600.0)
        .min_inner_size(560.0, 460.0)
        .minimizable(false)
        .maximizable(false)
        // Shown once its settings are on screen, like the main window.
        .visible(false)
        .build()?;
    Ok(())
}

/// Whether `key` is set to `value`; an unreadable setting is its default.
pub fn is(state: &AppState, key: &str, value: &str) -> bool {
    state.library().setting(key).ok().flatten().as_deref() == Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_hold_only_what_they_allow() {
        assert_eq!(
            checked(THEME, Some(" sepia ".into())),
            Ok(Some("sepia".into()))
        );
        assert_eq!(checked(THEME, Some(String::new())), Ok(None));
        assert!(checked(THEME, Some("neon".into())).is_err());
        assert!(checked("password", Some("x".into())).is_err());
        assert!(
            checked(
                parallax_core::enhance::GOOGLE_BOOKS_KEY,
                Some("k".repeat(201))
            )
            .is_err()
        );
        assert_eq!(
            checked(KOBO_RULES, Some("off".into())),
            Ok(Some("off".into()))
        );
        assert!(checked(KOBO_RULES, Some("on".into())).is_err());
    }

    #[test]
    fn comic_strengths_are_percentages_the_pipeline_reads() {
        for key in [COMIC_COLOUR_SEPARATION, COMIC_MIDTONE_DARKENING] {
            for value in COMIC_STRENGTHS {
                assert_eq!(checked(key, Some(value.into())), Ok(Some(value.into())));
                assert!(value.parse::<u8>().is_ok_and(|v| v <= 200));
            }
            assert!(checked(key, Some("100".into())).is_err(), "unset is 100");
            assert!(checked(key, Some("300".into())).is_err());
        }
    }

    #[test]
    fn every_comic_page_setting_is_a_preference() {
        for key in parallax_core::send::COMIC_PAGE_SETTINGS {
            assert!(PREFERENCES.contains(&key), "{key}");
        }
    }

    #[test]
    fn every_preference_is_a_setting() {
        for key in PREFERENCES {
            assert!(SETTINGS.iter().any(|(k, _)| *k == key), "{key}");
        }
    }

    #[test]
    fn themes_set_the_native_appearance() {
        assert_eq!(
            AppTheme::parse(Some("sepia")).native(),
            Some(tauri::Theme::Light)
        );
        assert_eq!(
            AppTheme::parse(Some("dark")).native(),
            Some(tauri::Theme::Dark)
        );
        assert_eq!(AppTheme::parse(Some("contrast")).native(), None);
        assert_eq!(AppTheme::parse(None), AppTheme::System);
    }
}
