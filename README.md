<!-- rumdl-disable MD033 MD041 -->
<div align="center">

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="assets/readme-dark.svg">
  <source media="(prefers-color-scheme: light)" srcset="assets/readme-light.svg">
  <img alt="Parallax" src="assets/readme-light.svg" width="400">
</picture>

**A desktop library for reading books and comics and organising them on a Kobo.**

[![Latest version](https://img.shields.io/github/v/release/JacksonFergusonDev/parallax?style=flat-square&label=version&labelColor=0f172a&color=f5c568)](https://github.com/JacksonFergusonDev/parallax/releases/latest) ![macOS](https://img.shields.io/badge/platform-macOS-f5c568?style=flat-square&labelColor=0f172a&logo=apple&logoColor=white) [![CI](https://img.shields.io/github/actions/workflow/status/JacksonFergusonDev/parallax/ci.yml?branch=main&style=flat-square&labelColor=0f172a)](https://github.com/JacksonFergusonDev/parallax/actions/workflows/ci.yml) [![Rust](https://img.shields.io/badge/built_with-Rust-f5c568?style=flat-square&labelColor=0f172a&logo=rust&logoColor=white)](https://www.rust-lang.org/) [![Tauri](https://img.shields.io/badge/desktop-Tauri-f5c568?style=flat-square&labelColor=0f172a&logo=tauri&logoColor=white)](https://tauri.app/) [![GPLv3](https://img.shields.io/badge/license-GPLv3-f5c568?style=flat-square&labelColor=0f172a)](LICENSE)

| [Using Parallax](#using-it) | [Download](https://parallax.jacksonferguson.me/download/) | [Kobo compatibility](docs/device-support.md) | [Development](#development) |

</div>
<!-- rumdl-enable MD033 MD041 -->

## Overview

Import books, improve their metadata and covers, convert them to KEPUB, and manage the Kobo’s own collections. You can import DRM-free Kindle-format books. Support for Kindle devices is deferred.

Licensed under GPLv3. Device logic is ported from [calibre](https://calibre-ebook.com)'s Kobo driver; EPUB to KEPUB conversion uses [kepubify](https://pgaskin.net/kepubify/), and Amazon formats are read with [boko](https://github.com/zacharydenton/boko).

## Installing it

Download the app from [parallax.jacksonferguson.me](https://parallax.jacksonferguson.me), which also lists every earlier version. It runs on Apple silicon and Intel Macs, checks for new versions itself, and installs one only when you say so (**Parallax › Check for Updates…** checks now). In Settings › General you can turn automatic checks off, or have new versions download in the background so updating only needs a restart.

The command-line companion installs with Homebrew:

```bash
brew install jacksonfergusondev/tap/parallax-cli
```

or builds from source with Cargo (sending books then needs `kepubify` on your `PATH`):

```bash
cargo install --locked --git https://github.com/JacksonFergusonDev/parallax parallax-cli
```

## Using it

1. Drop EPUBs, DRM-free Kindle books (AZW3, MOBI, AZW, PRC, KFX), comic archives (CBZ, CBR, CB7), or ZIP, RAR and 7z bundles of books onto the window (or **Add Books…**, ⌘O). Bundles can contain folders of books. Edit title, authors, series, tags and cover in the side panel. Parallax leaves the original files untouched.
2. Connect your Kobo by USB and tap **Connect** on its screen.
3. Select books and **Send to Kobo**, optionally straight into a collection. Then **Eject Kobo** and unplug it so it imports them.
4. **Read** in the side panel opens a book in its own window and remembers where you stopped. **Search** (⌘F) finds a word or line in it. Select text to highlight it or add a note, listed under the pen button with **Copy all**; footnotes open over the page; **Back** (⌘[) returns after a jump; the speaker button reads aloud; **Aa** sets theme, size, pages or scrolling, line length, margins and spacing. Books are shown with their scripts blocked and no internet access.
5. The **On Kobo** tab (⌘2) shows what's on the device, with reading progress, and lets you manage collections, remove books, and turn folders into collections.

Choose **Library tools › Add comic folder…** to bundle pages or chapter archives as a volume. The comic reader supports spreads, vertical scroll, zoom and bookmarks. Parallax prepares comic pages for the Kobo screen when sending. See [device support](docs/device-support.md) for hardware verification and compatibility.

Every change to the Kobo's database is backed up first, compressed, to `~/Library/Application Support/dev.parallax.app/device-backups/`. Removed books are held on your Mac for 30 days, and the library is saved before removals and bulk changes. **Revert changes…**, in both the Library and On Kobo tabs, lists these points by time and goes back to one. How things are now is saved first, so a revert can be undone too.

When upgrading, Parallax moves the existing library and backups into the new app data folder if it does not already exist. Existing Kobo history stays readable and moves to `.parallax` the next time Parallax records a change.

## Development

Requires Rust (stable), Node and pnpm, [just](https://github.com/casey/just), and [uv](https://docs.astral.sh/uv/).

Run `just sync` to set up the checkout and `prek install` to enable the Git hooks. Commits run formatting and quick checks on changed files. Pushes run the slower builds, linters, type checks and tests. Run `just sync` after changing dependencies so the push hooks can verify the pnpm lockfiles offline.

```bash
just          # list available recipes
just sync     # native kepubify sidecar and locked UI dependencies
just dev      # run the app with hot reload
just dev-optimized # build and run it as a release behaves (slower to build, no hot reload)
just check-scripts # Python formatting, lint, types and tests
just ci       # run the local formatting, lint, type and test suite on demand
just test-fat # extra tests that mount FAT32 disk images
just test-reader # reader and Settings in a real macOS WebView (opens windows)
just build    # macOS .app bundle
```

The Python project and lockfile live in `scripts/`. uv selects Python 3.12 and installs the locked Ruff, mypy and pytest tools for `just check-scripts`. The Core ML converter uses a separate environment and is installed only when the model needs to be built.

Run `uv run --directory scripts --locked python make_reader_fixtures.py` to regenerate the reader EPUBs. Run `uv run --directory scripts --locked python make_amazon_fixtures.py` with calibre's `ebook-convert` on your PATH to regenerate the Amazon-format books. Both commands write into `tests/fixtures/`.

CI builds and tests the frontend on Ubuntu. Apple silicon and Intel Macs run the same Rust checks and executable builds. Two parallel Apple silicon jobs exercise EPUB reading and security, and Settings, menus, and comics in WKWebView. Locally, `just test-reader` runs all 11 cases. Run the **Portability** workflow from the Actions tab to test the Rust workspace and desktop builds on Linux and Windows on x64 and arm64. The Kobo tests use fixtures and temporary directories.

`just dev` prompts before stopping processes that occupy Vite’s port (1420). Accepting retries app startup without repeating setup; declining or closing input leaves those processes running.

`parallax-cli` is a command-line companion for scripting: `cargo run -p parallax-cli -- --help`. It shares the desktop app’s library and core operations.

The CLI uses `comfy-table` for device, book, collection, cover, rule and transfer-result tables; `console` for coloured status labels; and `indicatif` for transfer, Kobo-import and metadata-lookup progress bars on stderr. Redirected output uses plain tables and progress logs. Set `NO_COLOR=1` to disable colours; `--json` keeps existing JSON responses free of styling and suppresses progress.

```bash
cargo run -p parallax-cli -- library ls
cargo run -p parallax-cli -- library ls --json
cargo run -p parallax-cli -- library enhance  # preview metadata proposals
```

The site lives in `site/` (`just site` runs it, `just check-site` checks it). See [releasing](engineering/releasing.md) for cutting a release. See the [engineering index](engineering/README.md) for the design system, architecture decisions, and hardware verification. The [maintainer handoff](PROGRESS.md) tracks current work and release checks.

### README branding

Install Inkscape and the Space Grotesk font (including Bold), then run `scripts/build-readme-logos.sh` after editing the SVGs in `branding/`. Commit the regenerated `assets/readme-light.svg` and `assets/readme-dark.svg` with the sources. Inkscape converts the wordmark to paths so GitHub can display it without the font.

## Cover upscaling

"Upscale cover…" enlarges a cover and removes compression noise with [4xBHI_dat2_real](https://huggingface.co/Phips/4xBHI_dat2_real) by Philip Hofmann, used under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The weights are converted to Core ML at half precision with a fixed tile size. Every result is reviewed before and after, and nothing changes until it's kept. It's offered for covers under 1680 pixels tall, and needs macOS 15 or later.

The model isn't in the repository. `just upscale-model` builds it on Apple Silicon from the pinned upstream weights (about five minutes and 9 GB of memory, once), and `just build`, `just dev` and `just dev-optimized` run that for you. The app hides the feature when the model is missing.
