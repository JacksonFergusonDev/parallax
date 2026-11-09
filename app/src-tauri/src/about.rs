use serde::Serialize;
use tauri::{AppHandle, Emitter, EventTarget, Manager, State, WebviewUrl, WebviewWindowBuilder};

use crate::{AppState, CmdResult, err, menu::REPOSITORY, settings::AppTheme};

pub const LABEL: &str = "about";

pub fn open_window(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(LABEL) {
        if window.is_visible()? {
            window.set_focus()?;
        }
        return Ok(());
    }
    WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("about.html".into()))
        .title("About Parallax")
        .inner_size(480.0, 430.0)
        .center()
        .resizable(false)
        .minimizable(false)
        .maximizable(false)
        .visible(false)
        .build()?;
    Ok(())
}

pub fn push_theme(app: &AppHandle, theme: AppTheme) {
    let _ = app.emit_to(
        EventTarget::webview_window(LABEL),
        "app-theme-changed",
        theme,
    );
}

#[derive(Serialize)]
pub struct AboutInfo {
    version: String,
    theme: AppTheme,
}

#[tauri::command]
pub fn about_info(app: AppHandle, state: State<AppState>) -> AboutInfo {
    AboutInfo {
        version: app.package_info().version.to_string(),
        theme: AppTheme::saved(&state),
    }
}

#[tauri::command]
pub fn about_open_repository() -> CmdResult<()> {
    std::process::Command::new("open")
        .arg(REPOSITORY)
        .status()
        .map_err(err)?
        .success()
        .then_some(())
        .ok_or_else(|| "Could not open Parallax on GitHub.".to_owned())
}
