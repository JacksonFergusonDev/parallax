# Kobo MVP — Implementation Plan

Status: **approved** (decisions recorded in §6); M0 starts after spike S1. Source brief: `ereader-companion-project.md`.

## 1. Research findings that shape the design

| Area | Finding | Consequence |
|---|---|---|
| EPUB → KEPUB | `kepubify` (Go, MIT, pgaskin) is the de-facto converter, used by calibre plugins and Kobo users. Latest release v4.0.4 (2022) — stable, effectively feature-complete. No Rust port exists. | Ship kepubify as a **Tauri sidecar binary** behind a `Converter` trait. A native Rust port stays possible later without touching callers. |
| Device storage | Kobo mounts as USB mass storage (`/Volumes/KOBOeReader` on macOS) with a hidden `.kobo/` dir: `KoboReader.sqlite` (library DB), `version` (serial, firmware, model). | Detection = scan mounted volumes for `.kobo/version`. No MTP needed for Kobo. |
| Import timing | Nickel only adds sideloaded files to `content` **after USB eject** (library rescan). calibre therefore can only touch DB rows of books imported on a *previous* connection. | Anything DB-side for a freshly sent book (collections, series) must be **deferred**: either applied on next connect, or applied by Nickel itself via triggers (below). |
| Pending metadata via triggers | kepubify's `seriesmeta` installs a side table plus `AFTER INSERT/UPDATE ON content` triggers in `KoboReader.sqlite`, so series metadata lands the moment Nickel imports the book. Proven in the wild. | Same pattern can apply pending **collection membership** (`ShelfContent`) and series at import time, giving "send + collect in one step". Must be validated on hardware (Spike S1). |
| Prior art: calibre | calibre's KoboTouch driver (GPLv3, ~5k lines Python) and the Kobo Utilities plugin encode years of firmware knowledge: shelf SQL (`Id = InternalName = Name`, `Type='UserTag'`), delete cleanup across `Bookmark/ShelfContent/content/content_settings/ratings/Activity`, cover upload to `.kobo-images`, supported DbVersion up to 222 / fw up to 6.0. | Treat calibre as the **reference specification** for every device write: read the corresponding calibre function first, mirror its SQL and edge cases, cite it in comments. Whether code is *ported* verbatim depends on the licence decision (§6.7). |
| Collections | `Shelf` (name, flags `_IsDeleted`, `_IsSynced`, `_IsVisible`) and `ShelfContent` (ShelfName, ContentId = `file:///mnt/onboard/...`). | Native collection management is direct, transactional DB writes. |
| Kobo cloud sync | Shelf names sync to the Kobo account; sideloaded books inside them do not. Deleted shelves have been reported to reappear from the account. | Soft-delete via `_IsDeleted=true` (as Kobo Utilities does) rather than hard `DELETE`; surface a note for sync-enabled users. |
| Series | Stock firmware does not reliably read `calibre:series` from sideloaded EPUBs; tools write `content.Series/SeriesNumber/SeriesID` directly. | Write series both into the OPF (portable) and into the DB (what the device displays). |
| Reading data | `content` holds `ReadStatus`, `___PercentRead`, `DateLastRead`, `TimeSpentReading`; `Event` table holds more. | Read-only reading status is cheap to show in the MVP; analytics deferred. |
| macOS on FAT32 | Finder/macOS writes `._*` AppleDouble and `.Spotlight-V100` files; Kobo imports `._book.kepub.epub` as broken books. | Copy without xattrs, clean `._*` files we created, drop `.metadata_never_index` on the volume. |
| Kindle (phase 2) | 2024+ Kindles use **MTP**, not mass storage; sideloading needs AZW3/MOBI or Amazon's Send-to-Kindle EPUB pipeline. | Keep a `Device` trait with capability flags now; don't assume a filesystem path for every device. |
| Toolchain | Rust 1.99 stable is installed via Homebrew rustup (keg-only, so `/opt/homebrew/opt/rustup/bin` must be on PATH). Node 26 + npm present; Xcode CLT present. | Use **npm** (no pnpm) and a **Makefile** (no `just`). kepubify fetched as a prebuilt release binary (no Go needed). |

## 2. Architecture

