//! `erc`: a developer CLI that exercises the core crates end to end without
//! the desktop UI. Device commands are read-only unless their help says they
//! write; database writes go through the backed-up write session.
//! `library` commands share the desktop app's library and backups.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use erc_convert::Kepubify;
use erc_core::device::remove_from_device;
use erc_core::send::{Progress, SendOutcome, send_books, send_library_books, update_on_device};
use erc_device::{DeviceBook, ReadStatus};
use erc_device_kobo::{
    KoboDb, KoboDevice, MACOS_VOLUMES, ONBOARD_PREFIX, restore, tidy_apple_double,
};
use erc_epub::MetadataEdit;
use erc_library::{ImportOutcome, Library};
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
    /// Send books to the Kobo with their edited metadata, then eject. Writes files.
    Send {
        #[arg(required = true)]
        ids: Vec<i64>,
        #[arg(long)]
        mount: Option<PathBuf>,
        #[arg(long)]
        no_eject: bool,
    },
}

/// Same folder the desktop app uses (Tauri's app data dir on macOS).
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
    /// Delete macOS `._*` clutter from the device.
    Tidy {
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
            let ids: Vec<String> = books
                .iter()
                .map(|b| {
                    if b.starts_with("file://") {
                        b.clone()
                    } else {
                        format!("{ONBOARD_PREFIX}{}", b.trim_start_matches('/'))
                    }
                })
                .collect();
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
        Command::Kobo(KoboCommand::Tidy { mount }) => {
            let kobo = find_kobo(mount.as_deref())?;
            println!("removed {} macOS ._ files", tidy_apple_double(&kobo));
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
        LibraryCommand::Send {
            ids,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let outcomes =
                send_library_books(lib, &kobo, &Kepubify::locate()?, &ids, print_progress);
            finish_send(&kobo, &outcomes, no_eject, json)?;
        }
    }
    Ok(())
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
