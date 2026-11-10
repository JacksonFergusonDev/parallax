//! The menu bar. Commands for the library and Kobo go to the main window,
//! which runs them as its buttons do and reports which are available; book
//! commands go to the reader in front. A window that handles a shortcut
//! itself stops the menu running it a second time.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use serde::Deserialize;
use tauri::menu::{
    CheckMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, MenuItemKind, PredefinedMenuItem, Submenu,
};
use tauri::{AppHandle, Emitter, EventTarget, Manager, Runtime, State, WebviewWindowBuilder};

use crate::{about, reader, settings};

pub(crate) const REPOSITORY: &str = "https://github.com/JacksonFergusonDev/parallax";
const MAIN: &str = "main";
/// The event the main window gets with a menu command's id.
const COMMAND: &str = "menu-command";

/// Where a menu item's command goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Target {
    /// The main window, which says whether it's available.
    Main,
    /// The reader in front.
    Reader,
    /// The main window or the reader in front, whichever was used last.
    Front,
    /// Handled here.
    App,
}

/// Every command, by id: its label, shortcut, where it goes and whether it's
/// a checkmark item. Ids name the window's handler, so keep them in step
/// with `app/src/menu.ts` and the reader.
struct Command {
    id: &'static str,
    label: &'static str,
    accelerator: Option<&'static str>,
    target: Target,
    check: bool,
}

const fn command(id: &'static str, label: &'static str, target: Target) -> Command {
    Command {
        id,
        label,
        accelerator: None,
        target,
        check: false,
    }
}

const fn key(mut c: Command, accelerator: &'static str) -> Command {
    c.accelerator = Some(accelerator);
    c
}

const fn check(mut c: Command) -> Command {
    c.check = true;
    c
}

use Target::{App, Front, Main, Reader};

/// The library's and Kobo view's sort keys; each view enables its own.
const SORTS: [(&str, &str); 11] = [
    ("title", "Title"),
    ("author", "Author"),
    ("series", "Series"),
    ("tags", "Tags"),
    ("pages", "Pages"),
    ("progress", "Progress"),
    ("status", "Status"),
    ("last_read", "Last Read"),
    ("kobo", "On Kobo"),
    ("added", "Date Added"),
    ("color", "Colour"),
];

const ABOUT: Command = command("about", "About Parallax", App);
const SETTINGS: Command = key(command("settings", "Settings…", App), "CmdOrCtrl+,");

const FILE: &[Command] = &[
    key(command("file.add", "Add Books…", Main), "CmdOrCtrl+O"),
    command("file.calibre", "Import from calibre…", Main),
    command("kobo.addAll", "Add All from Kobo to Library", Main),
];

const FIND: Command = key(command("edit.find", "Find", Front), "CmdOrCtrl+F");

const VIEWS: &[Command] = &[
    check(key(command("view.library", "Library", Main), "CmdOrCtrl+1")),
    check(key(command("view.device", "On Kobo", Main), "CmdOrCtrl+2")),
    check(key(command("view.reading", "Reading", Main), "CmdOrCtrl+3")),
];

const LAYOUTS: &[Command] = &[
    check(command("view.grid", "as Covers", Main)),
    check(command("view.list", "as List", Main)),
];

const SORT_ORDER: &[Command] = &[
    check(command("sort.desc", "Descending", Main)),
    check(command("sort.asc", "Ascending", Main)),
];

const TEXT_SIZE: &[Command] = &[
    key(command("book.larger", "Larger Text", Reader), "CmdOrCtrl+="),
    key(
        command("book.smaller", "Smaller Text", Reader),
        "CmdOrCtrl+-",
    ),
];

const LIBRARY_TOOLS: &[Command] = &[
    command("library.enhance", "Find Series & Genres…", Main),
    command("library.duplicates", "Find Duplicates…", Main),
    command("library.check", "Check Library…", Main),
    command("library.tidy", "Tidy Genres…", Main),
];

const LIBRARY_CHANGES: &[Command] = &[
    command("library.remove", "Remove from Library…", Main),
    command("library.revert", "Revert Library Changes…", Main),
];

const KOBO_SEND: &[Command] = &[
    command("kobo.send", "Send to Kobo", Main),
    command("kobo.update", "Update on Kobo", Main),
];

