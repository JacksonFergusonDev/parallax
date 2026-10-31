# Progress

Read this first each session. Plan: `docs/PLAN.md`. Device facts: `docs/device-notes.md`. Manual checks: `docs/hardware-checks.md`.

## Completed

- Planning: research + Kobo MVP plan; decisions recorded (PLAN §6).
- Read-only inspection of the reference Kobo (Libra 2, fw 4.38, DbVersion 174).
- Spike S1 on device: pre-import ShelfContent rows work immediately; triggers need a restart (dropped); replug reconcile works for series.
- M0: Cargo workspace, Tauri 2 + React 19/Vite app in `app/`, kepubify v4.0.4 sidecar (`scripts/fetch-kepubify.sh`, checksum-pinned), `just check` (fmt, clippy -D warnings, cargo test, tsc, eslint, prettier, vitest), GitHub Actions on macOS. `pnpm tauri dev` launches the window.
- M1 (code): `crates/device` (shared types + Capabilities), `crates/device-kobo` (detect, version parse, immutable read-only DB, books via calibre's KoboTouch filter, collections), `crates/cli` (`erc devices`, `erc kobo ls|collections [--mount|--db] [--json]`). Tests: synthetic DB from `tests/fixtures/kobo/schema-db174.sql` + private real DB (93 books).
- M1 verified on the device: `erc devices` / `erc kobo ls` match (93 books, 0 collections), DB checksum unchanged.
- M2 (code): `crates/convert` (Kepubify sidecar, `ERC_KEPUBIFY` override), Kobo transfer (temp name + rename, size check, no xattr copy, deletes the `._*` companions macOS creates on FAT32 for every new file/dir, `.metadata_never_index`), eject via `diskutil`, `crates/core::send` (per-book convert+copy with progress events), `erc send`, app UI (device badge polling, drag-and-drop/picker queue, Send, Eject). FAT32 disk-image test: `just test-fat`.
- M2 verified on the device (2026-10-31): book sent from the app, imported and opened as KEPUB; no `._*` file next to it. File name came from the source file name (long, ugly), fixed in M3.
- M3: `crates/epub` (OPF read for EPUB2/3 incl. calibre + EPUB3 series, cover extraction, byte-preserving metadata splice, raw zip copy; all 94 device books parse), `crates/library` (SQLite `user_version` migrations, managed `books/<id>/book.epub` + `cover.jpg` thumbnail, SHA-256 exact dedupe + ISBN/title-author likely-duplicate, author normalisation, export with edits, `device_copies`), `send_library_books` (device names `KEpub/<Title> - <Author>.kepub.epub`, records copies, refuses a second copy of a renamed book), `erc library add|ls|edit|rm|send`, `erc epub-info`, app library UI (cover grid, search, multi-select, edit panel, per-book send status, On Kobo badges). Browser mock backend for UI work: `.claude/launch.json` `ui-mock` (dev only, not bundled).
- M4: covers. `erc-epub` writes a replacement cover (same media type → bytes swapped in place so title pages update; otherwise a new `erc-cover.*` item + `meta name=cover` + EPUB3 `cover-image` moved). `erc-library` schema v2 `custom_cover`, `set_cover` (capped at 1680 px tall, JPEG q90), `reset_cover`, export re-encodes to PNG when the original cover is PNG. `erc-covers`: Apple Books (`3000x3000bb` artwork, returns up to original size), Open Library (search + ISBN), Google Books (keyless quota often exhausted → 429, skipped silently). CLI `erc library covers|cover|export`; app cover picker modal with measured resolution (soft < 1200 px tall), choose local image, reset.
- M5: `DeviceWriter` safe-write session (refuses leftover `-wal/-shm/-journal` and DbVersion outside calibre's 53–222; `quick_check`; SQLite online backup to `<app data>/device-backups/<serial>/`, newest 10 kept; immediate transactions; `integrity_check` on close with automatic restore; WAL checkpoint; removes `._` companions of the DB). Ops: `remove_book` (calibre `delete_via_sql` tables + file + `.kobo-images` cache via `qhash` + empty folders), `replace_book` (same path → progress kept; updates Title/Attribution/Series/SeriesID; clears cover cache so Nickel regenerates), `tidy_apple_double` (magic-number checked, skips `.kobo*`). Core `update_on_device`, `remove_from_device`, `books_on_device` (stale = edited after send). CLI `erc kobo rm|tidy|backups|restore`, `erc library update`. App: Library / On Kobo tabs, device list with status/last read/added, multi-select remove, "Update available" badges + "Update N on Kobo", native confirm dialogs (`dialog:allow-message`; `window.confirm` is unreliable in WKWebView). Tests: fixture DB, real DB copy, FAT32 image (`just test-fat`).
- M6: `device-kobo::collections` (create/undelete, add — pre-import OK, remove, rename, delete: unsynced hard-deleted, synced soft-deleted; Shortlist/Wishlist/SystemTag protected; ported from calibre). `set_series` shared with replace. Core `collections::apply` (all changes in one transaction), `folder_groups`, `pending_series`/`reconcile_series` (no write session when nothing pending). CLI `erc kobo collection create|rename|delete|add|remove`, `erc kobo folders [--apply]`, `erc library reconcile`, `erc library send --collection`. App: Kobo tab collections sidebar (filter, rename, delete, new), Add to collection menu, Remove from collection, Folders → collections modal; library send-into-collection picker; series reconciled automatically on connect. Dry run on the real Kobo: Fantasy 20, Fiction 8, Horror 1, Nonfiction 40, Sci-Fi 24.

## Current

- Batched device checks for M3–M6 + spike S2 (user): docs/hardware-checks.md.
- Next up: M7 UI polish pass.

## Unresolved

- S2 (cloud sync) checklist written; awaiting a device run.

## Next

- M7: polish pass — startup/empty/error states, keyboard shortcuts, virtualised grid for large libraries, app icon and name, window state, consistent notices (toasts), accessibility pass, sort options.
- M8 (stretch): automatic grouping rules (series → collection, tag → collection) with preview.
- Google Books needs an API key to be useful; optional setting later.

## Notes for agents

- Licence is GPLv3: port calibre's Kobo logic (src/calibre/devices/kobo/driver.py) where it helps; cite it in a comment.
- Git hooks via prek (`.pre-commit-config.yaml`; run `prek install` in a fresh clone): fmt/clippy/eslint/tsc/prettier/rumdl/zizmor/gitleaks on commit, conventional-commit check on the message, cargo test + vitest on push. Still run `just check` before committing. The justfile puts Homebrew's keg-only rustup on PATH; in raw shells prepend `/opt/homebrew/opt/rustup/bin`.
- Apple's `sqlite3` persists `-wal`/`-shm` after close; on the device use `.filectrl persist_wal 0` (CLI) or the equivalent `SQLITE_FCNTL_PERSIST_WAL` off in Rust, then `wal_checkpoint(TRUNCATE)`.
- SQL `LIKE` treats `_` as a wildcard: match literal paths with `instr()` (bit us once).
- macOS adds an unremovable `com.apple.provenance` xattr to every new file; on FAT32 it becomes `._<name>`. Delete the companion after writing (verified on device).
- UI work: `preview_start ui-mock` serves the app in the browser pane with fake data (`app/src/dev/mockBackend.ts`). Stop it before `pnpm tauri dev` (same port).
- Never write to `/Volumes/KOBOeReader` outside the safe-write layer or a user-run script.