```
calibre-rewrite/
├─ crates/
│  ├─ epub/          OPF read/edit, cover swap, safe re-zip (zip + quick-xml). No Tauri deps.
│  ├─ convert/       Converter trait; KepubifySidecar impl (spawns binary, captures errors).
│  ├─ library/       Local library: SQLite (rusqlite, bundled) + managed book folder, import, dedupe, metadata, covers.
│  ├─ covers/        Online cover search (Open Library, Google Books, Apple Books search) + image checks (image crate).
│  ├─ device/        Device trait, Capabilities {collections, series, reading_stats, ...}, detection registry.
│  ├─ device-kobo/   Detect, version parse, DB read, safe-write layer, transfer, shelves, pending-op triggers.
│  └─ cli/           `erc` dev CLI over all crates — the primary E2E test harness for agents.
├─ app/
│  ├─ src-tauri/     Thin command/event layer over the crates (Tauri 2).
│  └─ ui/            React + TS + Vite; TanStack Query for command data, virtualized cover grid.
├─ tests/fixtures/   Sample EPUBs (public domain), synthetic KoboReader.sqlite fixtures.
└─ docs/  PLAN.md, device notes, hardware test checklists
```

Key principles:

- **Headless core.** All logic lives in plain Rust crates exercised by unit tests and the `erc` CLI. Tauri is a thin adapter; the UI never holds business logic. This keeps most milestones verifiable without screenshots or a device.
- **Device isolation.** `device` defines the trait and capability flags; Kobo and (later) Kindle are separate crates. UI shows on-device vs local-only actions based on capabilities, never by device-type checks.
- **Originals are immutable.** Imported EPUBs are copied into the managed library untouched; metadata/cover edits are stored in the library DB and applied when producing the export (edited EPUB → KEPUB). Re-exports are reproducible.
- **Safe device writes** (one module, every write goes through it):
  1. Refuse if `DBVersion` is outside the tested range (read-only mode instead).
  2. Back up `KoboReader.sqlite` via SQLite online backup API to the app data dir (timestamped, keep last N).
  3. `PRAGMA integrity_check` before; single transaction; integrity check after; restore on failure.
  4. Operation journal in the library DB: intended changes, applied state — so interrupted syncs/transfers are resumable and idempotent.
  5. File transfers write to a temp name then rename; verify size/hash; fsync; eject via `diskutil eject`.
- **Deferred device ops.** Collections/series for not-yet-imported books are stored as pending ops, applied either by installed triggers (if S1 passes) or reconciled on next connect (always, as fallback).

Main dependencies: `tauri 2`, `rusqlite` (bundled), `zip`, `quick-xml`, `image`, `reqwest`, `serde`, `thiserror`, `tracing`, `tempfile`, `sha2`; UI: `react`, `@tanstack/react-query`, `@tanstack/react-virtual`, a small headless component lib (Radix). Package manager: npm. Task runner: Makefile.

## 3. Milestones

Each milestone is sized for ~1 session, ends with green `make check` (fmt, clippy -D warnings, cargo test, tsc, eslint, vitest), a commit, a push, and a `PROGRESS.md` update. 🔌 = needs a real-device check by the user.

| # | Milestone | Done when |
|---|---|---|
| M0 | **Scaffold** — toolchain, Cargo workspace, Tauri 2 + React/Vite shell, `Makefile`, GitHub Actions (macOS), fixture EPUBs, kepubify sidecar fetch script with checksum pin. | `make check` green locally and in CI; empty app window launches. |
| M1 | **Kobo read-only** — volume detection, `version` parse, open a *copy* of the DB read-only, list books (title, author, status, %, last read), list shelves. `erc kobo ls`. Synthetic DB fixtures + an anonymized real DB fixture if available. | Tests pass on fixtures. 🔌 `erc kobo ls` on your device matches what the Kobo shows. |
| M2 | **Thin end-to-end (earliest working workflow)** — `erc send book.epub`: EPUB → kepubify → copy to `/Volumes/KOBOeReader/<dir>` with xattr hygiene → eject. Minimal UI: drop zone, connected-device badge, Send button, progress/error toasts. | 🔌 Book appears and opens on the Kobo with kepub features (reading stats, page counts). |
| M3 | **Local library + metadata** — library DB + managed folder, multi-file drag-and-drop import, dedupe (content hash, then identifier/title+author), edit title/author/series/tags, OPF rewrite on export, round-trip tests across EPUB2/EPUB3 fixtures. | Edited metadata survives export → kepubify → re-read. |
| M4 | **Covers** — replace from local image; online search (Open Library, Google Books, Apple Books) with resolution shown; cover written into OPF manifest correctly for EPUB2/3; thumbnails cached for the grid. | Fixture tests for cover swap; 🔌 new cover visible on device after send. |
| M5 | **Safe-write layer + device management** — backup/integrity/transaction/journal, schema version gate, remove book from device (file + `content`/`ShelfContent` cleanup as calibre does), duplicate-on-device detection, disconnect-mid-transfer handling. | Fault-injection tests (kill mid-write, yank volume simulation) leave DB valid; 🔌 delete on device is clean. |
| M6 | **Native collections** — create/rename/soft-delete shelves, add/remove books (bulk), series → DB, pending ops for unimported books via triggers or next-connect reconcile (per S1 result). UI: collection sidebar, multi-select bulk assign. Includes one-click "device folders → collections" (your 93 books sit in genre folders the Kobo ignores). | 🔌 Send 3 books into a new collection in one action → after eject they appear in that collection on device. |
| M7 | **UI polish pass** — cover grid view, instant search/filter, library vs device views, reading status badges, clear local-only vs on-device labelling, empty/error states, startup time budget. | Manual UX review checklist + vitest for state logic. |
| M8 | **Auto-grouping rules (MVP stretch)** — "series → collection", "tag → collection" rules, previewed before applying. | Rule engine unit tests; 🔌 apply rules to a real device. |

