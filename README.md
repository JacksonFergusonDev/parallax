# Parallax

![Parallax icon](app/src-tauri/icons/64x64.png)

A focused desktop app for getting EPUBs onto a Kobo and keeping them organised there: import, fix metadata and covers, convert to KEPUB, send, and manage the Kobo's own collections. Kindle support is planned.

Licensed under GPLv3. Device logic is ported from [calibre](https://calibre-ebook.com)'s Kobo driver; EPUB to KEPUB conversion uses [kepubify](https://pgaskin.net/kepubify/), and Amazon formats are read with [boko](https://github.com/zacharydenton/boko).

## Using it

1. Drop EPUB files or DRM-free Kindle books (AZW3, MOBI, AZW, PRC, KFX, converted to EPUB) onto the window (or **Add books…**, ⌘O). Edit title, authors, series, tags and cover in the side panel. Your original files are never modified.
2. Connect your Kobo by USB and tap **Connect** on its screen.
3. Select books and **Send to Kobo**, optionally straight into a collection. Then **Eject Kobo** and unplug it so it imports them.
4. **Read** in the side panel opens a book in its own window and remembers where you stopped. **Search** (⌘F) finds a word or line in it. Select text to highlight it or add a note, listed under the pen button with **Copy all**; footnotes open over the page; **Back** (⌘[) returns after a jump; the speaker button reads aloud; **Aa** sets theme, size, pages or scrolling, line length, margins and spacing. Books are shown with their scripts blocked and no internet access.
5. The **On Kobo** tab (⌘2) shows what's on the device, with reading progress, and lets you manage collections, remove books, and turn folders into collections.

Every change to the Kobo's database is backed up first, compressed, to `~/Library/Application Support/dev.erc.companion/device-backups/`. Removed books are held on your Mac for 30 days, and the library is saved before removals and bulk changes. **Revert changes…**, in both the Library and On Kobo tabs, lists these points by time and goes back to one. How things are now is saved first, so a revert can be undone too.

Brand and UI rules: [Parallax design system](docs/design-system.md).

## Development

Requires Rust (stable), Node and pnpm, and [just](https://github.com/casey/just).

```bash
just setup    # kepubify sidecar + UI dependencies
just dev      # run the app with hot reload
just dev-optimized # build and run it as a release behaves (slower to build, no hot reload)
just check    # formatting, lints, type checks and tests (run before committing)
just test-fat # extra tests that mount FAT32 disk images
just test-reader # the reader in a real build, with a hostile book (opens windows)
just build    # macOS .app bundle
```

`just dev` prompts before stopping processes that occupy Vite’s port (1420). Accepting retries app startup without repeating setup; declining or closing input leaves those processes running.

`erc` is a command-line companion for development and scripting: `cargo run -p erc-cli -- --help`. It shares the desktop app’s library and core operations.

The CLI uses `comfy-table` for device, book, collection, cover, rule and transfer-result tables; `console` for coloured status labels; and `indicatif` for transfer, Kobo-import and metadata-lookup progress bars on stderr. Redirected output uses plain tables and progress logs. Set `NO_COLOR=1` to disable colours; `--json` keeps existing JSON responses free of styling and suppresses progress.

```bash
cargo run -p erc-cli -- library ls
cargo run -p erc-cli -- library ls --json
cargo run -p erc-cli -- library enhance  # preview metadata proposals
```

Project plan and progress: [docs/PLAN.md](docs/PLAN.md), [PROGRESS.md](PROGRESS.md). Device findings: [docs/device-notes.md](docs/device-notes.md). Manual device checks: [docs/hardware-checks.md](docs/hardware-checks.md). App caching and invalidation: [docs/app-caching.md](docs/app-caching.md).

## Cover upscaling

"Upscale cover…" enlarges a cover and removes compression noise with [4xBHI_dat2_real](https://huggingface.co/Phips/4xBHI_dat2_real) by Philip Hofmann, used under [CC BY 4.0](https://creativecommons.org/licenses/by/4.0/). The weights are converted to Core ML at half precision with a fixed tile size. Every result is reviewed before and after, and nothing changes until it's kept. It's offered for covers under 1680 pixels tall, and needs macOS 15 or later.

The model isn't in the repository. `just upscale-model` builds it from the pinned upstream weights (about five minutes and 9 GB of memory, once), and `just build`, `just dev` and `just dev-optimized` run that for you. The app hides the feature when the model is missing.