const KOBO_BOOKS: &[Command] = &[
    command("kobo.organise", "Enhance and Organise…", Main),
    command("kobo.rules", "Automatic Collections…", Main),
    command("kobo.folders", "Folders → Collections…", Main),
    command("kobo.duplicates", "Find Duplicates on Kobo…", Main),
];

const KOBO_CARE: &[Command] = &[
    command("kobo.revert", "Revert Kobo Changes…", Main),
    command("kobo.tidy", "Remove macOS Clutter", Main),
    command("kobo.check", "Check This Kobo…", Main),
    command("kobo.about", "About This Kobo…", Main),
];

const EJECT: Command = key(command("kobo.eject", "Eject Kobo", Main), "CmdOrCtrl+E");

const BOOK_PANELS: &[Command] = &[
    command("book.contents", "Contents", Reader),
    command("book.notes", "Highlights and Notes", Reader),
    command("book.listen", "Read Aloud", Reader),
];

const BOOK_HISTORY: &[Command] = &[
    key(command("book.back", "Back", Reader), "CmdOrCtrl+["),
    key(command("book.forward", "Forward", Reader), "CmdOrCtrl+]"),
];

const LIBRARY_WINDOW: Command = key(command("window.library", "Library", App), "CmdOrCtrl+0");

const HELP: &[Command] = &[
    command("help.welcome", "Welcome to Parallax…", Main),
    command("help.guide", "Parallax Help", App),
    command("help.devices", "Supported Kobo Models", App),
    command("help.issue", "Report an Issue…", App),
];

/// What a window says about one of its commands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct ItemState {
    enabled: bool,
    #[serde(default)]
    checked: Option<bool>,
}

/// The built items, and what decides whether each is available.
#[derive(Default)]
pub struct MenuState {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    items: HashMap<String, (Target, Item)>,
    /// What the main window last reported; empty while it's closed.
    main: HashMap<String, ItemState>,
    /// The main window or reader used last.
    front: Option<String>,
}

impl MenuState {
    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Inner {
    fn reader_in_front(&self) -> Option<&str> {
        self.front
            .as_deref()
            .filter(|label| label.starts_with(reader::LABEL_PREFIX))
    }

    /// Whether `id`, going to `target`, can run now, and its checkmark.
    fn state(&self, id: &str, target: Target) -> ItemState {
        let main = self.main.get(id).copied().unwrap_or_default();
        match target {
            App => ItemState {
                enabled: true,
                checked: None,
            },
            Main => main,
            Reader => ItemState {
                enabled: self.reader_in_front().is_some(),
                checked: None,
            },
            Front => ItemState {
                enabled: self.reader_in_front().is_some() || main.enabled,
                checked: None,
            },
        }
    }

    /// Shows every item's current state.
    fn refresh(&self) {
        for (id, (target, item)) in &self.items {
            let state = self.state(id, *target);
            match item {
                MenuItemKind::MenuItem(item) => {
                    let _ = item.set_enabled(state.enabled);
                }
                MenuItemKind::Check(item) => {
                    let _ = item.set_enabled(state.enabled);
                    let _ = item.set_checked(state.checked.unwrap_or(false));
                }
                _ => {}
            }
        }
    }
}

/// The main window's report of which commands it can run now.
#[tauri::command]
pub fn menu_state(state: State<MenuState>, items: HashMap<String, ItemState>) {
    let mut inner = state.lock();
    inner.main = items;
    inner.refresh();
}

/// Notes the window used last, which book commands and Find go to.
pub fn focused(app: &AppHandle, label: &str) {
    if label != MAIN && !label.starts_with(reader::LABEL_PREFIX) {
        return;
    }
    let state = app.state::<MenuState>();
    let mut inner = state.lock();
    inner.front = Some(label.to_owned());
    inner.refresh();
}

/// Forgets a closed window: a closed main window can't run commands, and a
/// closed reader can't be in front.
pub fn closed(app: &AppHandle, label: &str) {
    let state = app.state::<MenuState>();
    let mut inner = state.lock();
    if label == MAIN {
        inner.main.clear();
    }
    if inner.front.as_deref() == Some(label) {
        inner.front = None;
    }
    inner.refresh();
}

type Wry = tauri::Wry;
type Item = MenuItemKind<Wry>;

/// Makes items and remembers each with where its command goes.
struct Items<'a> {
    app: &'a AppHandle,
    built: HashMap<String, (Target, Item)>,
}

