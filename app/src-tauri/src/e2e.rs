//! Hooks for `scripts/reader-e2e.mjs`, compiled only with the `e2e` feature.
//!
//! - `PARALLAX_E2E_OPEN=<id>`: open that book in a reader and no main window. The
//!   main window writes to a connected Kobo by itself, which a test must not.
//! - `PARALLAX_E2E_SETTINGS=1`: open the Settings window instead, also alone.
//! - `PARALLAX_E2E_STEPS`: scripts to run in that window, separated by `@@`, two
//!   seconds apart. They stand in for a person turning pages. A step
//!   `menu:<id>` chooses that menu command instead.
//! - `PARALLAX_E2E_SECS`: how long to wait after the last step before quitting.
//! - `PARALLAX_E2E_WAIT=1`: treat `PARALLAX_E2E_SECS` as a backstop instead. Quit
//!   once a step sets `location.hash` to `#e2e-done`, or exit with status 1 and
//!   the message when it sets `#e2e-failed:<encoded message>` or the backstop
//!   passes. The native window title doesn't follow `document.title`.
//! - `PARALLAX_E2E_LOOSE=1`: serve the reader page without its CSP and with a loose
//!   one in its place, so the test can show its detector catches scripts.

use std::borrow::Cow;
use std::thread::sleep;
use std::time::{Duration, Instant};

use tauri::{App, Context, Manager, Runtime, WebviewWindowBuilder};

use crate::{AppState, reader, settings};

const LOOSE_CSP: &str = "default-src * 'unsafe-inline' 'unsafe-eval' data: blob: ipc:; \
    script-src * 'unsafe-inline' 'unsafe-eval' data: blob:; \
    connect-src * ipc: http://ipc.localhost";

fn book() -> Option<i64> {
    std::env::var("PARALLAX_E2E_OPEN").ok()?.parse().ok()
}

fn settings_window() -> bool {
    std::env::var_os("PARALLAX_E2E_SETTINGS").is_some()
}

pub fn prepare<R: Runtime>(context: &mut Context<R>) {
    if book().is_some() || settings_window() {
        context.config_mut().app.windows.clear();
    }
}

pub fn start(app: &App) {
    let book = book();
    if book.is_none() && !settings_window() {
        return;
    }
    let app = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        let label = if let Some(id) = book {
            let opened = reader::reader_open(app.clone(), app.state::<AppState>(), id).await;
            eprintln!("e2e: reader_open {opened:?}");
            format!("reader-{id}")
        } else {
            eprintln!("e2e: settings {:?}", settings::open_window(&app));
            "settings".to_owned()
        };
        let steps = std::env::var("PARALLAX_E2E_STEPS").unwrap_or_default();
        let secs = std::env::var("PARALLAX_E2E_SECS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5);
        tauri::async_runtime::spawn_blocking(move || {
            sleep(Duration::from_secs(4));
            for step in steps.split("@@").filter(|s| !s.is_empty()) {
                if let Some(id) = step.strip_prefix("menu:") {
                    crate::menu::run(&app, id);
                    eprintln!("e2e: menu {id}");
                } else if let Some(window) = app.get_webview_window(&label) {
                    eprintln!("e2e: step {:?}", window.eval(step).is_ok());
                }
                sleep(Duration::from_secs(2));
            }
            if std::env::var_os("PARALLAX_E2E_WAIT").is_some() {
                let deadline = Instant::now() + Duration::from_secs(secs);
                let code = loop {
                    let hash = app
                        .get_webview_window(&label)
                        .and_then(|w| w.url().ok())
                        .and_then(|url| url.fragment().map(str::to_owned))
                        .unwrap_or_default();
                    if hash == "e2e-done" {
                        // Let the reader save what the last step did.
                        sleep(Duration::from_secs(2));
                        break 0;
                    }
                    if let Some(message) = hash.strip_prefix("e2e-failed:") {
                        eprintln!("e2e: step failed: {}", percent_decode(message));
                        break 1;
                    }
                    if Instant::now() >= deadline {
                        eprintln!("e2e: steps did not finish within {secs}s");
                        break 1;
                    }
                    sleep(Duration::from_millis(200));
                };
                app.exit(code);
                return;
            }
            sleep(Duration::from_secs(secs));
            app.exit(0);
        });
    });
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn reader_window<R: Runtime, M: Manager<R>>(
    builder: WebviewWindowBuilder<'_, R, M>,
) -> WebviewWindowBuilder<'_, R, M> {
    if std::env::var_os("PARALLAX_E2E_LOOSE").is_none() {
        return builder;
    }
    builder.on_web_resource_request(|request, response| {
        if !request.uri().path().ends_with("reader.html") {
            return;
        }
        let page = String::from_utf8_lossy(response.body()).into_owned();
        let page = page.replacen(
            "http-equiv=\"Content-Security-Policy\"",
            "name=\"removed-csp\"",
            1,
        );
        *response.body_mut() = Cow::Owned(page.into_bytes());
        response
            .headers_mut()
            .insert("content-security-policy", LOOSE_CSP.parse().unwrap());
    })
}
