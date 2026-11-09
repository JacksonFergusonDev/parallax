//! Ebook conversion. KEPUB comes from kepubify, run as a sidecar binary, and
//! Amazon formats become EPUB through [`amazon`]; [`ToKepub`] chains the two.
//! The [`Converter`] trait keeps callers independent of both so a native
//! implementation can replace them later.

pub mod amazon;

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("kepubify binary not found (looked in: {0})")]
    NotFound(String),
    #[error("could not run kepubify: {0}")]
    Spawn(std::io::Error),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("kepubify failed to convert {input}: {message}")]
    Failed { input: PathBuf, message: String },
    #[error("{} is DRM-protected", .0.display())]
    DrmProtected(PathBuf),
    #[error("could not convert {input}: {message}", input = input.display())]
    Amazon { input: PathBuf, message: String },
}

pub type Result<T> = std::result::Result<T, Error>;

pub trait Converter {
    /// Converts `input` and writes the result to `output`, which must not exist.
    fn convert(&self, input: &Path, output: &Path) -> Result<()>;

    /// Converts a fixed-layout EPUB without inserting a dummy title page.
    fn convert_fixed_layout(&self, input: &Path, output: &Path) -> Result<()> {
        self.convert(input, output)
    }
}

/// Environment variable that overrides where kepubify is found.
pub const KEPUBIFY_ENV: &str = "ERC_KEPUBIFY";

#[derive(Debug, Clone)]
pub struct Kepubify {
    binary: PathBuf,
}

impl Kepubify {
    pub fn new(binary: impl Into<PathBuf>) -> Self {
        Self {
            binary: binary.into(),
        }
    }

    /// Finds kepubify: `$ERC_KEPUBIFY`, then next to the running executable
    /// (where Tauri places sidecars, both in bundles and in `target/`), then
    /// the repo's downloaded sidecar for development builds.
    pub fn locate() -> Result<Self> {
        let mut candidates = Vec::new();
        if let Some(path) = env::var_os(KEPUBIFY_ENV) {
            candidates.push(PathBuf::from(path));
        }
        if let Some(dir) = env::current_exe()
            .ok()
            .and_then(|exe| exe.parent().map(Path::to_path_buf))
        {
            candidates.push(dir.join("kepubify"));
        }
        candidates.push(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../app/src-tauri/binaries")
                .join(format!("kepubify-{}", target_triple())),
        );
        candidates
            .iter()
            .find(|p| p.is_file())
            .map(Self::new)
            .ok_or_else(|| {
                let looked: Vec<String> =
                    candidates.iter().map(|p| p.display().to_string()).collect();
                Error::NotFound(looked.join(", "))
            })
    }

    pub fn binary(&self) -> &Path {
        &self.binary
    }
    fn run(&self, input: &Path, output: &Path, fixed_layout: bool) -> Result<()> {
        let mut command = Command::new(&self.binary);
        if fixed_layout {
            command.arg("--no-add-dummy-titlepage");
        }
        let out = command
            .arg("--output")
            .arg(output)
            .arg(input)
            .output()
            .map_err(Error::Spawn)?;
        // kepubify reports per-file errors in its summary and may still exit 0,
        // so the output file existing is the real success signal.
        if out.status.success() && output.is_file() {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&out.stderr);
        let stdout = String::from_utf8_lossy(&out.stdout);
        let message = stderr
            .lines()
            .chain(stdout.lines())
            .map(str::trim)
            .filter(|l| l.to_lowercase().contains("error"))
            .collect::<Vec<_>>()
            .join("; ");
        Err(Error::Failed {
            input: input.to_path_buf(),
            message: if message.is_empty() {
                format!("exit status {}", out.status)
            } else {
                message
            },
        })
    }
}

impl Converter for Kepubify {
    fn convert(&self, input: &Path, output: &Path) -> Result<()> {
        self.run(input, output, false)
    }

    fn convert_fixed_layout(&self, input: &Path, output: &Path) -> Result<()> {
        self.run(input, output, true)
    }
}

/// Any book to KEPUB: Amazon formats go through EPUB first, as kepubify only
/// reads EPUB.
pub struct ToKepub {
    amazon: Box<dyn Converter>,
    kepub: Box<dyn Converter>,
}

impl ToKepub {
    pub fn new(amazon: impl Converter + 'static, kepub: impl Converter + 'static) -> Self {
        Self {
            amazon: Box::new(amazon),
            kepub: Box::new(kepub),
        }
    }

    /// The real converters: kepubify, and boko in a child of this executable.
    pub fn locate() -> Result<Self> {
        Ok(Self::new(
            amazon::ChildProcess::current()?,
            Kepubify::locate()?,
        ))
    }
}

impl Converter for ToKepub {
    fn convert(&self, input: &Path, output: &Path) -> Result<()> {
        if amazon::sniff(input)?.is_none() {
            return self.kepub.convert(input, output);
        }
        let workdir = tempfile::tempdir()?;
        let epub = workdir.path().join("book.epub");
        self.amazon.convert(input, &epub)?;
        self.kepub.convert(&epub, output)
    }

    fn convert_fixed_layout(&self, input: &Path, output: &Path) -> Result<()> {
        self.kepub.convert_fixed_layout(input, output)
    }
}

fn target_triple() -> &'static str {
    match (env::consts::ARCH, env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "windows") => "x86_64-pc-windows-msvc.exe",
        _ => "unknown",
    }
}
