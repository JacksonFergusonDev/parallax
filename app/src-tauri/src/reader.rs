//! Reader windows. A book can carry hostile scripts, so a window only ever
//! reaches the book in its own label, and never sees a file path.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use erc_epub::{BookFile, Entry, has_drm};
use erc_library::{HighlightDraft, ReaderHighlight, ReaderPosition};
use serde::{Deserialize, Serialize};
use tauri::ipc::Response;
use tauri::utils::config::BackgroundThrottlingPolicy;
use tauri::webview::NewWindowResponse;
use tauri::{
    AppHandle, Manager, State, Url, Webview, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

use crate::{AppState, CmdResult, err};

const LABEL_PREFIX: &str = "reader-";
const SETTINGS_KEY: &str = "reader";

/// Books open in reader windows, by book id.
#[derive(Default)]
pub struct OpenBooks(Mutex<HashMap<i64, Arc<Mutex<BookFile>>>>);

impl OpenBooks {
    fn lock(&self) -> MutexGuard<'_, HashMap<i64, Arc<Mutex<BookFile>>>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Theme {
    Auto,
    Light,
    Sepia,
    Dark,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Flow {
    #[default]
    Paginated,
    Scrolled,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Measure {
    Narrow,
    #[default]
    Medium,
    Wide,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Spread {
    /// Two pages side by side when the window is wide enough.
    #[default]
    Auto,
    Single,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Spacing {
    /// The publisher's line height.
    #[default]
    Book,
    Relaxed,
    Loose,
}

/// Each field falls back to its default, so settings saved by an older build
/// still load. Unknown values are refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReaderSettings {
    /// Percent of the book's own text size.
    font_size: u16,
    theme: Theme,
    flow: Flow,
    /// How long a line may get.
    width: Measure,
    margins: Measure,
    spread: Spread,
    spacing: Spacing,
}

impl Default for ReaderSettings {
    fn default() -> Self {
        Self {
            font_size: 100,
            theme: Theme::Auto,
            flow: Flow::default(),
            width: Measure::default(),
            margins: Measure::default(),
            spread: Spread::default(),
            spacing: Spacing::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
}

impl HighlightColor {
    fn name(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
        }
    }
}

/// A highlight as the reader sends it: typed and bounded, as it comes from
/// the same window as the book.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct HighlightInput {
    cfi: String,
    href: String,
    fraction: f64,
    text: String,
    note: Option<String>,
    color: HighlightColor,
}

const MAX_CFI: usize = 4_096;
const MAX_HIGHLIGHT_TEXT: usize = 20_000;
const MAX_NOTE: usize = 50_000;

impl HighlightInput {
    fn draft(self) -> CmdResult<HighlightDraft> {
        let too_long = self.cfi.len() > MAX_CFI
            || self.href.len() > MAX_CFI
            || self.text.chars().count() > MAX_HIGHLIGHT_TEXT
            || self
                .note
                .as_ref()
                .is_some_and(|n| n.chars().count() > MAX_NOTE);
        if too_long {
            return Err("that highlight is too long to keep".to_owned());
        }
        if !self.cfi.starts_with("epubcfi(") || !self.fraction.is_finite() {
            return Err("that isn't a place in the book".to_owned());
        }
        Ok(HighlightDraft {
            cfi: self.cfi,
            href: self.href,
            fraction: self.fraction,
            text: self.text,
            note: self.note,
            color: self.color.name().to_owned(),
        })
    }
}

#[derive(Serialize)]
pub struct ReaderBook {
    title: String,
    entries: Vec<Entry>,
    position: Option<ReaderPosition>,
    settings: ReaderSettings,
}

fn book_id(label: &str) -> CmdResult<i64> {
    label
        .strip_prefix(LABEL_PREFIX)
        .and_then(|id| id.parse().ok())
        .ok_or_else(|| "not a reader window".to_owned())
}

/// The reader's own page and the frames it makes. Everything else is a book
/// trying to take a frame somewhere else.
fn may_navigate(url: &Url) -> bool {
    match url.scheme() {
        "tauri" => url.host_str() == Some("localhost"),
        "blob" => url.path().starts_with("tauri://localhost/") || tauri::is_dev(),
        "about" => matches!(url.path(), "blank" | "srcdoc"),
        "http" => tauri::is_dev() && url.host_str() == Some("localhost"),
        _ => false,
    }
}

/// Opens `id` in its own window, or brings that window forward.
#[tauri::command]
pub async fn reader_open(app: AppHandle, state: State<'_, AppState>, id: i64) -> CmdResult<()> {
    let label = format!("{LABEL_PREFIX}{id}");
    if let Some(window) = app.get_webview_window(&label) {
        window.unminimize().map_err(err)?;
        return window.set_focus().map_err(err);
    }
    let library = state.library.clone();
    let title = tauri::async_runtime::spawn_blocking(move || -> CmdResult<String> {
        let lib = library.lock().unwrap_or_else(PoisonError::into_inner);
        let book = lib.book(id).map_err(err)?;
        if has_drm(&lib.book_file(id)).map_err(err)? {
            return Err(format!(
                "{} is DRM-protected, so it can't be read here",
                book.title
            ));
        }
        Ok(book.title)
    })
    .await
    .map_err(err)??;

    let builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::App("reader.html".into()))
        .title(title)
        .inner_size(860.0, 1000.0)
        .min_inner_size(420.0, 520.0)
        // Separate storage from the main window.
        .incognito(true)
        // A covered window otherwise stalls, and can stop altogether.
        .background_throttling(BackgroundThrottlingPolicy::Disabled)
        .on_navigation(may_navigate)
        .on_new_window(|_, _| NewWindowResponse::Deny);
    #[cfg(feature = "e2e")]
    let builder = crate::e2e::reader_window(builder);
    let window = builder.build().map_err(err)?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if matches!(event, WindowEvent::Destroyed) {
            handle.state::<OpenBooks>().lock().remove(&id);
        }
    });
    Ok(())
}

/// What the window needs to start: the book's files and where it left off.
#[tauri::command]
pub async fn reader_book(
    webview: Webview,
    state: State<'_, AppState>,
    open: State<'_, OpenBooks>,
) -> CmdResult<ReaderBook> {
    let id = book_id(webview.label())?;
    let library = state.library.clone();
    let (path, title, position, settings) = tauri::async_runtime::spawn_blocking(move || {
        let lib = library.lock().unwrap_or_else(PoisonError::into_inner);
        let settings = lib
            .setting(SETTINGS_KEY)
            .map_err(err)?
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        CmdResult::Ok((
            lib.book_file(id),
            lib.book(id).map_err(err)?.title,
            lib.reader_position(id).map_err(err)?,
            settings,
        ))
    })
    .await
    .map_err(err)??;

    let existing = open.lock().get(&id).cloned();
    let (book, entries) = tauri::async_runtime::spawn_blocking(move || {
        let book = match existing {
            Some(book) => book,
            None => Arc::new(Mutex::new(BookFile::open(&path).map_err(err)?)),
        };
        let entries = book
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entries()
            .map_err(err)?;
        CmdResult::Ok((book, entries))
    })
    .await
    .map_err(err)??;
    open.lock().insert(id, book);
    Ok(ReaderBook {
        title,
        entries,
        position,
        settings,
    })
}

/// One file from the window's book.
#[tauri::command]
pub async fn reader_entry(
    webview: Webview,
    open: State<'_, OpenBooks>,
    name: String,
) -> CmdResult<Response> {
    let id = book_id(webview.label())?;
    let book = open
        .lock()
        .get(&id)
        .cloned()
        .ok_or_else(|| "the book isn't open".to_owned())?;
    let data = tauri::async_runtime::spawn_blocking(move || {
        book.lock()
            .unwrap_or_else(PoisonError::into_inner)
            .read(&name)
    })
    .await
    .map_err(err)?
    .map_err(err)?;
    Ok(Response::new(data))
}

#[tauri::command]
pub async fn reader_save_position(
    webview: Webview,
    state: State<'_, AppState>,
    position: ReaderPosition,
) -> CmdResult<()> {
    let id = book_id(webview.label())?;
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_reader_position(id, &position)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Takes a typed value, so a book can only ever store valid settings.
#[tauri::command]
pub async fn reader_save_settings(
    state: State<'_, AppState>,
    settings: ReaderSettings,
) -> CmdResult<()> {
    let library = state.library.clone();
    let json = serde_json::to_string(&settings).map_err(err)?;
    tauri::async_runtime::spawn_blocking(move || {
        library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .set_setting(SETTINGS_KEY, Some(&json))
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// The window's book's highlights, in reading order.
#[tauri::command]
pub async fn reader_highlights(
    webview: Webview,
    state: State<'_, AppState>,
) -> CmdResult<Vec<ReaderHighlight>> {
    let id = book_id(webview.label())?;
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .reader_highlights(id)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

/// Adds a highlight, or with `id` changes one, in the window's book only.
#[tauri::command]
pub async fn reader_save_highlight(
    webview: Webview,
    state: State<'_, AppState>,
    id: Option<i64>,
    highlight: HighlightInput,
) -> CmdResult<ReaderHighlight> {
    let book = book_id(webview.label())?;
    let draft = highlight.draft()?;
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .save_reader_highlight(book, id, &draft)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[tauri::command]
pub async fn reader_delete_highlight(
    webview: Webview,
    state: State<'_, AppState>,
    id: i64,
) -> CmdResult<()> {
    let book = book_id(webview.label())?;
    let library = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .delete_reader_highlight(book, id)
            .map_err(err)
    })
    .await
    .map_err(err)?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_window_only_reaches_the_book_in_its_label() {
        assert_eq!(book_id("reader-42"), Ok(42));
        assert!(book_id("main").is_err());
        assert!(book_id("reader-").is_err());
        assert!(book_id("reader-1-2").is_err());
    }

    #[test]
    fn frames_stay_inside_the_reader() {
        let allowed = |url: &str| may_navigate(&url.parse().unwrap());
        assert!(allowed("tauri://localhost/reader.html"));
        assert!(allowed("about:srcdoc"));
        assert!(allowed("about:blank"));
        assert!(allowed("blob:tauri://localhost/6f1c"));
        assert!(!allowed("https://example.com/"));
        assert!(!allowed("http://127.0.0.1:8931/"));
        assert!(!allowed("file:///etc/passwd"));
        assert!(!allowed("data:text/html,x"));
        assert!(!allowed("javascript:alert(1)"));
        assert!(!allowed("tauri://evil/"));
        assert!(!allowed("about:config"));
    }

    #[test]
    fn settings_reject_anything_unexpected() {
        assert!(
            serde_json::from_str::<ReaderSettings>(r#"{"fontSize":120,"theme":"dark"}"#).is_ok()
        );
        assert!(serde_json::from_str::<ReaderSettings>(r#"{"fontSize":120,"theme":"x"}"#).is_err());
        assert!(serde_json::from_str::<ReaderSettings>(r#"{"flow":"sideways"}"#).is_err());
    }

    #[test]
    fn settings_from_an_older_build_still_load() {
        let old: ReaderSettings =
            serde_json::from_str(r#"{"fontSize":120,"theme":"dark"}"#).unwrap();
        assert_eq!(old.font_size, 120);
        assert_eq!(old.flow, Flow::Paginated);
        assert_eq!(old.width, Measure::Medium);
        let all = r#"{"fontSize":90,"theme":"sepia","flow":"scrolled","width":"wide","margins":"narrow","spread":"single","spacing":"loose"}"#;
        let new: ReaderSettings = serde_json::from_str(all).unwrap();
        assert_eq!(serde_json::to_string(&new).unwrap(), all);
    }

    #[test]
    fn highlights_are_checked_before_they_are_kept() {
        let input = |json: &str| serde_json::from_str::<HighlightInput>(json);
        let good = r#"{"cfi":"epubcfi(/6/2!/4/2,/1:0,/1:5)","href":"c1.xhtml","fraction":0.2,"text":"Hello","note":null,"color":"green"}"#;
        assert_eq!(input(good).unwrap().draft().unwrap().color, "green");
        assert!(input(&good.replace("green", "red")).is_err());
        assert!(
            input(&good.replace("epubcfi(", "javascript:("))
                .unwrap()
                .draft()
                .is_err()
        );
        let huge = good.replace("Hello", &"x".repeat(MAX_HIGHLIGHT_TEXT + 1));
        assert!(input(&huge).unwrap().draft().is_err());
    }
}
