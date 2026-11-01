//! `erc`: a developer CLI that exercises the core crates end to end without
//! the desktop UI. Device commands are read-only unless their help says they
//! write; database writes go through the backed-up write session.
//! `library` commands share the desktop app's library and backups.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use erc_convert::Kepubify;
use erc_core::collections::{self, CollectionChange, folder_groups, reconcile_series};
use erc_core::device::remove_from_device;
use erc_core::duplicates::{Reason, device_duplicates, library_duplicates};
use erc_core::import::{DeviceImport, import_from_device};
use erc_core::rules;
use erc_core::send::{Progress, SendOutcome, send_books, send_library_books, update_on_device};
use erc_device::{DeviceBook, ReadStatus};
use erc_device_kobo::{
    BookCheck, KoboDb, KoboDevice, LogEntry, MACOS_VOLUMES, ONBOARD_PREFIX, Record, restore,
    tidy_apple_double,
};
use erc_epub::MetadataEdit;
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
        /// EPUB or KEPUB files.
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
    Delete {
        name: String,
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
    /// Import EPUB files.
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
    },
    /// Remove a book from the library (not from devices).
    Rm { id: i64 },
    /// Search online for covers for a book.
    Covers { id: i64 },
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
    /// Add books from the Kobo to the library (all sideloaded EPUBs, or the
    /// given device paths). Only reads the Kobo.
    FromKobo {
        /// Device paths (`KEpub/…`) or ContentIDs; all books if none.
        books: Vec<String>,
        #[arg(long)]
        mount: Option<PathBuf>,
    },
    /// Apply series to books the Kobo has imported since they were sent. Writes, with a backup.
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
    let cli = Cli::parse();
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
            for i in infos {
                println!(
                    "{}  firmware {}  at {}",
                    i.model,
                    i.firmware,
                    i.mount.display()
                );
            }
        }
        Command::Kobo(KoboCommand::Ls(source)) => {
            let books = source.open()?.books()?;
            if cli.json {
                return print_json(&books);
            }
            for b in &books {
                println!("{}", book_line(b));
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
            let outcomes = send_books(&kobo, &Kepubify::locate()?, &files, print_progress);
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
                        eprintln!("FAILED {}: {e}", o.content_id)
                    }
                }
            }
            if failed > 0 {
                bail!("{failed} of {} books failed", outcomes.len());
            }
        }
        Command::Kobo(KoboCommand::Collection(command)) => {
            let (mount, change) = match command {
                CollectionCommand::Create { name, mount } => {
                    (mount, CollectionChange::Create { name })
                }
                CollectionCommand::Rename { from, to, mount } => {
                    (mount, CollectionChange::Rename { from, to })
                }
                CollectionCommand::Delete { name, mount } => {
                    (mount, CollectionChange::Delete { name })
                }
                CollectionCommand::Add { name, books, mount } => (
                    mount,
                    CollectionChange::Add {
                        name,
                        content_ids: books.iter().map(|b| content_id_for(b)).collect(),
                    },
                ),
                CollectionCommand::Remove { name, books, mount } => (
                    mount,
                    CollectionChange::Remove {
                        name,
                        content_ids: books.iter().map(|b| content_id_for(b)).collect(),
                    },
                ),
            };
            let kobo = find_kobo(mount.as_deref())?;
            collections::apply(&kobo, &backup_root()?, &[change])?;
            println!("done");
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
            for c in collections {
                println!("{:<40} {:>4} books", c.name, c.book_count);
            }
        }
    }
    Ok(())
}

