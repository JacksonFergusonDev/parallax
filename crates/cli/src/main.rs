//! `erc`: a developer CLI that exercises the core crates end to end without
//! the desktop UI. Device commands are read-only unless their help says they
//! write; database writes go through the backed-up write session.
//! `library` commands share the desktop app's library and backups.

mod output;

use std::path::{Path, PathBuf};

use output::{Reporter, failure, success, table, warning};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use erc_convert::amazon::ChildProcess;
use erc_convert::{Kepubify, ToKepub};
use erc_core::calibre;
use erc_core::collections::{self, CollectionChange, folder_groups, reconcile_series};
use erc_core::device::{
    change_reading_status, health, merge_collections, remove_from_device, sync_annotations,
    sync_reading,
};
use erc_core::duplicates::{Reason, device_duplicates, library_duplicates};
use erc_core::enhance;
use erc_core::health::check_library;
use erc_core::import::{DeviceImport, import_from_device};
use erc_core::ingest::import_file;
use erc_core::rules;
use erc_core::send::{SendOutcome, send_books, send_library_books, update_on_device};
use erc_device::{DeviceBook, ReadStatus};
use erc_device_kobo::{
    BookCheck, KoboDb, KoboDevice, LogEntry, MACOS_VOLUMES, ONBOARD_PREFIX, Record,
    remove_orphaned_images, restore, tidy_apple_double,
};
use erc_epub::{Contributor, MetadataEdit};
use erc_library::{ImportOutcome, Library, Rule, RuleKind};
use serde::Serialize;

#[derive(Parser)]
#[command(name = "erc", version, about = "E-Reader Companion developer CLI")]
struct Cli {
    /// Print JSON instead of a table.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List connected e-readers.
    Devices {
        /// Directory to scan for mounted volumes.
        #[arg(long, default_value = MACOS_VOLUMES)]
        volumes: PathBuf,
    },
    /// Kobo commands.
    #[command(subcommand)]
    Kobo(KoboCommand),
    /// Show an EPUB's metadata and cover details.
    EpubInfo { file: PathBuf },
    /// Convert EPUBs to KEPUB, copy them to the Kobo and eject it. Writes files.
    Send {
        /// EPUB, KEPUB or Amazon (AZW3, MOBI, AZW, PRC, KFX) files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
        /// Mount point of the Kobo (default: the first one found in /Volumes).
        #[arg(long)]
        mount: Option<PathBuf>,
        /// Leave the Kobo mounted afterwards.
        #[arg(long)]
        no_eject: bool,
    },
    /// The local library (shared with the desktop app).
    Library {
        /// Library folder (default: the desktop app's).
        #[arg(long, global = true)]
        library: Option<PathBuf>,
        #[command(subcommand)]
        command: LibraryCommand,
    },
}

