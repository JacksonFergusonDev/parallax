//! `erc`: a developer CLI that exercises the core crates end to end without
//! the desktop UI. Every command is read-only unless stated otherwise.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
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
        let device = match &self.mount {
            Some(mount) => KoboDevice::open(mount)?,
            None => match KoboDevice::detect(Path::new(MACOS_VOLUMES))
                .into_iter()
                .next()
            {
                Some(device) => device,
                None => bail!("no Kobo found in {MACOS_VOLUMES}; connect it and tap Connect"),
            },
        };
        Ok(device.open_db()?)
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
