# E-Reader Companion

A focused desktop app for getting EPUBs onto a Kobo and keeping them organised there: import, fix metadata and covers, convert to KEPUB, send, and manage the Kobo's own collections. Kindle support is planned.

Licensed under GPLv3. Device logic is ported from [calibre](https://calibre-ebook.com)'s Kobo driver; EPUB to KEPUB conversion uses [kepubify](https://pgaskin.net/kepubify/).

## Using it

1. Drop EPUB files onto the window (or **Add books…**, ⌘O). Edit title, authors, series, tags and cover in the side panel. Your original files are never modified.
2. Connect your Kobo by USB and tap **Connect** on its screen.
3. Select books and **Send to Kobo**, optionally straight into a collection. Then **Eject Kobo** and unplug it so it imports them.
4. The **On Kobo** tab (⌘2) shows what's on the device, with reading progress, and lets you manage collections, remove books, and turn folders into collections.

Every change to the Kobo's database is backed up first, to `~/Library/Application Support/dev.erc.companion/device-backups/`.

## Development

Requires Rust (stable), Node and pnpm, and [just](https://github.com/casey/just).

```bash
just setup    # kepubify sidecar + UI dependencies
just dev      # run the app with hot reload
just check    # formatting, lints, type checks and tests (run before committing)
just test-fat # extra tests that mount FAT32 disk images
just build    # macOS .app bundle
```

`just dev` prompts before stopping processes that occupy Vite’s port (1420). Accepting retries app startup without repeating setup; declining or closing input leaves those processes running.

`erc` is a command-line companion for development and scripting: `cargo run -p erc-cli -- --help`.

Project plan and progress: [docs/PLAN.md](docs/PLAN.md), [PROGRESS.md](PROGRESS.md). Device findings: [docs/device-notes.md](docs/device-notes.md). Manual device checks: [docs/hardware-checks.md](docs/hardware-checks.md). App caching and invalidation: [docs/app-caching.md](docs/app-caching.md).
