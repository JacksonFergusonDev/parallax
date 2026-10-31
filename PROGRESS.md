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

## Current

- M3 device check (user, batched): send a library book after editing its title/series and confirm the Kobo shows the new title and series.
- Next up: M4 covers.

## Unresolved

- S2 (cloud sync behaviour) deferred to M6.

## Next

- M4: replace cover from a local image; online cover search (Open Library, Google Books, Apple Books) with resolution shown; cover written into the exported EPUB.
- M5 must include: "replace on device" for books edited after sending (today a resend is refused).

## Notes for agents

- Licence is GPLv3: port calibre's Kobo logic (src/calibre/devices/kobo/driver.py) where it helps; cite it in a comment.
- Git hooks via prek (`.pre-commit-config.yaml`; run `prek install` in a fresh clone): fmt/clippy/eslint/tsc/prettier/rumdl/zizmor/gitleaks on commit, conventional-commit check on the message, cargo test + vitest on push. Still run `just check` before committing. The justfile puts Homebrew's keg-only rustup on PATH; in raw shells prepend `/opt/homebrew/opt/rustup/bin`.
- Apple's `sqlite3` persists `-wal`/`-shm` after close; on the device use `.filectrl persist_wal 0` (CLI) or the equivalent `SQLITE_FCNTL_PERSIST_WAL` off in Rust, then `wal_checkpoint(TRUNCATE)`.
- SQL `LIKE` treats `_` as a wildcard: match literal paths with `instr()` (bit us once).
- macOS adds an unremovable `com.apple.provenance` xattr to every new file; on FAT32 it becomes `._<name>`. Delete the companion after writing (verified on device).
- UI work: `preview_start ui-mock` serves the app in the browser pane with fake data (`app/src/dev/mockBackend.ts`). Stop it before `pnpm tauri dev` (same port).
- Never write to `/Volumes/KOBOeReader` outside the safe-write layer or a user-run script.
