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

## Current

- M2 device check (user): send a real book from the app, eject, confirm it opens with KEPUB features and no `._*` file appears next to it.

## Unresolved

- S2 (cloud sync behaviour) deferred to M6.

## Next

- M3: local library (SQLite + managed folder), metadata read/edit (OPF), dedupe; device file names from metadata (`Author - Title`).

## Notes for agents

- Licence is GPLv3: port calibre's Kobo logic (src/calibre/devices/kobo/driver.py) where it helps; cite it in a comment.
- Git hooks via prek (`.pre-commit-config.yaml`; run `prek install` in a fresh clone): fmt/clippy/eslint/tsc/prettier/rumdl/zizmor/gitleaks on commit, conventional-commit check on the message, cargo test + vitest on push. Still run `just check` before committing. The justfile puts Homebrew's keg-only rustup on PATH; in raw shells prepend `/opt/homebrew/opt/rustup/bin`.
- Apple's `sqlite3` persists `-wal`/`-shm` after close; on the device use `.filectrl persist_wal 0` (CLI) or the equivalent `SQLITE_FCNTL_PERSIST_WAL` off in Rust, then `wal_checkpoint(TRUNCATE)`.
- SQL `LIKE` treats `_` as a wildcard: match literal paths with `instr()` (bit us once).
- macOS adds an unremovable `com.apple.provenance` xattr to every new file; on FAT32 it becomes `._<name>`. Delete the companion after writing (verified on device).
- Never write to `/Volumes/KOBOeReader` outside the safe-write layer or a user-run script.