### Spikes (hardware experiments that de-risk design choices)

| Spike | Question | Status |
|---|---|---|
| **S1** | Can a just-sent book land in a collection without a replug? Tests (A) row written pre-import, (B) import trigger, (C) replug + series baseline. | Script `scripts/spike-s1.sh`, procedure in `docs/hardware-checks.md`. Simulated on a DB copy: passes. **Awaiting device run.** |
| S2 | Do collections survive Kobo cloud sync with Wi-Fi on, and does soft-delete propagate? | Deferred to M6; needs a throwaway collection on a synced account. |

Design rule from S1, whatever the result: the **replug flow (C) is always implemented** as the fallback and for reconciliation. A or B is layered on top only if the device proves it works, so "send to collection" becomes one step.

## 4. Significant risks

1. **Kobo DB schema/firmware drift** — undocumented schema; Nickel may change it. Mitigate: version gate, read-only fallback, column-existence checks, fixtures per firmware.
2. **Device DB corruption** — highest-impact failure. Mitigate: safe-write layer (M5) lands *before* any shelf writes (M6); backups always; never write while device isn't in USB mode.
3. **Installing triggers into Nickel's DB** — proven by seriesmeta but is a persistent modification; firmware updates could drop or conflict with them. Mitigate: namespaced (`_erc_*`), uninstallable, fallback to next-connect reconcile.
4. **Kobo cloud sync** interfering with collections. Mitigate: soft-deletes, documentation, spike S2.
5. **kepubify sidecar** — unmaintained-but-stable Go binary; needs code signing/notarization inside the macOS bundle; universal binary (arm64 + x64). Mitigate: checksum-pinned download, Converter trait for a future Rust port.
6. **macOS FAT32 artefacts** (`._*` files) creating ghost books. Mitigate in M2 and test explicitly.
7. **Verification gap** — I can't touch hardware; 🔌 checkpoints depend on you, and an unverified feature will be reported as such.
8. **Online cover sources** — API terms/rate limits, quality variance; keep providers pluggable, no scraping of sites that forbid it.

## 5. Usage-efficiency rules for autonomous sessions

- Start every session by reading `PROGRESS.md` only (plus files it points to); don't re-explore.
- One milestone (or one clearly bounded half) per session; commit at green checkpoints so an interrupted session loses little.
- Prefer CLI/unit-test verification over UI screenshots; do one UI visual check per UI milestone.
- Fixtures and the `erc` CLI are the shared test harness — invest in them early (M0–M1).
- Hardware checks are batched into a checklist in `docs/hardware-checks.md` so you can run several at once.

## 6. Decisions

1. Hardware: Kobo Libra 2, fw 4.38.23697, DbVersion 174, account sync on (`docs/device-notes.md`). A copy of the real DB is kept as a gitignored local fixture in `tests/fixtures/private/`.
2. Library: app-managed desktop library.
3. Pending collections: replug flow always; trigger or pre-import row added if S1 passes.
4. Converter: kepubify sidecar for the MVP.
5. Online covers: Open Library, Google Books and Apple Books search APIs approved.
6. Distribution: unsigned personal builds.
7. **Open — licence.** If the project is GPLv3, calibre's Kobo code can be ported directly. If not, calibre is used only as a reference for facts (schema, SQL behaviour) and the code is written independently.