fn library_command(lib: &mut Library, command: LibraryCommand, json: bool) -> Result<()> {
    match command {
        LibraryCommand::Add { files } => {
            for file in files {
                match lib.import(&file) {
                    Ok(ImportOutcome::Added {
                        book,
                        possible_duplicate_of,
                    }) => {
                        let dup = possible_duplicate_of
                            .map(|id| format!(" (possible duplicate of #{id})"))
                            .unwrap_or_default();
                        println!("added #{} {}{dup}", book.id, book.title);
                    }
                    Ok(ImportOutcome::AlreadyInLibrary { book }) => {
                        println!("already in library as #{} {}", book.id, book.title)
                    }
                    Err(e) => eprintln!("FAILED {}: {e}", file.display()),
                }
            }
        }
        LibraryCommand::Ls => {
            let books = lib.books()?;
            if json {
                return print_json(&books);
            }
            for b in &books {
                let series = b
                    .series
                    .as_ref()
                    .map(|s| format!("  [{s} {}]", b.series_index.as_deref().unwrap_or("")))
                    .unwrap_or_default();
                println!(
                    "#{:<4} {}  —  {}{series}",
                    b.id,
                    truncate(&b.title, 50),
                    b.authors.join(", ")
                );
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
        } => {
            let current = lib.book(id)?.edit();
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
            };
            let book = lib.update(id, &edit)?;
            println!("updated #{} {}", book.id, book.title);
        }
        LibraryCommand::Covers { id } => {
            let book = lib.book(id)?;
            let found = erc_covers::search(&erc_covers::CoverQuery {
                title: book.title.clone(),
                author: book.authors.first().cloned(),
                isbn: book
                    .identifiers
                    .iter()
                    .find(|i| i.scheme.as_deref() == Some("isbn"))
                    .map(|i| i.value.clone()),
            });
            if json {
                return print_json(&found);
            }
            for c in &found {
                println!("{:<13} {}", c.source, c.url);
            }
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
            println!("removed #{id}");
        }
        LibraryCommand::Update {
            ids,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let outcomes = update_on_device(
                lib,
                &kobo,
                &Kepubify::locate()?,
                &backup_root()?,
                &ids,
                print_progress,
            )?;
            finish_send(&kobo, &outcomes, no_eject, json)?;
        }
        LibraryCommand::Rules(command) => rules_command(lib, command, json)?,
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
            let outcomes = import_from_device(
                lib,
                &kobo,
                (!ids.is_empty()).then_some(&ids[..]),
                |i, n, title| eprintln!("[{i}/{n}] {title}"),
            )?;
            if json {
                return print_json(&outcomes);
            }
            let (mut added, mut linked, mut skipped, mut failed) = (0, 0, 0, 0);
            for o in &outcomes {
                match &o.result {
                    Ok(DeviceImport::Added { book_id }) => {
                        added += 1;
                        println!("added    #{book_id} {}", o.title);
                    }
                    Ok(DeviceImport::Linked { book_id }) => {
                        linked += 1;
                        println!("linked   #{book_id} {}", o.title);
                    }
                    Ok(DeviceImport::Skipped { reason }) => {
                        skipped += 1;
                        println!("skipped  {} ({reason})", o.title);
                    }
                    Err(e) => {
                        failed += 1;
                        eprintln!("FAILED   {}: {e}", o.title);
                    }
                }
            }
            println!("{added} added, {linked} linked, {skipped} skipped, {failed} failed");
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
        }
        LibraryCommand::Send {
            ids,
            collection,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let outcomes =
                send_library_books(lib, &kobo, &Kepubify::locate()?, &ids, print_progress);
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
            for r in rules {
                println!(
                    "#{:<3} {:<7} min {:<3} prefix {:?} {}",
                    r.id,
                    format!("{:?}", r.kind).to_lowercase(),
                    r.min_books,
                    r.prefix,
                    if r.enabled { "" } else { "(disabled)" }
                );
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
                rules::run(lib, &kobo, &backup_root()?)?
            );
        }
    }
    Ok(())
}

/// Accepts a device path (`KEpub/x.kepub.epub`) or a full ContentID.
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

fn print_progress(p: Progress) {
    match p {
        Progress::Preparing { index, total, file } => {
            eprintln!("[{index}/{total}] preparing {file}")
        }
        Progress::Converting { index, total, file } => {
            eprintln!("[{index}/{total}] converting {file}")
        }
        Progress::Copying { index, total, file } => eprintln!("[{index}/{total}] copying {file}"),
        Progress::Done {
            index,
            total,
            device_path,
            ..
        } => eprintln!("[{index}/{total}] sent to {device_path}"),
        Progress::Failed {
            index,
            total,
            file,
            error,
        } => eprintln!("[{index}/{total}] FAILED {file}: {error}"),
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

fn book_line(b: &DeviceBook) -> String {
    let status = match b.read_status {
        ReadStatus::Unread => "unread".to_owned(),
        ReadStatus::Reading => format!("{:>3}%", b.percent_read),
        ReadStatus::Finished => "done".to_owned(),
    };
    let last = b
        .last_read
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_default();
    format!(
        "{status:>6}  {last:<10}  {}  —  {}",
        truncate(&b.title, 50),
        truncate(b.author.as_deref().unwrap_or("Unknown"), 30)
    )
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        s.chars().take(max - 1).chain(['…']).collect()
    }
}

fn print_json<T: Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}