#[derive(Subcommand)]
enum CollectionCommand {
    Create {
        name: String,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    Rename {
        from: String,
        to: String,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Delete collections (all in one backed-up session). Books stay.
    Delete {
        #[arg(required = true)]
        names: Vec<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Add books (device paths or ContentIDs) to a collection, creating it if needed.
    Add {
        name: String,
        #[arg(required = true)]
        books: Vec<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    Remove {
        name: String,
        #[arg(required = true)]
        books: Vec<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum RulesCommand {
    Ls,
    /// Add a rule: `series`, `tag` or `author`.
    Add {
        kind: String,
        /// Only make a collection once this many books share the value.
        #[arg(long, default_value_t = 2)]
        min: usize,
        /// Prepended to collection names, e.g. "Series: ".
        #[arg(long, default_value = "")]
        prefix: String,
    },
    Enable {
        id: i64,
    },
    Disable {
        id: i64,
    },
    Rm {
        id: i64,
    },
    /// Show what enabled rules would add on the connected Kobo (read-only).
    Preview {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Apply enabled rules on the connected Kobo. Writes, with a backup.
    Apply {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
enum LibraryCommand {
    /// Import EPUB files, or Amazon books (AZW3, MOBI, AZW, PRC, KFX), which
    /// are converted to EPUB.
    Add {
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// List books.
    Ls,
    /// Edit a book's metadata. Unspecified fields keep their values.
    Edit {
        id: i64,
        #[arg(long)]
        title: Option<String>,
        /// Repeat for several authors.
        #[arg(long = "author")]
        authors: Vec<String>,
        /// Empty string clears the series.
        #[arg(long)]
        series: Option<String>,
        #[arg(long)]
        index: Option<String>,
        /// Repeat for several tags; replaces existing tags.
        #[arg(long = "tag")]
        tags: Vec<String>,
        /// `Name=role` with a MARC relator code (trl translator, ill
        /// illustrator, edt editor, nrt narrator); repeat for several;
        /// replaces existing contributors. `--contributor ""` clears them.
        #[arg(long = "contributor")]
        contributors: Vec<String>,
    },
    /// Remove a book from the library (not from devices).
    Rm { id: i64 },
    /// Search online for covers for a book.
    Covers { id: i64 },
    /// Upscale a book's cover into an image file, leaving the book alone.
    Upscale {
        id: i64,
        out: PathBuf,
        /// The compiled Core ML model.
        #[arg(long, default_value = "app/src-tauri/models/cover-upscaler.mlmodelc")]
        model: PathBuf,
    },
    /// Replace a book's cover from a URL or image file, or reset it.
    Cover {
        id: i64,
        #[arg(long, conflicts_with_all = ["file", "reset"])]
        url: Option<String>,
        #[arg(long, conflicts_with = "reset")]
        file: Option<PathBuf>,
        /// Go back to the cover inside the EPUB.
        #[arg(long)]
        reset: bool,
    },
    /// Re-send books already on the Kobo with their latest edits, keeping
    /// reading progress. Writes, with a backup.
    Update {
        #[arg(required = true)]
        ids: Vec<i64>,
        #[arg(long)]
        mount: Option<PathBuf>,
        #[arg(long)]
        no_eject: bool,
    },
    /// Write a book with its edits applied (what a device receives, before KEPUB conversion).
    Export { id: i64, out: PathBuf },
    /// Automatic grouping rules (series/tag/author → collection).
    #[command(subcommand)]
    Rules(RulesCommand),
    /// List probable duplicate books in the library.
    Duplicates,
    /// Print or save a book's highlights and notes (copied from the Kobo by
    /// `reconcile` or when the app sees it).
    Annotations {
        id: i64,
        /// Markdown (default) or JSON.
        #[arg(long, value_enum, default_value_t = AnnotationFormat::Markdown)]
        format: AnnotationFormat,
        /// Write to this file instead of printing.
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Check every book: covers, authors, series numbers and gaps, and EPUB
    /// structure (missing files, broken links, clutter, DRM).
    Check,
    /// Look up series and genres online (and on a connected Kobo) for books
    /// that lack them. Shows proposals; --apply saves the confident ones.
    Enhance {
        /// Library book ids; all books if none.
        ids: Vec<i64>,
        /// Also use the Kobo's series and folders.
        #[arg(long)]
        mount: Option<PathBuf>,
        #[arg(long)]
        apply: bool,
    },
    /// Show or change a setting (google_books_key).
    Setting {
        key: String,
        value: Option<String>,
        #[arg(long)]
        clear: bool,
    },
    /// Add books from the Kobo to the library (all sideloaded EPUBs, or the
    /// given device paths). Only reads the Kobo.
    FromKobo {
        /// Device paths (`KEpub/…`) or ContentIDs; all books if none.
        books: Vec<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Use standard genre names: rename genre tags written other ways
    /// ("sci-fi" → "Science Fiction") and, with a Kobo connected, its genre
    /// collections. Shows the changes; --apply makes them.
    Genres {
        #[arg(long)]
        mount: Option<PathBuf>,
        /// Leave the Kobo out even if one is connected.
        #[arg(long)]
        library_only: bool,
        /// Also rename tags that only mention a genre ("Space Opera").
        #[arg(long)]
        loose: bool,
        #[arg(long)]
        apply: bool,
    },
    /// Import a calibre library: EPUBs (or KEPUBs or Amazon formats, converted)
    /// with calibre's metadata and covers. Only reads the calibre library; books already here are matched.
    FromCalibre {
        /// The calibre library folder (the one with metadata.db); calibre's
        /// current library if omitted.
        path: Option<PathBuf>,
        /// Show what would be imported without importing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Apply series to books the Kobo has imported since they were sent (writes,
    /// with a backup), and copy reading progress into the library.
    Reconcile {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Send books to the Kobo with their edited metadata, then eject. Writes files.
    Send {
        #[arg(required = true)]
        ids: Vec<i64>,
        /// Also add the books to this collection (created if needed).
        #[arg(long)]
        collection: Option<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
        #[arg(long)]
        no_eject: bool,
    },
}

/// The desktop app's data folder (Tauri's app data dir on macOS).
fn app_data_dir() -> Result<PathBuf> {
    let home = std::env::var_os("HOME").context("HOME is not set")?;
    Ok(PathBuf::from(home).join("Library/Application Support/dev.erc.companion"))
}

fn default_library_root() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("library"))
}

/// Shared with the desktop app so backups from either show up together.
fn backup_root() -> Result<PathBuf> {
    Ok(app_data_dir()?.join("device-backups"))
}

fn device_backups(kobo: &KoboDevice) -> Result<Vec<PathBuf>> {
    let serial: String = kobo
        .version()
        .serial
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let dir = backup_root()?.join(serial);
    let mut found: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|entries| entries.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    found.retain(|p| p.extension().is_some_and(|e| e == "sqlite"));
    found.sort();
    Ok(found)
}

#[derive(Subcommand)]
enum KoboCommand {
    /// List books on the device.
    Ls(Source),
    /// List collections on the device.
    Collections(Source),
    /// Remove books from the device (files and database rows). Writes, with a backup.
    Rm {
        /// Paths on the device (e.g. `KEpub/Dune - Frank Herbert.kepub.epub`) or ContentIDs.
        #[arg(required = true)]
        books: Vec<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Change collections on the device. Writes, with a backup.
    #[command(subcommand)]
    Collection(CollectionCommand),
    /// Turn the device's folders (e.g. `KEpub/Fantasy`) into collections.
    Folders {
        /// Create the collections; without this, only shows what would happen.
        #[arg(long)]
        apply: bool,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Delete macOS `._*` clutter from the device.
    Tidy {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Report entries whose file is gone, files not imported, duplicate
    /// collections and leftover cover images. Only reads the device.
    Check {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Delete cached covers of books no longer on the device.
    CleanImages {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Merge collections that exist more than once. Writes, with a backup.
    MergeCollections {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Set books' reading status. Writes, with a backup.
    Status {
        #[arg(value_enum)]
        status: StatusArg,
        /// Device paths (`KEpub/…`) or ContentIDs.
        #[arg(required = true)]
        books: Vec<String>,
        /// Also clear where the book was open (start over).
        #[arg(long)]
        reset_position: bool,
        /// Also clear the reading time.
        #[arg(long)]
        reset_time: bool,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// List probable duplicate books on the device. Only reads it.
    Duplicates {
        #[arg(long)]
        mount: Option<PathBuf>,
        /// Library holding dismissed groups (default: the app's).
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// What ERC has done to the device, from its `/.erc/` records.
    History {
        #[arg(long)]
        mount: Option<PathBuf>,
        /// How many log entries to show, newest first.
        #[arg(short = 'n', long, default_value_t = 20)]
        limit: usize,
    },
    /// Save a device report: model, firmware and database layout, without
    /// book titles, serial number or reading data. Send it to whoever adds
    /// support for new Kobos. Only reads the device.
    Report {
        #[arg(long)]
        mount: Option<PathBuf>,
        /// Where to save it (default: print it).
        #[arg(long, short)]
        out: Option<PathBuf>,
    },
    /// Reading statistics: time read, books in progress, books finished by
    /// year. Only reads the device.
    Stats {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// List database backups for the connected device.
    Backups {
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Put a database backup back on the device.
    Restore {
        /// Backup file (default: the newest for this device).
        backup: Option<PathBuf>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
}

#[derive(Args)]
struct Source {
    /// Mount point of the Kobo (default: the first one found in /Volumes).
    #[arg(long, conflicts_with = "db")]
    mount: Option<PathBuf>,
    /// Read a KoboReader.sqlite file directly instead of a device.
    #[arg(long)]
    db: Option<PathBuf>,
}

impl Source {
    fn open(&self) -> Result<KoboDb> {
        if let Some(db) = &self.db {
            return KoboDb::open_read_only(db).with_context(|| format!("opening {}", db.display()));
        }
        Ok(find_kobo(self.mount.as_deref())?.open_db()?)
    }
}

fn find_kobo(mount: Option<&Path>) -> Result<KoboDevice> {
    match mount {
        Some(mount) => Ok(KoboDevice::open(mount)?),
        None => KoboDevice::detect(Path::new(MACOS_VOLUMES))
            .into_iter()
            .next()
            .with_context(|| {
                format!("no Kobo found in {MACOS_VOLUMES}; connect it and tap Connect")
            }),
    }
}

fn main() -> Result<()> {
    erc_convert::amazon::run_child_if_asked();
    let cli = Cli::parse();
    if cli.json {
        console::set_colors_enabled(false);
        console::set_colors_enabled_stderr(false);
    }
    match cli.command {
        Command::Devices { volumes } => {
            let infos: Vec<_> = KoboDevice::detect(&volumes)
                .iter()
                .map(KoboDevice::info)
                .collect();
            if cli.json {
                return print_json(&infos);
            }
            if infos.is_empty() {
                println!("No devices found.");
            }
            if !infos.is_empty() {
                let mut rows = table(&["Model", "Firmware", "Mount", "Changes"]);
                for i in infos {
                    let changes = match &i.read_only {
                        None => "allowed".to_owned(),
                        Some(why) => format!("read-only: {why}"),
                    };
                    rows.add_row([i.model, i.firmware, i.mount.display().to_string(), changes]);
                }
                println!("{rows}");
            }
        }
        Command::Kobo(KoboCommand::Ls(source)) => {
            let books = source.open()?.books()?;
            if cli.json {
                return print_json(&books);
            }
            if !books.is_empty() {
                let mut rows = table(&["Status", "Last read", "Title", "Author"]);
                for b in &books {
                    rows.add_row([
                        status_of(b),
                        b.last_read
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default(),
                        b.title.clone(),
                        b.author.clone().unwrap_or_else(|| "Unknown".into()),
                    ]);
                }
                println!("{rows}");
            }
            println!("{} books", books.len());
        }
        Command::EpubInfo { file } => {
            let epub = erc_epub::Epub::open(&file)?;
            let metadata = epub.metadata();
            let cover = epub
                .cover()?
                .map(|c| format!("{} ({} bytes)", c.media_type, c.data.len()));
            if cli.json {
                return print_json(&serde_json::json!({ "metadata": metadata, "cover": cover }));
            }
            println!("{metadata:#?}");
            println!("cover: {}", cover.as_deref().unwrap_or("none"));
        }
        Command::Send {
            files,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let progress = Reporter::new(cli.json);
            let outcomes = send_books(&kobo, &ToKepub::locate()?, &files, |p| progress.send(p));
            drop(progress);
            finish_send(&kobo, &outcomes, no_eject, cli.json)?;
        }
        Command::Library { library, command } => {
            let root = library.map_or_else(default_library_root, Ok)?;
            let mut lib = Library::open(&root)?;
            library_command(&mut lib, command, cli.json)?;
        }
        Command::Kobo(KoboCommand::Rm { books, mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let ids: Vec<String> = books.iter().map(|b| content_id_for(b)).collect();
            let library = Library::open(default_library_root()?).ok();
            let outcomes = remove_from_device(library.as_ref(), &kobo, &backup_root()?, &ids)?;
            let mut failed = 0;
            for o in &outcomes {
                match &o.result {
                    Ok(()) => println!("removed {}", o.content_id),
                    Err(e) => {
                        failed += 1;
                        eprintln!("{} {}: {e}", failure("FAILED"), o.content_id);
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} of {} books failed", outcomes.len());
            }
        }
        Command::Kobo(KoboCommand::Collection(command)) => {
            let (mount, changes) = match command {
                CollectionCommand::Create { name, mount } => {
                    (mount, vec![CollectionChange::Create { name }])
                }
                CollectionCommand::Rename { from, to, mount } => {
                    (mount, vec![CollectionChange::Rename { from, to }])
                }
                CollectionCommand::Delete { names, mount } => (
                    mount,
                    names
                        .into_iter()
                        .map(|name| CollectionChange::Delete { name })
                        .collect(),
                ),
                CollectionCommand::Add { name, books, mount } => (
                    mount,
                    vec![CollectionChange::Add {
                        name,
                        content_ids: books.iter().map(|b| content_id_for(b)).collect(),
                    }],
                ),
                CollectionCommand::Remove { name, books, mount } => (
                    mount,
                    vec![CollectionChange::Remove {
                        name,
                        content_ids: books.iter().map(|b| content_id_for(b)).collect(),
                    }],
                ),
            };
            let kobo = find_kobo(mount.as_deref())?;
            collections::apply(&kobo, &backup_root()?, &changes)?;
            println!("{}", success("done"));
        }
        Command::Kobo(KoboCommand::Folders { apply, mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let groups = folder_groups(&kobo.open_db()?.books()?);
            for g in &groups {
                println!("{:<30} {:>4} books", g.folder, g.content_ids.len());
            }
            if apply && !groups.is_empty() {
                let changes: Vec<CollectionChange> = groups
                    .into_iter()
                    .map(|g| CollectionChange::Add {
                        name: g.folder,
                        content_ids: g.content_ids,
                    })
                    .collect();
                collections::apply(&kobo, &backup_root()?, &changes)?;
                println!("created {} collections", changes.len());
            } else if !apply {
                println!("(dry run; pass --apply to create these collections)");
            }
        }
        Command::Kobo(KoboCommand::Tidy { mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            println!("removed {} macOS ._ files", tidy_apple_double(&kobo));
        }
        Command::Kobo(KoboCommand::Check { mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let h = health(&kobo)?;
            if cli.json {
                return print_json(&h);
            }
            // Green when there's nothing to do, yellow when there is.
            let count = |n: usize, what: &str| {
                let text = format!("{n} {what}");
                if n == 0 {
                    success(&text)
                } else {
                    warning(&text)
                }
            };
            println!(
                "{}",
                count(h.missing_files.len(), "entries whose file is gone")
            );
            for m in &h.missing_files {
                println!("  {}  ({})", m.title, m.path);
            }
            println!(
                "{}",
                count(h.not_imported.len(), "files not in the Kobo's library")
            );
            for m in &h.not_imported {
                println!("  {}", m.path);
            }
            println!(
                "{}",
                count(h.duplicate_collections.len(), "duplicated collections")
            );
            for d in &h.duplicate_collections {
                println!("  {} ×{}", d.name, d.copies);
            }
            println!(
                "{} ({} KB)",
                count(h.orphaned_images, "leftover cover images"),
                h.orphaned_image_bytes / 1024
            );
        }
        Command::Kobo(KoboCommand::CleanImages { mount }) => {
            let (files, bytes) = remove_orphaned_images(&find_kobo(mount.as_deref())?)?;
            println!(
                "removed {files} leftover cover images ({} KB)",
                bytes / 1024
            );
        }
        Command::Kobo(KoboCommand::MergeCollections { mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            println!(
                "removed {} duplicate collection copies",
                merge_collections(&kobo, &backup_root()?)?
            );
        }
        Command::Kobo(KoboCommand::Status {
            status,
            books,
            reset_position,
            reset_time,
            mount,
        }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let ids: Vec<String> = books.iter().map(|b| content_id_for(b)).collect();
            let library = Library::open(default_library_root()?).ok();
            let status = match status {
                StatusArg::Unread => ReadStatus::Unread,
                StatusArg::Reading => ReadStatus::Reading,
                StatusArg::Finished => ReadStatus::Finished,
            };
            let n = change_reading_status(
                library.as_ref(),
                &kobo,
                &backup_root()?,
                &ids,
                status,
                reset_position,
                reset_time,
            )?;
            println!("updated {n} books; eject to see the change");
        }
        Command::Kobo(KoboCommand::Duplicates { mount, library }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let lib = Library::open(library.map_or_else(default_library_root, Ok)?)?;
            let groups = device_duplicates(&lib, &kobo)?;
            if cli.json {
                return print_json(&groups);
            }
            let titles: std::collections::HashMap<String, DeviceBook> = kobo
                .open_db()?
                .books()?
                .into_iter()
                .map(|b| (b.id.clone(), b))
                .collect();
            for g in &groups {
                println!("{}", reasons_label(&g.reasons));
                for m in &g.members {
                    let b = &titles[&m.content_id];
                    println!(
                        "  {} {:<10} {:>5}  {} — {}  ({})",
                        if m.remove { "remove?" } else { "keep   " },
                        m.format,
                        status_of(b),
                        b.title,
                        b.author.as_deref().unwrap_or("?"),
                        b.path.as_deref().unwrap_or(&b.id),
                    );
                }
            }
            println!("{} groups", groups.len());
        }
        Command::Kobo(KoboCommand::Stats { mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let library = Library::open(default_library_root()?).ok();
            let stats = erc_core::stats::reading_stats(&kobo, library.as_ref())?;
            if cli.json {
                return print_json(&stats);
            }
            let hours = |secs: u64| format!("{:.1} h", secs as f64 / 3600.0);
            println!(
                "{}: {} read across {} books on the device",
                stats.device_model,
                hours(stats.total_secs),
                stats.books_on_device
            );
            let mut by_year = std::collections::BTreeMap::<String, usize>::new();
            for b in &stats.finished {
                let year = b
                    .finished_at
                    .map_or_else(|| "undated".to_owned(), |d| d.format("%Y").to_string());
                *by_year.entry(year).or_default() += 1;
            }
            println!("{} books finished", stats.finished.len());
            for (year, n) in by_year.iter().rev() {
                println!("  {year}: {n}");
            }
            if !stats.reading.is_empty() {
                let mut rows = table(&["Reading", "Progress", "Time", "Left"]);
                for b in &stats.reading {
                    rows.add_row([
                        b.title.clone(),
                        format!("{}%", b.percent),
                        b.time_secs.map(hours).unwrap_or_default(),
                        b.left_secs.map(hours).unwrap_or_default(),
                    ]);
                }
                println!("{rows}");
            }
        }
        Command::Kobo(KoboCommand::Report { mount, out }) => {
            let report = find_kobo(mount.as_deref())?.report();
            let json = serde_json::to_string_pretty(&report)?;
            match out {
                Some(path) => {
                    std::fs::write(&path, json + "\n")
                        .with_context(|| format!("could not write {}", path.display()))?;
                    eprintln!(
                        "{}",
                        success(&format!(
                            "Saved the report for this {} to {}",
                            report.model_name,
                            path.display()
                        ))
                    );
                }
                None => println!("{json}"),
            }
        }
        Command::Kobo(KoboCommand::History { mount, limit }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let mut state = kobo.erc_state();
            if cli.json {
                state.log.truncate(limit);
                return print_json(&state);
            }
            match &state.device {
                Some(d) => {
                    println!("ERC device id   {}", d.device_id);
                    println!(
                        "first write     {} (ERC {}, firmware {})",
                        d.first_write.at, d.first_write.erc_version, d.first_write.firmware
                    );
                    println!(
                        "latest write    {} (ERC {}, firmware {})",
                        d.last_write.at, d.last_write.erc_version, d.last_write.firmware
                    );
                }
                None => println!("ERC has not changed this Kobo yet."),
            }
            let checks = state.check(kobo.mount());
            let count = |c: BookCheck| checks.iter().filter(|(_, k)| *k == c).count();
            println!(
                "books           {} written by ERC ({} changed since, {} gone)",
                checks.len(),
                count(BookCheck::Changed),
                count(BookCheck::Missing)
            );
            for problem in &state.problems {
                println!("problem         {problem}");
            }
            for e in state.log.iter().take(limit) {
                let what = e.detail.clone().unwrap_or_else(|| {
                    e.content_ids
                        .iter()
                        .map(|id| id.trim_start_matches(ONBOARD_PREFIX))
                        .collect::<Vec<_>>()
                        .join(", ")
                });
                let failed = e
                    .error
                    .as_deref()
                    .map(|err| format!("  FAILED: {err}"))
                    .unwrap_or_default();
                println!("{}  {:<10} {what}{failed}", e.at, e.op);
            }
        }
        Command::Kobo(KoboCommand::Backups { mount }) => {
            for b in device_backups(&find_kobo(mount.as_deref())?)? {
                println!("{}", b.display());
            }
        }
        Command::Kobo(KoboCommand::Restore { backup, mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            let backup = match backup {
                Some(b) => b,
                None => device_backups(&kobo)?
                    .pop()
                    .context("no backups for this device")?,
            };
            restore(&kobo.db_path(), &backup)?;
            kobo.record(Record {
                entries: vec![
                    LogEntry::new("restore").detail(
                        backup
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    ),
                ],
                books: Vec::new(),
            });
            println!("restored {}", backup.display());
        }
        Command::Kobo(KoboCommand::Collections(source)) => {
            let collections = source.open()?.collections()?;
            if cli.json {
                return print_json(&collections);
            }
            if collections.is_empty() {
                println!("No collections.");
            }
            if !collections.is_empty() {
                let mut rows = table(&["Collection", "Books"]);
                for c in collections {
                    rows.add_row([c.name, c.book_count.to_string()]);
                }
                println!("{rows}");
            }
        }
    }
    Ok(())
}

fn library_command(lib: &mut Library, command: LibraryCommand, json: bool) -> Result<()> {
    match command {
        LibraryCommand::Add { files } => {
            let amazon = ChildProcess::current()?;
            for file in files {
                match import_file(lib, &amazon, &file) {
                    Ok(ImportOutcome::Added {
                        book,
                        possible_duplicate_of,
                    }) => {
                        let dup = possible_duplicate_of
                            .map(|id| format!(" (possible duplicate of #{id})"))
                            .unwrap_or_default();
                        println!("{} #{} {}{dup}", success("added"), book.id, book.title);
                    }
                    Ok(ImportOutcome::AlreadyInLibrary { book }) => {
                        println!("already in library as #{} {}", book.id, book.title);
                    }
                    Err(e) => eprintln!("{} {}: {e}", failure("FAILED"), file.display()),
                }
            }
        }
        LibraryCommand::Ls => {
            let books = lib.books()?;
            if json {
                return print_json(&books);
            }
            if !books.is_empty() {
                let mut rows = table(&["ID", "Title", "Authors", "Series", "Number"]);
                for b in &books {
                    rows.add_row([
                        b.id.to_string(),
                        b.title.clone(),
                        b.authors.join(", "),
                        b.series.clone().unwrap_or_default(),
                        b.series_index.clone().unwrap_or_default(),
                    ]);
                }
                println!("{rows}");
            }
            println!("{} books", books.len());
        }
        LibraryCommand::Edit {
            id,
            title,
            authors,
            series,
            index,
            tags,
            contributors,
        } => {
            let current = lib.book(id)?.edit();
            let contributors = if contributors.is_empty() {
                current.contributors
            } else {
                Some(
                    contributors
                        .iter()
                        .filter(|c| !c.trim().is_empty())
                        .map(|c| match c.rsplit_once('=') {
                            Some((name, role)) => Ok(Contributor {
                                name: name.trim().to_owned(),
                                role: role.trim().to_lowercase(),
                            }),
                            None => Err(anyhow::anyhow!("expected Name=role, got {c:?}")),
                        })
                        .collect::<Result<Vec<_>>>()?,
                )
            };
            let edit = MetadataEdit {
                title: title.unwrap_or(current.title),
                authors: if authors.is_empty() {
                    current.authors
                } else {
                    authors
                },
                series_index: index.or(current.series_index),
                series: series.or(current.series),
                tags: if tags.is_empty() { current.tags } else { tags },
                contributors,
            };
            let book = lib.update(id, &edit)?;
            println!("{} #{} {}", success("updated"), book.id, book.title);
        }
        LibraryCommand::Covers { id } => {
            let book = lib.book(id)?;
            let found = erc_covers::search(&erc_covers::CoverQuery {
                title: book.title.clone(),
                author: book.authors.first().cloned(),
                isbn: book.isbn().map(str::to_owned),
                asin: book.asin().map(str::to_owned),
            });
            if json {
                return print_json(&found);
            }
            if !found.is_empty() {
                let mut rows = table(&["Source", "URL"]);
                for c in &found {
                    rows.add_row([c.source, c.url.as_str()]);
                }
                println!("{rows}");
            }
        }
        LibraryCommand::Upscale { id, out, model } => {
            let source = lib
                .cover_source(id)?
                .context("this book has no cover to upscale")?;
            let started = std::time::Instant::now();
            let engine = erc_upscale::CoreMl::load(&model)?;
            let up = erc_upscale::upscale_cover(
                &engine,
                &source,
                erc_library::CUSTOM_COVER_MAX_HEIGHT,
                |done, total| eprint!("\rtile {done} of {total}"),
                &std::sync::atomic::AtomicBool::new(false),
            )?;
            eprintln!();
            std::fs::write(&out, &up.after_png)?;
            println!(
                "{}×{} to {}×{} in {:.1}s: {}",
                up.before_size.0,
                up.before_size.1,
                up.after_size.0,
                up.after_size.1,
                started.elapsed().as_secs_f32(),
                out.display()
            );
        }
        LibraryCommand::Cover {
            id,
            url,
            file,
            reset,
        } => {
            let book = if reset {
                lib.reset_cover(id)?
            } else if let Some(url) = url {
                lib.set_cover(id, &erc_covers::download(&url)?)?
            } else if let Some(file) = file {
                lib.set_cover(id, &std::fs::read(&file)?)?
            } else {
                bail!("pass --url, --file or --reset");
            };
            println!("cover updated for #{} {}", book.id, book.title);
        }
        LibraryCommand::Export { id, out } => {
            let book = lib.export(id, &out)?;
            println!("exported #{} {} to {}", book.id, book.title, out.display());
        }
        LibraryCommand::Rm { id } => {
            lib.remove(id)?;
            println!("{} #{id}", success("removed"));
        }
        LibraryCommand::Update {
            ids,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let progress = Reporter::new(json);
            let outcomes = update_on_device(
                lib,
                &kobo,
                &Kepubify::locate()?,
                &backup_root()?,
                &ids,
                |p| progress.send(p),
            )?;
            drop(progress);
            finish_send(&kobo, &outcomes, no_eject, json)?;
        }
        LibraryCommand::Rules(command) => rules_command(lib, command, json)?,
        LibraryCommand::Enhance { ids, mount, apply } => {
            let ids = if ids.is_empty() {
                lib.books()?.iter().map(|b| b.id).collect()
            } else {
                ids
            };
            let clues = match mount {
                Some(m) => enhance::DeviceClues::read(lib, &KoboDevice::open(m)?)?,
                None => enhance::DeviceClues::default(),
            };
            let fetch = enhance::fetcher(lib).map_err(anyhow::Error::msg)?;
            let books = ids
                .iter()
                .map(|&id| lib.book(id))
                .collect::<erc_library::Result<Vec<_>>>()?;
            let progress = Reporter::new(json);
            let proposals = enhance::propose(
                &books,
                &enhance::options(lib),
                &fetch,
                &clues,
                |n, total, title| progress.update(n, total, title),
            );
            drop(progress);
            if json {
                return print_json(&proposals);
            }
            let mark = |confident: bool| if confident { "+" } else { "?" };
            let mut accepted = Vec::new();
            for p in proposals
                .iter()
                .filter(|p| !p.is_empty() || !p.problems.is_empty())
            {
                println!("#{} {}", p.book_id, lib.book(p.book_id)?.title);
                if let Some(s) = &p.series {
                    let index = s
                        .index
                        .as_deref()
                        .map(|i| format!(" #{i}"))
                        .unwrap_or_default();
                    println!(
                        "  {} series  {}{index}  ({})",
                        mark(s.confident),
                        s.name,
                        s.source
                    );
                }
                for g in &p.genres {
                    println!(
                        "  {} genre   {}  ({})",
                        mark(g.confident),
                        g.name,
                        g.sources.join(", ")
                    );
                }
                for problem in &p.problems {
                    println!("  ! {problem}");
                }
                accepted.push(enhance::Accepted {
                    book_id: p.book_id,
                    series: p
                        .series
                        .as_ref()
                        .filter(|s| s.confident)
                        .map(|s| (s.name.clone(), s.index.clone())),
                    genres: p
                        .genres
                        .iter()
                        .filter(|g| g.confident)
                        .map(|g| g.name.clone())
                        .collect(),
                });
            }
            let with = proposals.iter().filter(|p| !p.is_empty()).count();
            println!(
                "{with} of {} books have proposals (+ selected, ? low confidence)",
                proposals.len()
            );
            if apply {
                let updated = enhance::apply(lib, &accepted)?;
                println!("updated {} books", updated.len());
            } else {
                println!("(dry run; pass --apply to save the + changes)");
            }
        }
        LibraryCommand::Setting { key, value, clear } => {
            if clear {
                lib.set_setting(&key, None)?;
            } else if let Some(v) = value {
                lib.set_setting(&key, Some(&v))?;
            } else {
                println!("{}", lib.setting(&key)?.unwrap_or_default());
            }
        }
        LibraryCommand::Check => {
            let findings = check_library(lib, |_, _| {})?;
            if json {
                return print_json(&findings);
            }
            let mut rows = table(&["Check", "Book", "Detail"]);
            for f in &findings {
                let titles: Vec<String> = f
                    .book_ids
                    .iter()
                    .filter_map(|&id| lib.book(id).ok().map(|b| b.title))
                    .collect();
                let label = format!("{:?}", f.check);
                rows.add_row(vec![label, titles.join("; "), f.detail.clone()]);
            }
            println!("{rows}");
            println!("{} findings", findings.len());
        }
        LibraryCommand::Annotations { id, format, out } => {
            let book = lib.book(id)?;
            let items = lib.annotations(id)?;
            let text = match format {
                AnnotationFormat::Markdown => erc_core::annotations::markdown(&book, &items),
                AnnotationFormat::Json => erc_core::annotations::json(&book, &items),
            };
            match out {
                Some(path) => {
                    std::fs::write(&path, text).with_context(|| path.display().to_string())?;
                    println!(
                        "{} {} annotations to {}",
                        success("saved"),
                        items.len(),
                        path.display()
                    );
                }
                None => print!("{text}"),
            }
        }
        LibraryCommand::Duplicates => {
            let groups = library_duplicates(lib)?;
            if json {
                return print_json(&groups);
            }
            for g in &groups {
                println!("{}", reasons_label(&g.reasons));
                for m in &g.members {
                    let b = lib.book(m.book_id)?;
                    println!(
                        "  {} #{:<5} {} — {}",
                        if m.remove { "remove?" } else { "keep   " },
                        b.id,
                        b.title,
                        b.authors.join(", "),
                    );
                }
            }
            println!("{} groups", groups.len());
        }
        LibraryCommand::FromKobo { books, mount } => {
            let kobo = find_kobo(mount.as_deref())?;
            let ids: Vec<String> = books.iter().map(|b| content_id_for(b)).collect();
            let progress = Reporter::new(json);
            let outcomes = import_from_device(
                lib,
                &kobo,
                (!ids.is_empty()).then_some(&ids[..]),
                |i, n, title| {
                    progress.update(i.saturating_sub(1), n, &format!("importing {title}"));
                },
            )?;
            drop(progress);
            if json {
                return print_json(&outcomes);
            }
            let (mut added, mut linked, mut skipped, mut failed) = (0, 0, 0, 0);
            for o in &outcomes {
                match &o.result {
                    Ok(DeviceImport::Added { book_id }) => {
                        added += 1;
                        println!("{}    #{book_id} {}", success("added"), o.title);
                    }
                    Ok(DeviceImport::Linked { book_id }) => {
                        linked += 1;
                        println!("{}   #{book_id} {}", success("linked"), o.title);
                    }
                    Ok(DeviceImport::Skipped { reason }) => {
                        skipped += 1;
                        println!("{}  {} ({reason})", warning("skipped"), o.title);
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("{}   {}: {e}", failure("FAILED"), o.title);
                    }
                }
            }
            println!("{added} added, {linked} linked, {skipped} skipped, {failed} failed");
            if failed > 0 {
                bail!("{failed} books could not be imported");
            }
        }
        LibraryCommand::Genres {
            mount,
            library_only,
            loose,
            apply,
        } => {
            let kobo = if library_only {
                None
            } else if mount.is_some() {
                Some(find_kobo(mount.as_deref())?)
            } else {
                find_kobo(None).ok()
            };
            let plan = erc_core::genres::plan(lib, kobo.as_ref()).map_err(anyhow::Error::msg)?;
            if json && !apply {
                return print_json(&plan);
            }
            let tags: Vec<_> = plan.tags.iter().filter(|t| t.exact || loose).collect();
            for t in &plan.tags {
                let chosen = t.exact || loose;
                println!(
                    "{} tag “{}” → “{}” ({} {}){}",
                    if chosen { "rename" } else { "  skip" },
                    t.from,
                    t.to,
                    t.book_ids.len(),
                    if t.book_ids.len() == 1 {
                        "book"
                    } else {
                        "books"
                    },
                    if t.exact {
                        ""
                    } else {
                        ", only mentions it: --loose"
                    }
                );
            }
            for c in &plan.collections {
                println!(
                    "{} Kobo collection “{}” → “{}” ({} books)",
                    if c.merge { " merge" } else { "rename" },
                    c.from,
                    c.to,
                    c.books
                );
            }
            if tags.is_empty() && plan.collections.is_empty() {
                println!("Genres already use standard names.");
                return Ok(());
            }
            if !apply {
                println!("Run with --apply to make these changes.");
                return Ok(());
            }
            let choice = erc_core::genres::TidyChoice {
                tags: tags
                    .iter()
                    .map(|t| (t.from.clone(), t.to.clone()))
                    .collect(),
                collections: plan
                    .collections
                    .iter()
                    .map(|c| (c.from.clone(), c.to.clone()))
                    .collect(),
            };
            let result = erc_core::genres::apply_tidy(lib, kobo.as_ref(), &backup_root()?, &choice)
                .map_err(anyhow::Error::msg)?;
            if json {
                return print_json(&result);
            }
            println!(
                "{} {} books retagged, {} Kobo collections renamed or merged",
                success("done"),
                result.books,
                result.collections
            );
        }
        LibraryCommand::FromCalibre { path, dry_run } => {
            let root = match path {
                Some(p) => p,
                None => calibre::default_library()
                    .context("no calibre library found; pass its folder")?,
            };
            let books = calibre::read_library(&root)?;
            let plan = calibre::preview(lib, &books);
            if dry_run {
                if json {
                    return print_json(&plan);
                }
                println!("{}: {} books", root.display(), plan.total);
                println!("  {} to import", plan.new);
                println!("  {} already in your library", plan.already);
                for (formats, n) in &plan.no_epub {
                    println!("  {n} without an EPUB ({formats})");
                }
                return Ok(());
            }
            let progress = Reporter::new(json);
            let amazon = ChildProcess::current()?;
            let outcomes = calibre::import(lib, &amazon, &books, |i, n, title| {
                progress.update(i.saturating_sub(1), n, &format!("importing {title}"));
            });
            drop(progress);
            if json {
                return print_json(&outcomes);
            }
            let (mut added, mut matched, mut skipped, mut failed) = (0, 0, 0, 0);
            for o in &outcomes {
                match &o.result {
                    Ok(calibre::CalibreImport::Added { book_id }) => {
                        added += 1;
                        println!("{}    #{book_id} {}", success("added"), o.title);
                    }
                    Ok(calibre::CalibreImport::Matched { .. }) => matched += 1,
                    Ok(calibre::CalibreImport::Skipped { reason }) => {
                        skipped += 1;
                        println!("{}  {} ({reason})", warning("skipped"), o.title);
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("{}   {}: {e}", failure("FAILED"), o.title);
                    }
                }
            }
            println!(
                "{added} added, {matched} already in your library, {skipped} skipped, {failed} failed"
            );
            if failed > 0 {
                bail!("{failed} books could not be imported");
            }
        }
        LibraryCommand::Reconcile { mount } => {
            let kobo = find_kobo(mount.as_deref())?;
            println!(
                "applied series to {} books",
                reconcile_series(lib, &kobo, &backup_root()?)?
            );
            println!(
                "updated reading progress for {} books",
                sync_reading(lib, &kobo)?
            );
            println!(
                "copied {} new or changed highlights, notes and bookmarks",
                sync_annotations(lib, &kobo)?
            );
        }
        LibraryCommand::Send {
            ids,
            collection,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let progress = Reporter::new(json);
            let outcomes =
                send_library_books(lib, &kobo, &Kepubify::locate()?, &ids, |p| progress.send(p));
            drop(progress);
            if let Some(name) = collection {
                let content_ids: Vec<String> = outcomes
                    .iter()
                    .filter_map(|o| o.result.as_ref().ok().map(|s| s.content_id.clone()))
                    .collect();
                if !content_ids.is_empty() {
                    collections::apply(
                        &kobo,
                        &backup_root()?,
                        &[CollectionChange::Add {
                            name: name.clone(),
                            content_ids,
                        }],
                    )?;
                    eprintln!("added to collection “{name}”");
                }
            }
            finish_send(&kobo, &outcomes, no_eject, json)?;
        }
    }
    Ok(())
}

fn rules_command(lib: &Library, command: RulesCommand, json: bool) -> Result<()> {
    let set_enabled = |id: i64, enabled: bool| -> Result<()> {
        let rule = lib
            .rules()?
            .into_iter()
            .find(|r| r.id == id)
            .context("no such rule")?;
        lib.save_rule(&Rule { enabled, ..rule })?;
        Ok(())
    };
    match command {
        RulesCommand::Ls => {
            let rules = lib.rules()?;
            if json {
                return print_json(&rules);
            }
            if rules.is_empty() {
                println!("No rules.");
            }
            if !rules.is_empty() {
                let mut rows = table(&["ID", "Kind", "Minimum books", "Prefix", "Status"]);
                for r in rules {
                    rows.add_row([
                        r.id.to_string(),
                        format!("{:?}", r.kind).to_lowercase(),
                        r.min_books.to_string(),
                        r.prefix,
                        if r.enabled {
                            success("enabled")
                        } else {
                            warning("disabled")
                        },
                    ]);
                }
                println!("{rows}");
            }
        }
        RulesCommand::Add { kind, min, prefix } => {
            let kind = match kind.as_str() {
                "series" => RuleKind::Series,
                "tag" => RuleKind::Tag,
                "author" => RuleKind::Author,
                other => bail!("unknown rule kind {other:?}; use series, tag or author"),
            };
            let rule = lib.save_rule(&Rule {
                id: 0,
                kind,
                enabled: true,
                min_books: min,
                prefix,
            })?;
            println!("added rule #{}", rule.id);
        }
        RulesCommand::Enable { id } => set_enabled(id, true)?,
        RulesCommand::Disable { id } => set_enabled(id, false)?,
        RulesCommand::Rm { id } => lib.delete_rule(id)?,
        RulesCommand::Preview { mount } => {
            let kobo = find_kobo(mount.as_deref())?;
            let planned = rules::plan(&lib.rules()?, lib, &kobo)?;
            if json {
                return print_json(&planned);
            }
            if planned.is_empty() {
                println!("Nothing to do.");
            }
            for p in &planned {
                println!(
                    "{} “{}”: add {} {}",
                    if p.new { "create" } else { "update" },
                    p.name,
                    p.content_ids.len(),
                    if p.content_ids.len() == 1 {
                        "book"
                    } else {
                        "books"
                    }
                );
            }
        }
        RulesCommand::Apply { mount } => {
            let kobo = find_kobo(mount.as_deref())?;
            println!(
                "added {} books to collections",
                rules::run(lib, &kobo, &backup_root()?, rules::Trigger::User)?
            );
        }
    }
    Ok(())
}

/// Accepts a device path (`KEpub/x.kepub.epub`) or a full ContentID.
#[derive(Clone, Copy, clap::ValueEnum)]
enum AnnotationFormat {
    Markdown,
    Json,
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum StatusArg {
    Unread,
    Reading,
    Finished,
}

fn reasons_label(reasons: &[Reason]) -> String {
    reasons
        .iter()
        .map(|r| match r {
            Reason::SameFile => "identical file",
            Reason::SameIsbn => "same ISBN",
            Reason::SameTitleAndAuthor => "same title and author",
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn status_of(b: &DeviceBook) -> String {
    match b.read_status {
        ReadStatus::Unread => "-".into(),
        ReadStatus::Reading => format!("{}%", b.percent_read),
        ReadStatus::Finished => "done".into(),
    }
}

fn content_id_for(book: &str) -> String {
    if book.starts_with("file://") {
        book.to_owned()
    } else {
        format!("{ONBOARD_PREFIX}{}", book.trim_start_matches('/'))
    }
}

fn finish_send(
    kobo: &KoboDevice,
    outcomes: &[SendOutcome],
    no_eject: bool,
    json: bool,
) -> Result<()> {
    let failed = outcomes.iter().filter(|o| o.result.is_err()).count();
    if json {
        print_json(&outcomes)?;
    } else if !outcomes.is_empty() {
        let mut rows = table(&["Status", "Book", "Destination / error"]);
        for outcome in outcomes {
            let (status, detail) = match &outcome.result {
                Ok(sent) => (success("sent"), sent.device_path.clone()),
                Err(error) => (warning("failed"), error.clone()),
            };
            rows.add_row([status, outcome.source.clone(), detail]);
        }
        println!("{rows}");
    }
    if !no_eject && failed < outcomes.len() {
        kobo.eject()?;
        eprintln!("Ejected. Unplug the Kobo and let it import the new books.");
    }
    if failed > 0 {
        bail!("{failed} of {} books failed", outcomes.len());
    }
    Ok(())
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
