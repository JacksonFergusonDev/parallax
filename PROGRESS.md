# Progress

Read this first each session. Plan: `docs/PLAN.md`. Device facts: `docs/device-notes.md`. Manual checks: `docs/hardware-checks.md`.

## Completed
- Planning: research + Kobo MVP plan; decisions recorded (PLAN §6).
- Read-only inspection of the reference Kobo (Libra 2, fw 4.38, DbVersion 174).
- Spike S1 on device: pre-import ShelfContent rows work immediately; triggers need a restart (dropped); replug reconcile works for series.
- M0: Cargo workspace, Tauri 2 + React 19/Vite app in `app/`, kepubify v4.0.4 sidecar (`scripts/fetch-kepubify.sh`, checksum-pinned), `just check` (fmt, clippy -D warnings, cargo test, tsc, eslint, prettier, vitest), GitHub Actions on macOS. `pnpm tauri dev` launches the window.

## Current
- M1: Kobo read-only (`crates/device`, `crates/device-kobo`, `crates/cli`).

## Unresolved
- S2 (cloud sync behaviour) deferred to M6.

## Next
- M1 plan: detect `/Volumes/*/.kobo/version`; parse version (serial, fw, model id); open DB copy (`immutable=1` or temp copy); list books + shelves + reading status; `erc kobo ls`; fixtures: synthetic DB built from the real schema (`sqlite3 tests/fixtures/private/*.sqlite .schema`), private real DB used in an ignored-if-missing test.

## Notes for agents
- Licence is GPLv3: port calibre's Kobo logic (src/calibre/devices/kobo/driver.py) where it helps; cite it in a comment.
- Run `just check` before committing. The justfile puts Homebrew's keg-only rustup on PATH; in raw shells prepend `/opt/homebrew/opt/rustup/bin`.
- Apple's `sqlite3` persists `-wal`/`-shm` after close; on the device use `.filectrl persist_wal 0` (CLI) or the equivalent `SQLITE_FCNTL_PERSIST_WAL` off in Rust, then `wal_checkpoint(TRUNCATE)`.
- Never write to `/Volumes/KOBOeReader` outside the safe-write layer or a user-run script.