impl Items<'_> {
    fn item(
        &mut self,
        id: &str,
        label: &str,
        accelerator: Option<&str>,
        target: Target,
        is_check: bool,
    ) -> tauri::Result<Item> {
        let kind = if is_check {
            MenuItemKind::Check(CheckMenuItem::with_id(
                self.app,
                id,
                label,
                false,
                false,
                accelerator,
            )?)
        } else {
            MenuItemKind::MenuItem(MenuItem::with_id(
                self.app,
                id,
                label,
                target == App,
                accelerator,
            )?)
        };
        self.built.insert(id.to_owned(), (target, kind.clone()));
        Ok(kind)
    }

    fn command(&mut self, c: &Command) -> tauri::Result<Item> {
        self.item(c.id, c.label, c.accelerator, c.target, c.check)
    }

    fn group(&mut self, commands: &[Command]) -> tauri::Result<Vec<Item>> {
        commands.iter().map(|c| self.command(c)).collect()
    }
}

/// `groups` with a separator between each.
fn join<'a>(
    groups: &[&[&'a dyn IsMenuItem<Wry>]],
    separator: &'a PredefinedMenuItem<Wry>,
) -> Vec<&'a dyn IsMenuItem<Wry>> {
    let mut out: Vec<&'a dyn IsMenuItem<Wry>> = Vec::new();
    for (i, group) in groups.iter().enumerate() {
        if i > 0 {
            out.push(separator);
        }
        out.extend_from_slice(group);
    }
    out
}

fn refs(items: &[Item]) -> Vec<&dyn IsMenuItem<Wry>> {
    items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect()
}

