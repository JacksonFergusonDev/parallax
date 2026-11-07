//! Amazon formats (AZW3, MOBI, AZW, PRC, KFX) to EPUB, via boko.
//!
//! Files are recognised by their content, since `.azw` can hold any of them.
//! DRM is detected and refused, never removed.
//!
//! Release builds abort on panic, and these parsers read untrusted binary
//! files, so production conversion runs in a child process: [`ChildProcess`]
//! re-executes the current binary, which hands over to [`run_child_if_asked`]
//! at the top of `main`.

use std::env;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use boko::{Book, Format};

use crate::{Converter, Error, Result};

/// File extensions offered for Amazon books.
pub const EXTENSIONS: [&str; 5] = ["azw3", "azw", "mobi", "prc", "kfx"];

/// First argument that turns the current binary into a converter.
const CHILD_ARG: &str = "--erc-amazon-to-epub";
const EXIT_DRM: i32 = 3;
const EXIT_FAILED: i32 = 4;

const PDB_TYPE_OFFSET: usize = 60;
const MOBI_MAGIC: &[u8] = b"BOOKMOBI";
const KFX_MAGIC: &[u8] = b"CONT";
const ENCRYPTED_KFX_MAGIC: &[u8] = b"\xeaDRMION\xee";
const TOPAZ_MAGIC: &[u8] = b"TPZ0";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A Mobipocket container: KF8 (AZW3), old MOBI, or a combo of both.
    Mobi,
    Kfx,
    EncryptedKfx,
    Topaz,
}

/// What kind of Amazon book `path` is, or `None` if it isn't one.
pub fn sniff(path: &Path) -> io::Result<Option<Kind>> {
    let mut head = Vec::with_capacity(PDB_TYPE_OFFSET + MOBI_MAGIC.len());
    File::open(path)?
        .take((PDB_TYPE_OFFSET + MOBI_MAGIC.len()) as u64)
        .read_to_end(&mut head)?;
    Ok(if head.get(PDB_TYPE_OFFSET..) == Some(MOBI_MAGIC) {
        Some(Kind::Mobi)
    } else if head.starts_with(KFX_MAGIC) {
        Some(Kind::Kfx)
    } else if head.starts_with(ENCRYPTED_KFX_MAGIC) {
        Some(Kind::EncryptedKfx)
    } else if head.starts_with(TOPAZ_MAGIC) {
        Some(Kind::Topaz)
    } else {
        None
    })
}

/// Converts in this process. A panic in the parser takes this process with
/// it, so outside tests use [`ChildProcess`].
#[derive(Debug, Clone, Copy, Default)]
pub struct InProcess;

impl Converter for InProcess {
    fn convert(&self, input: &Path, output: &Path) -> Result<()> {
        let result = export(input, output);
        if result.is_err() {
            let _ = fs::remove_file(output);
        }
        result
    }
}

fn export(input: &Path, output: &Path) -> Result<()> {
    let failed = |message: String| Error::Amazon {
        input: input.to_path_buf(),
        message,
    };
    let book = match sniff(input)? {
        Some(Kind::Mobi) => open_mobi(input),
        Some(Kind::Kfx) => Book::open_format(input, Format::Kfx),
        Some(Kind::EncryptedKfx) => return Err(Error::DrmProtected(input.to_path_buf())),
        Some(Kind::Topaz) => return Err(failed("Topaz books aren't supported".to_owned())),
        None => return Err(failed("not an Amazon book".to_owned())),
    };
    let book = book.map_err(|e| match e {
        boko::Error::DrmProtected(_) => Error::DrmProtected(input.to_path_buf()),
        other => failed(other.to_string()),
    })?;
    let mut file = File::create(output)?;
    book.export(Format::Epub, &mut file)
        .map_err(|e| failed(e.to_string()))
}

/// Opens the KF8 half when there is one: it has the book's real HTML and CSS,
/// while the older half of a combo file has neither stylesheets nor the
/// publisher's structure. boko picks by extension, which can't tell them apart.
fn open_mobi(input: &Path) -> boko::Result<Book> {
    match Book::open_format(input, Format::Azw3) {
        Err(boko::Error::UnsupportedFormat { .. }) => Book::open_format(input, Format::Mobi),
        result => result,
    }
}

/// Converts in a child process of `exe`, which must call
/// [`run_child_if_asked`] first thing in `main`.
#[derive(Debug, Clone)]
pub struct ChildProcess {
    exe: PathBuf,
}

impl ChildProcess {
    pub fn new(exe: impl Into<PathBuf>) -> Self {
        Self { exe: exe.into() }
    }

    /// The running executable.
    pub fn current() -> Result<Self> {
        Ok(Self::new(env::current_exe()?))
    }
}

impl Converter for ChildProcess {
    fn convert(&self, input: &Path, output: &Path) -> Result<()> {
        let out = Command::new(&self.exe)
            .arg(CHILD_ARG)
            .arg(input)
            .arg(output)
            .output()?;
        let failed = |message: String| {
            let _ = fs::remove_file(output);
            Error::Amazon {
                input: input.to_path_buf(),
                message,
            }
        };
        match out.status.code() {
            Some(0) if output.is_file() => Ok(()),
            Some(EXIT_DRM) => Err(Error::DrmProtected(input.to_path_buf())),
            Some(EXIT_FAILED) => Err(failed(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            )),
            // The parser crashed; the app carries on without this book.
            _ => Err(failed(format!("the converter crashed ({})", out.status))),
        }
    }
}

/// Does the conversion and exits if this process is a [`ChildProcess`] child;
/// otherwise returns. Call it first thing in `main`.
pub fn run_child_if_asked() {
    let mut args = env::args_os().skip(1);
    if args.next().is_none_or(|arg| arg != CHILD_ARG) {
        return;
    }
    let (Some(input), Some(output)) = (args.next(), args.next()) else {
        eprintln!("usage: {CHILD_ARG} INPUT OUTPUT");
        process::exit(EXIT_FAILED);
    };
    match InProcess.convert(Path::new(&input), Path::new(&output)) {
        Ok(()) => process::exit(0),
        Err(Error::DrmProtected(_)) => process::exit(EXIT_DRM),
        Err(Error::Amazon { message, .. }) => {
            eprintln!("{message}");
            process::exit(EXIT_FAILED);
        }
        Err(e) => {
            eprintln!("{e}");
            process::exit(EXIT_FAILED);
        }
    }
}
