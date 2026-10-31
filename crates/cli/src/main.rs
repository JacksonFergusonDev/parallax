//! `erc`: a developer CLI that exercises the core crates end to end without
//! the desktop UI. Every command is read-only unless stated otherwise;
//! `send` writes book files to the device (never its database).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use erc_convert::Kepubify;
use erc_core::send::{Progress, send_books};
use erc_device::{DeviceBook, ReadStatus};
use erc_device_kobo::{KoboDb, KoboDevice, MACOS_VOLUMES};
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
}

#[derive(Subcommand)]
enum KoboCommand {
    /// List books on the device.
    Ls(Source),
    /// List collections on the device.
    Collections(Source),
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
        Command::Send {
            files,
            mount,
            no_eject,
        } => {
            let kobo = find_kobo(mount.as_deref())?;
            let converter = Kepubify::locate()?;
            let outcomes = send_books(&kobo, &converter, &files, |p| match p {
                Progress::Converting { index, total, file } => {
                    eprintln!("[{index}/{total}] converting {file}")
                }
                Progress::Copying { index, total, file } => {
                    eprintln!("[{index}/{total}] copying {file}")
                }
                Progress::Done {
                    index,
                    total,
                    device_path,
                    ..
                } => {
                    eprintln!("[{index}/{total}] sent to {device_path}")
                }
                Progress::Failed {
                    index,
                    total,
                    file,
                    error,
                } => {
                    eprintln!("[{index}/{total}] FAILED {file}: {error}")
                }
            });
            let failed = outcomes.iter().filter(|o| o.result.is_err()).count();
            if cli.json {
                print_json(&outcomes)?;
            }
            if !no_eject && failed < outcomes.len() {
                kobo.eject()?;
                eprintln!("Ejected. Unplug the Kobo and let it import the new books.");
            }
            if failed > 0 {
                bail!("{failed} of {} books failed", outcomes.len());
            }
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