/// Builds the menu bar and keeps its items for [`menu_state`].
#[allow(clippy::too_many_lines)]
pub fn build(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let mut items = Items {
        app,
        built: HashMap::new(),
    };
    let about_item = items.command(&ABOUT)?;
    let settings_item = items.command(&SETTINGS)?;
    let file = items.group(FILE)?;
    let find = items.command(&FIND)?;
    let views = items.group(VIEWS)?;
    let layouts = items.group(LAYOUTS)?;
    let mut sorts = vec![items.command(&check(command("sort.default", "Default Order", Main)))?];
    for (sort, label) in SORTS {
        sorts.push(items.item(&format!("sort.{sort}"), label, None, Main, true)?);
    }
    let sort_order = items.group(SORT_ORDER)?;
    let text_size = items.group(TEXT_SIZE)?;
    let library_tools = items.group(LIBRARY_TOOLS)?;
    let library_changes = items.group(LIBRARY_CHANGES)?;
    let kobo_send = items.group(KOBO_SEND)?;
    let kobo_books = items.group(KOBO_BOOKS)?;
    let kobo_care = items.group(KOBO_CARE)?;
    let eject = items.command(&EJECT)?;
    let book_panels = items.group(BOOK_PANELS)?;
    let book_history = items.group(BOOK_HISTORY)?;
    let library_window = items.command(&LIBRARY_WINDOW)?;
    let help = items.group(HELP)?;

    let sep = PredefinedMenuItem::separator(app)?;
    let services = PredefinedMenuItem::services(app, None)?;
    let hide = PredefinedMenuItem::hide(app, None)?;
    let hide_others = PredefinedMenuItem::hide_others(app, None)?;
    let show_all = PredefinedMenuItem::show_all(app, None)?;
    let quit = PredefinedMenuItem::quit(app, None)?;
    let app_menu = Submenu::with_items(
        app,
        "Parallax",
        true,
        &join(
            &[
                &[&about_item],
                &[&settings_item],
                &[&services],
                &[&hide, &hide_others, &show_all],
                &[&quit],
            ],
            &sep,
        ),
    )?;

    let close = PredefinedMenuItem::close_window(app, None)?;
    let file_menu =
        Submenu::with_items(app, "File", true, &join(&[&refs(&file), &[&close]], &sep))?;

    let undo = PredefinedMenuItem::undo(app, None)?;
    let redo = PredefinedMenuItem::redo(app, None)?;
    let cut = PredefinedMenuItem::cut(app, None)?;
    let copy = PredefinedMenuItem::copy(app, None)?;
    let paste = PredefinedMenuItem::paste(app, None)?;
    let select_all = PredefinedMenuItem::select_all(app, None)?;
    let edit_menu = Submenu::with_items(
        app,
        "Edit",
        true,
        &join(
            &[
                &[&undo, &redo],
                &[&cut, &copy, &paste, &select_all],
                &[&find],
            ],
            &sep,
        ),
    )?;

    let sort_menu = Submenu::with_items(
        app,
        "Sort By",
        true,
        &join(&[&refs(&sorts), &refs(&sort_order)], &sep),
    )?;
    let mut layout_items = refs(&layouts);
    layout_items.push(&sort_menu);
    let fullscreen = PredefinedMenuItem::fullscreen(app, None)?;
    let view_menu = Submenu::with_items(
        app,
        "View",
        true,
        &join(
            &[
                &refs(&views),
                &layout_items,
                &refs(&text_size),
                &[&fullscreen],
            ],
            &sep,
        ),
    )?;

    let library_menu = Submenu::with_items(
        app,
        "Library",
        true,
        &join(&[&refs(&library_tools), &refs(&library_changes)], &sep),
    )?;
    let kobo_menu = Submenu::with_items(
        app,
        "Kobo",
        true,
        &join(
            &[
                &refs(&kobo_send),
                &refs(&kobo_books),
                &refs(&kobo_care),
                &[&eject],
            ],
            &sep,
        ),
    )?;
    let book_menu = Submenu::with_items(
        app,
        "Book",
        true,
        &join(&[&refs(&book_panels), &refs(&book_history)], &sep),
    )?;

    let minimize = PredefinedMenuItem::minimize(app, None)?;
    let zoom = PredefinedMenuItem::maximize(app, None)?;
    let front = PredefinedMenuItem::bring_all_to_front(app, None)?;
    let window_menu = Submenu::with_items(
        app,
        "Window",
        true,
        &join(&[&[&minimize, &zoom], &[&library_window], &[&front]], &sep),
    )?;
    let help_menu = Submenu::with_items(
        app,
        "Help",
        true,
        &join(
            &[&refs(&help[..1]), &refs(&help[1..3]), &refs(&help[3..])],
            &sep,
        ),
    )?;

    let menu = Menu::with_items(
        app,
        &[
            &app_menu,
            &file_menu,
            &edit_menu,
            &view_menu,
            &library_menu,
            &kobo_menu,
            &book_menu,
            &window_menu,
            &help_menu,
        ],
    )?;
    // macOS lists open windows in this menu and puts its search in Help.
    #[cfg(target_os = "macos")]
    {
        window_menu.set_as_windows_menu_for_nsapp()?;
        help_menu.set_as_help_menu_for_nsapp()?;
    }

    let state = app.state::<MenuState>();
    let mut inner = state.lock();
    inner.items = items.built;
    inner.refresh();
    Ok(menu)
}

/// Brings the main window forward, making it again if it was closed.
pub fn show_library<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(MAIN) {
        window.unminimize()?;
        // A window still loading shows itself once its library is ready.
        if window.is_visible()? {
            window.set_focus()?;
        }
        return Ok(());
    }
    if let Some(config) = app.config().app.windows.iter().find(|w| w.label == MAIN) {
        WebviewWindowBuilder::from_config(app, config)?.build()?;
    }
    Ok(())
}

/// Opens a Help page from the main window, as its Help menu item does.
#[tauri::command]
pub fn open_help(app: AppHandle, page: &str) -> crate::CmdResult<()> {
    match page {
        "devices" | "issue" => {
            run(&app, &format!("help.{page}"));
            Ok(())
        }
        _ => Err(format!("unknown help page {page}")),
    }
}

fn open_url(url: &str) {
    let _ = std::process::Command::new("open").arg(url).status();
}

#[allow(clippy::needless_pass_by_value)]
pub fn on_event(app: &AppHandle, event: MenuEvent) {
    run(app, event.id().as_ref());
}

