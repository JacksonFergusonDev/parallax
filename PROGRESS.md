# Progress

Read this first each session. Plan: `docs/PLAN.md`. Device facts: `docs/device-notes.md`. Manual checks: `docs/hardware-checks.md`.

## Completed
- Planning: research + Kobo MVP plan; decisions recorded (PLAN §6).
- Read-only inspection of the reference Kobo (Libra 2, fw 4.38, DbVersion 174).
- Spike S1 run on device: pre-import ShelfContent rows work immediately; triggers need a restart (dropped); replug reconcile works for series.

## Current
- M0 scaffold.

## Unresolved
- Licence (PLAN §6.7): decides whether calibre code is ported or only used as reference.
- S2 (cloud sync behaviour) deferred to M6.

## Next
- M0: workspace + Tauri/React scaffold, `just check`, CI. Toolchain: prepend `/opt/homebrew/opt/rustup/bin` to PATH in shells (rustup is keg-only); use pnpm.

## Notes for agents
- Apple's `sqlite3` persists `-wal`/`-shm` after close; on the device use `.filectrl persist_wal 0` (CLI) or the equivalent `SQLITE_FCNTL_PERSIST_WAL` off in Rust, then `wal_checkpoint(TRUNCATE)`.
- Never write to `/Volumes/KOBOeReader` outside the safe-write layer or a user-run script.
