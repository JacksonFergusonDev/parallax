//! `parallax-cli kindle …`: Kindles on a drive or over MTP, and Send to
//! Kindle by email.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use clap::Subcommand;
use parallax_core::import::DeviceImport;
use parallax_core::kindle::{self, KINDLE_PAGE_NUMBERS, SendOptions};
use parallax_core::kindle_email::{self, EmailSettings};
use parallax_core::send::SendOutcome;
use parallax_device::DeviceInfo;
use parallax_device_kindle::detect::{self, Unavailable};
use parallax_device_kindle::{Connections, KindleDevice, MetaCache};
use parallax_device_kobo::MACOS_VOLUMES;
use parallax_library::Library;

use crate::output::{Reporter, success, table, warning};
use crate::print_json;

#[derive(Subcommand)]
pub enum KindleCommand {
    /// List books on the Kindle with their reading progress. Only reads it.
    Ls {
        #[arg(long)]
        device: Option<String>,
    },
    /// Send library books to the Kindle as AZW3. Writes files.
    Send {
        /// Library book ids.
        #[arg(required = true)]
        ids: Vec<i64>,
        /// Send again over copies already there, keeping the Kindle's place.
        #[arg(long)]
        update: bool,
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Copy book files (AZW3, MOBI, PDF…) to the Kindle as they are. Writes files.
    SendFiles {
        #[arg(required = true)]
        files: Vec<PathBuf>,
        #[arg(long)]
        device: Option<String>,
    },
    /// Remove books from the Kindle with their sidecar files and covers. Writes.
    Rm {
        /// Paths on the Kindle, e.g. `documents/Dune - Frank Herbert.azw3`.
        #[arg(required = true)]
        paths: Vec<String>,
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Add books from the Kindle to the library. Only reads the Kindle.
    Import {
        /// Paths on the Kindle; all books when none are given.
        paths: Vec<String>,
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Put back covers Amazon replaced, and copy reading progress and
    /// highlights into the library.
    Sync {
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Email library books to your Kindle address (set in the app's Settings).
    Email {
        #[arg(required = true)]
        ids: Vec<i64>,
        #[arg(long)]
        library: Option<PathBuf>,
    },
    /// Close the connection (MTP) or unmount the drive.
    Eject {
        #[arg(long)]
        device: Option<String>,
    },
}

/// Kindles found, and MTP ones that couldn't be opened. `free_from_photos`
/// lets Parallax stop macOS's photo import service when it holds one.
pub fn detect_all(
    connections: &Connections,
    volumes: &Path,
    free_from_photos: bool,
) -> (Vec<KindleDevice>, Vec<Unavailable>) {
    let mut found = detect::drives(volumes);
    let (mtp, unavailable) = connections.refresh(free_from_photos);
    found.extend(mtp);
    (found, unavailable)
}

/// Every connected Kindle's info, for `devices`.
pub fn infos(volumes: &Path) -> Vec<DeviceInfo> {
    let connections = Connections::default();
    let (found, unavailable) = detect_all(&connections, volumes, true);
    let mut infos: Vec<DeviceInfo> = found.iter().map(KindleDevice::info).collect();
    for u in unavailable {
        let (_, model) = detect::model_for(&u.serial, u.product.as_deref());
        eprintln!("{model} ({}): {}", u.serial, u.reason);
    }
    infos.sort_by(|a, b| a.mount.cmp(&b.mount));
    infos
}

fn find(connections: &Connections, key: Option<&str>) -> Result<KindleDevice> {
    let (found, unavailable) = detect_all(connections, Path::new(MACOS_VOLUMES), true);
    let device = match key {
        Some(key) => found
            .into_iter()
            .find(|k| k.key() == key || k.serial() == key),
        None => found.into_iter().next(),
    };
    device.ok_or_else(|| match unavailable.first() {
        Some(u) => anyhow!("{}", u.reason),
        None => anyhow!("no Kindle found; connect it by USB and unlock it"),
    })
}

fn open_library(path: Option<PathBuf>) -> Result<Library> {
    let root = path.map_or_else(crate::default_library_root, Ok)?;
    Library::open(&root).with_context(|| format!("opening the library at {}", root.display()))
}

fn print_outcomes(outcomes: &[SendOutcome], json: bool) -> Result<()> {
    let failed = outcomes.iter().filter(|o| o.result.is_err()).count();
    if json {
        print_json(&outcomes)?;
    } else if !outcomes.is_empty() {
        let mut rows = table(&["Status", "Book", "Destination / error"]);
        for o in outcomes {
            let (status, detail) = match &o.result {
                Ok(sent) => (success("sent"), sent.device_path.clone()),
                Err(e) => (warning("failed"), e.clone()),
            };
            rows.add_row([status, o.source.clone(), detail]);
        }
        println!("{rows}");
    }
    if failed > 0 {
        bail!("{failed} of {} books failed", outcomes.len());
    }
    Ok(())
}

pub fn run(command: KindleCommand, json: bool) -> Result<()> {
    let connections = Connections::default();
    match command {
        KindleCommand::Ls { device } => {
            let kindle = find(&connections, device.as_deref())?;
            let books = kindle.books(&mut MetaCache::new())?;
            let catalog = kindle::catalog(&books);
            if json {
                return print_json(&catalog);
            }
            let mut rows = table(&["Read", "Title", "Author", "Path"]);
            for b in &catalog {
                rows.add_row([
                    format!("{}%", b.book.percent_read),
                    b.book.title.clone(),
                    b.book.author.clone().unwrap_or_default(),
                    b.book.id.clone(),
                ]);
            }
            println!("{rows}");
        }
        KindleCommand::Send {
            ids,
            update,
            device,
            library,
        } => {
            let kindle = find(&connections, device.as_deref())?;
            let lib = open_library(library)?;
            let converter = parallax_convert::amazon::ChildProcess::current()?.to_azw3();
            let options = SendOptions {
                converter: &converter,
                page_numbers: lib.setting(KINDLE_PAGE_NUMBERS)?.as_deref() != Some("off"),
            };
            let progress = Reporter::new(json);
            let outcomes = if update {
                kindle::update_on_device(&lib, &kindle, &options, &ids, |p| progress.send(p))
            } else {
                kindle::send_library_books(&lib, &kindle, &options, &ids, |p| progress.send(p))
            };
            drop(progress);
            print_outcomes(&outcomes, json)?;
        }
        KindleCommand::SendFiles { files, device } => {
            let kindle = find(&connections, device.as_deref())?;
            let progress = Reporter::new(json);
            let outcomes = kindle::send_files(&kindle, &files, true, |p| progress.send(p));
            drop(progress);
            print_outcomes(&outcomes, json)?;
        }
        KindleCommand::Rm {
            paths,
            device,
            library,
        } => {
            let kindle = find(&connections, device.as_deref())?;
            let lib = open_library(library).ok();
            let outcomes = kindle::remove_from_device(lib.as_ref(), &kindle, &paths, |_, _| {});
            if json {
                return print_json(&outcomes);
            }
            for o in &outcomes {
                match &o.result {
                    Ok(()) => println!("{} {}", success("removed"), o.content_id),
                    Err(e) => println!("{} {}: {e}", warning("failed"), o.content_id),
                }
            }
        }
        KindleCommand::Import {
            paths,
            device,
            library,
        } => {
            let kindle = find(&connections, device.as_deref())?;
            let mut lib = open_library(library)?;
            let books = kindle.books(&mut MetaCache::new())?;
            let converter = parallax_convert::amazon::ChildProcess::current()?;
            let progress = Reporter::new(json);
            let chosen = (!paths.is_empty()).then_some(paths.as_slice());
            let outcomes = kindle::import_from_device(
                &mut lib,
                &kindle,
                &books,
                chosen,
                &converter,
                |i, total, title| progress.update(i - 1, total, title),
            );
            drop(progress);
            if json {
                return print_json(&outcomes);
            }
            for o in &outcomes {
                let line = match &o.result {
                    Ok(DeviceImport::Added { book_id }) => {
                        format!("{} (book {book_id})", success("added"))
                    }
                    Ok(DeviceImport::Linked { book_id }) => {
                        format!("{} (book {book_id})", success("matched"))
                    }
                    Ok(DeviceImport::Skipped { reason }) => {
                        format!("{} {reason}", warning("skipped"))
                    }
                    Err(e) => format!("{} {e}", warning("failed")),
                };
                println!("{line}: {}", o.title);
            }
        }
        KindleCommand::Sync { device, library } => {
            let kindle = find(&connections, device.as_deref())?;
            let mut lib = open_library(library)?;
            let done = kindle::reconcile(&mut lib, &kindle, &mut MetaCache::new())?;
            if json {
                return print_json(&done);
            }
            println!(
                "Covers put back: {}. Reading progress updated: {}. Highlights and notes new or changed: {}.",
                done.thumbnails, done.reading, done.annotations
            );
        }
        KindleCommand::Email { ids, library } => {
            let lib = open_library(library)?;
            let settings = EmailSettings::load(&lib);
            let progress = Reporter::new(json);
            let outcomes = kindle_email::send_books(&lib, &settings, &ids, |p| progress.send(p))
                .map_err(|e| anyhow!(e))?;
            drop(progress);
            print_outcomes(&outcomes, json)?;
        }
        KindleCommand::Eject { device } => {
            let kindle = find(&connections, device.as_deref())?;
            if kindle.transport() == parallax_device::Transport::Mtp {
                connections.eject(kindle.key())?;
            } else {
                kindle.eject()?;
            }
            if !json {
                eprintln!("Ejected. You can unplug the Kindle.");
            }
        }
    }
    Ok(())
}