/// Runs the menu command `id`, as choosing it in the menu does.
pub fn run(app: &AppHandle, id: &str) {
    let target = app
        .state::<MenuState>()
        .lock()
        .items
        .get(id)
        .map(|(t, _)| *t);
    match (id, target) {
        ("about", _) => {
            if let Err(error) = about::open_window(app) {
                eprintln!("Could not open About Parallax: {error}");
            }
        }
        ("settings", _) => {
            let _ = settings::open_window(app);
        }
        ("window.library", _) => {
            let _ = show_library(app);
        }
        ("help.guide", _) => open_url(&format!("{REPOSITORY}#readme")),
        ("help.devices", _) => open_url(&format!("{REPOSITORY}/blob/main/docs/device-support.md")),
        ("help.issue", _) => open_url(&format!("{REPOSITORY}/issues/new")),
        (_, Some(Reader)) => to_reader(app, id),
        (_, Some(Front)) => {
            let reader = app
                .state::<MenuState>()
                .lock()
                .reader_in_front()
                .map(str::to_owned);
            match reader {
                Some(_) => to_reader(app, id),
                None => to_main(app, id),
            }
        }
        (_, Some(Main)) => to_main(app, id),
        _ => {}
    }
}

fn to_main(app: &AppHandle, id: &str) {
    let _ = show_library(app);
    let _ = app.emit_to(EventTarget::webview_window(MAIN), COMMAND, id);
}

fn to_reader(app: &AppHandle, id: &str) {
    let label = app
        .state::<MenuState>()
        .lock()
        .reader_in_front()
        .map(str::to_owned);
    if let Some(label) = label {
        reader::push_command(app, &label, id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<&'static Command> {
        [
            FILE,
            VIEWS,
            LAYOUTS,
            SORT_ORDER,
            TEXT_SIZE,
            LIBRARY_TOOLS,
            LIBRARY_CHANGES,
            KOBO_SEND,
            KOBO_BOOKS,
            KOBO_CARE,
            BOOK_PANELS,
            BOOK_HISTORY,
            HELP,
        ]
        .into_iter()
        .flatten()
        .chain([&ABOUT, &SETTINGS, &FIND, &EJECT, &LIBRARY_WINDOW])
        .collect()
    }

    #[test]
    fn ids_and_shortcuts_are_unique() {
        let commands = all();
        let mut ids: Vec<_> = commands.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        let count = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), count, "an id is used twice");
        let mut keys: Vec<_> = commands.iter().filter_map(|c| c.accelerator).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count, "a shortcut is used twice");
    }

    #[test]
    fn readers_know_every_command_sent_to_them() {
        let sent: Vec<_> = all()
            .into_iter()
            .filter(|c| matches!(c.target, Reader | Front))
            .map(|c| c.id)
            .collect();
        let mut known = reader::COMMANDS.to_vec();
        let mut sent_sorted = sent.clone();
        known.sort_unstable();
        sent_sorted.sort_unstable();
        assert_eq!(sent_sorted, known);
    }

    #[test]
    fn book_commands_go_to_the_reader_in_front_only() {
        let mut inner = Inner::default();
        assert!(!inner.state("book.back", Reader).enabled);
        inner.front = Some("reader-3".into());
        assert!(inner.state("book.back", Reader).enabled);
        // Find goes to the reader in front, whatever the main window says.
        assert!(inner.state("edit.find", Front).enabled);
        inner.front = Some(MAIN.into());
        assert!(!inner.state("book.back", Reader).enabled);
        assert!(!inner.state("edit.find", Front).enabled);
        inner.main.insert(
            "edit.find".into(),
            ItemState {
                enabled: true,
                checked: None,
            },
        );
        assert!(inner.state("edit.find", Front).enabled);
    }

    #[test]
    fn main_commands_follow_the_main_window() {
        let mut inner = Inner::default();
        assert!(
            !inner.state("kobo.eject", Main).enabled,
            "nothing reported yet"
        );
        inner.main.insert(
            "view.grid".into(),
            ItemState {
                enabled: true,
                checked: Some(true),
            },
        );
        assert_eq!(inner.state("view.grid", Main).checked, Some(true));
        assert!(inner.state("settings", App).enabled);
    }
}
