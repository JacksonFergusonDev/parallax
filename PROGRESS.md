# Progress

Read this first each session. Plan: `docs/PLAN.md`. Device facts: `docs/device-notes.md`. Manual checks: `docs/hardware-checks.md`.

## Completed
- Planning: research + Kobo MVP plan; decisions recorded (PLAN §6).
- Read-only inspection of the reference Kobo (Libra 2, fw 4.38, DbVersion 174).
- Spike S1 on device: pre-import ShelfContent rows work immediately; triggers need a restart (dropped); replug reconcile works for series.
- M0: Cargo workspace, Tauri 2 + React 19/Vite app in `app/`, kepubify v4.0.4 sidecar (`scripts/fetch-kepubify.sh`, checksum-pinned), `just check` (fmt, clippy -D warnings, cargo test, tsc, eslint, prettier, vitest), GitHub Actions on macOS. `pnpm tauri dev` launches the window.

- M1 (code): `crates/device` (shared types + Capabilities), `crates/device-kobo` (detect, version parse, immutable read-only DB, books via calibre's KoboTouch filter, collections), `crates/cli` (`erc devices`, `erc kobo ls|collections [--mount|--db] [--json]`). Tests: synthetic DB from `tests/fixtures/kobo/schema-db174.sql` + private real DB (93 books).

## Current
- M1 device check: run `erc devices` and `erc kobo ls` against the plugged-in Kobo (read-only; Claude can do it when the Kobo is connected) and confirm the DB checksum is unchanged.
- Then M2: thin end-to-end send.

## Unresolved
- S2 (cloud sync behaviour) deferred to M6.

## Next
- M2 plan: `crates/convert` (Converter trait; kepubify sidecar from `app/src-tauri/binaries/kepubify-<triple>`, locate via env `ERC_KEPUBIFY` for CLI/tests), `crates/device-kobo` transfer (copy to `KEpub/` with `cp -X`-equivalent: plain read/write, no xattrs; temp name + rename; size check; `.metadata_never_index`), eject via `diskutil eject`. `erc send <epub>...`. Minimal UI: drop zone, device badge, Send button, progress/error toasts via Tauri events.

## Notes for agents
- Licence is GPLv3: port calibre's Kobo logic (src/calibre/devices/kobo/driver.py) where it helps; cite it in a comment.
- Run `just check` before committing. The justfile puts Homebrew's keg-only rustup on PATH; in raw shells prepend `/opt/homebrew/opt/rustup/bin`.
- Apple's `sqlite3` persists `-wal`/`-shm` after close; on the device use `.filectrl persist_wal 0` (CLI) or the equivalent `SQLITE_FCNTL_PERSIST_WAL` off in Rust, then `wal_checkpoint(TRUNCATE)`.
- SQL `LIKE` treats `_` as a wildcard: match literal paths with `instr()` (bit us once).
- Never write to `/Volumes/KOBOeReader` outside the safe-write layer or a user-run script.
