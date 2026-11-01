# Kobo MVP — Implementation Plan

Status: **M0–M13 implemented** (decisions in §6; phase 1.5 in §7). Device checks for M3–M13 and spike S2 outstanding; see PROGRESS.md. Source brief: `ereader-companion-project.md`.

## 1. Research findings that shape the design

| Area | Finding | Consequence |
|---|---|---|
| EPUB → KEPUB | `kepubify` (Go, MIT, pgaskin) is the de-facto converter, used by calibre plugins and Kobo users. Latest release v4.0.4 (2022) — stable, effectively feature-complete. No Rust port exists. | Ship kepubify as a **Tauri sidecar binary** behind a `Converter` trait. A native Rust port stays possible later without touching callers. |
| Device storage | Kobo mounts as USB mass storage (`/Volumes/KOBOeReader` on macOS) with a hidden `.kobo/` dir: `KoboReader.sqlite` (library DB), `version` (serial, firmware, model). | Detection = scan mounted volumes for `.kobo/version`. No MTP needed for Kobo. |
| Import timing | Nickel only adds sideloaded files to `content` **after USB eject** (library rescan). calibre therefore can only touch DB rows of books imported on a *previous* connection. | Anything DB-side for a freshly sent book (collections, series) must be **deferred** — except collection rows, which can be written before import (S1). |
| Pending metadata via triggers | kepubify's `seriesmeta` installs a side table plus `AFTER INSERT/UPDATE ON content` triggers in `KoboReader.sqlite`, so series metadata lands the moment Nickel imports the book. Proven in the wild. | Same pattern can apply pending **collection membership** (`ShelfContent`) and series at import time, giving "send + collect in one step". Must be validated on hardware (Spike S1). |
| Prior art: calibre | calibre's KoboTouch driver (GPLv3, ~5k lines Python) and the Kobo Utilities plugin encode years of firmware knowledge: shelf SQL (`Id = InternalName = Name`, `Type='UserTag'`), delete cleanup across `Bookmark/ShelfContent/content/content_settings/ratings/Activity`, cover upload to `.kobo-images`, supported DbVersion up to 222 / fw up to 6.0. | Treat calibre as the **reference specification** for every device write: read the corresponding calibre function first, mirror its SQL and edge cases, cite it in comments. Port the logic directly (GPLv3, §6.7) and cite the calibre source. |
| Collections | `Shelf` (name, flags `_IsDeleted`, `_IsSynced`, `_IsVisible`) and `ShelfContent` (ShelfName, ContentId = `file:///mnt/onboard/...`). | Native collection management is direct, transactional DB writes. |
| Kobo cloud sync | Shelf names sync to the Kobo account; sideloaded books inside them do not. Deleted shelves have been reported to reappear from the account. | Soft-delete via `_IsDeleted=true` (as Kobo Utilities does) rather than hard `DELETE`; surface a note for sync-enabled users. |
| Series | Stock firmware does not reliably read `calibre:series` from sideloaded EPUBs; tools write `content.Series/SeriesNumber/SeriesID` directly. | Write series both into the OPF (portable) and into the DB (what the device displays). |
| Reading data | `content` holds `ReadStatus`, `___PercentRead`, `DateLastRead`, `TimeSpentReading`; `Event` table holds more. | Read-only reading status is cheap to show in the MVP; analytics deferred. |
| macOS on FAT32 | Finder/macOS writes `._*` AppleDouble and `.Spotlight-V100` files; Kobo imports `._book.kepub.epub` as broken books. | Copy without xattrs, clean `._*` files we created, drop `.metadata_never_index` on the volume. |
| Kindle (phase 2) | 2024+ Kindles use **MTP**, not mass storage; sideloading needs AZW3/MOBI or Amazon's Send-to-Kindle EPUB pipeline. | Keep a `Device` trait with capability flags now; don't assume a filesystem path for every device. |
| Toolchain | Rust 1.99 stable is installed via Homebrew rustup (keg-only, so `/opt/homebrew/opt/rustup/bin` must be on PATH). Node 26 + npm present; Xcode CLT present. | Use **pnpm** and **`just`** (both installed). kepubify fetched as a prebuilt release binary (no Go needed). |

## 2. Architecture

```text
calibre-rewrite/
├─ crates/           (each crate is added by the milestone that first needs it)
│  ├─ epub/          OPF read/edit, cover swap, safe re-zip (zip + quick-xml). No Tauri deps.
│  ├─ convert/       Converter trait; KepubifySidecar impl (spawns binary, captures errors).
│  ├─ library/       Local library: SQLite (rusqlite, bundled) + managed book folder, import, dedupe, metadata, covers.
│  ├─ covers/        Online cover search (Open Library, Google Books, Apple Books search) + image checks (image crate).
│  ├─ device/        Device trait, Capabilities {collections, series, reading_stats, ...}, detection registry.
│  ├─ device-kobo/   Detect, version parse, DB read, safe-write layer, transfer, shelves, pre-import collection rows, replug reconcile.
│  └─ cli/           `erc` dev CLI over all crates — the primary E2E test harness for agents.
├─ app/
│  ├─ src-tauri/     Thin command/event layer over the crates (Tauri 2).
│  └─ src/           React + TS + Vite UI; TanStack Query for command data, virtualized cover grid.
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
- **Deferred device ops.** Collections/series for not-yet-imported books are stored as pending ops, collection rows written pre-import (S1-A); series and anything else in `content` reconciled on next connect.

Main dependencies: `tauri 2`, `rusqlite` (bundled), `zip`, `quick-xml`, `image`, `reqwest`, `serde`, `thiserror`, `tracing`, `tempfile`, `sha2`; UI: `react`, `@tanstack/react-query`, `@tanstack/react-virtual`, a small headless component lib (Radix). Package manager: pnpm. Task runner: `just`.

## 3. Milestones

Each milestone is sized for ~1 session, ends with green `just check` (fmt, clippy -D warnings, cargo test, tsc, eslint, vitest), a commit, a push, and a `PROGRESS.md` update. 🔌 = needs a real-device check by the user.

| # | Milestone | Done when |
|---|---|---|
| M0 | **Scaffold** — toolchain, Cargo workspace, Tauri 2 + React/Vite shell, `justfile`, GitHub Actions (macOS), fixture EPUBs, kepubify sidecar fetch script with checksum pin. | `just check` green locally and in CI; empty app window launches. |
| M1 | **Kobo read-only** — volume detection, `version` parse, open a *copy* of the DB read-only, list books (title, author, status, %, last read), list shelves. `erc kobo ls`. Synthetic DB fixtures + an anonymized real DB fixture if available. | Tests pass on fixtures. 🔌 `erc kobo ls` on your device matches what the Kobo shows. |
| M2 | **Thin end-to-end (earliest working workflow)** — `erc send book.epub`: EPUB → kepubify → copy to `/Volumes/KOBOeReader/<dir>` with xattr hygiene → eject. Minimal UI: drop zone, connected-device badge, Send button, progress/error toasts. | 🔌 Book appears and opens on the Kobo with kepub features (reading stats, page counts). |
| M3 | **Local library + metadata** — library DB + managed folder, multi-file drag-and-drop import, dedupe (content hash, then identifier/title+author), edit title/author/series/tags, OPF rewrite on export, round-trip tests across EPUB2/EPUB3 fixtures. | Edited metadata survives export → kepubify → re-read. |
| M4 | **Covers** — replace from local image; online search (Open Library, Google Books, Apple Books) with resolution shown; cover written into OPF manifest correctly for EPUB2/3; thumbnails cached for the grid. | Fixture tests for cover swap; 🔌 new cover visible on device after send. |
| M5 | **Safe-write layer + device management** — backup/integrity/transaction/journal, schema version gate, remove book from device (file + `content`/`ShelfContent` cleanup as calibre does), duplicate-on-device detection, disconnect-mid-transfer handling. | Fault-injection tests (kill mid-write, yank volume simulation) leave DB valid; 🔌 delete on device is clean. |
| M6 | **Native collections** — create/rename/soft-delete shelves, add/remove books (bulk), series → DB, collection rows written at send time, series via replug reconcile. UI: collection sidebar, multi-select bulk assign. Includes one-click "device folders → collections" (your 93 books sit in genre folders the Kobo ignores). | 🔌 Send 3 books into a new collection in one action → after eject they appear in that collection on device. |
| M7 | **UI polish pass** — cover grid view, instant search/filter, library vs device views, reading status badges, clear local-only vs on-device labelling, empty/error states, startup time budget. | Manual UX review checklist + vitest for state logic. |
| M8 | **Auto-grouping rules (MVP stretch)** — "series → collection", "tag → collection" rules, previewed before applying. | Rule engine unit tests; 🔌 apply rules to a real device. |

### Spikes (hardware experiments that de-risk design choices)

| Spike | Question | Status |
|---|---|---|
| **S1** | Can a just-sent book land in a collection without a replug? Tests (A) row written pre-import, (B) import trigger, (C) replug + series baseline. | Script `scripts/spike-s1.sh`, procedure in `docs/hardware-checks.md`. **Done 2026-10-30.** A: in collection immediately. B: only after a Kobo restart. C: collection + series shown immediately after replug. Cleanup clean. |
| S2 | Do collections survive Kobo cloud sync with Wi-Fi on, and does soft-delete propagate? | Deferred to M6; needs a throwaway collection on a synced account. |

**S1 outcome → design:**

- **Collections for new books: write the `Shelf`/`ShelfContent` rows in the same session as the file copy (approach A).** The ContentId is predictable (`file:///mnt/onboard/<path>`), Nickel honours the row on import, and no triggers are installed. "Send to collection" is one step.
- **Series (and other `content` columns) for new books: replug reconcile (approach C)**, since the `content` row doesn't exist until import. The app shows "N books have pending details — replug to finish". A trigger is not used: it needs a Kobo restart to show, which is worse UX than a replug.
- Triggers dropped from the plan; nothing persistent is installed in Nickel's DB.

## 4. Significant risks

1. **Kobo DB schema/firmware drift** — undocumented schema; Nickel may change it. Mitigate: version gate, read-only fallback, column-existence checks, fixtures per firmware.
2. **Device DB corruption** — highest-impact failure. Mitigate: safe-write layer (M5) lands *before* any shelf writes (M6); backups always; never write while device isn't in USB mode.
3. **Pre-import collection rows** rely on Nickel matching ContentId by path at import (verified on fw 4.38). If a future firmware stops doing so, the replug reconcile re-applies them. ContentId must exactly match the on-device path, including case.
4. **Kobo cloud sync** interfering with collections. Mitigate: soft-deletes, documentation, spike S2.
5. **kepubify sidecar** — unmaintained-but-stable Go binary; needs code signing/notarization inside the macOS bundle; universal binary (arm64 + x64). Mitigate: checksum-pinned download, Converter trait for a future Rust port.
6. **macOS FAT32 artefacts** (`._*` files). fw 4.38 does not import them, but they clutter the volume and older firmware did import them. Never create them (M2) and offer a tidy-up.
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
3. Pending collections: pre-import rows (S1 approach A); series via replug reconcile. No triggers.
4. Converter: kepubify sidecar for the MVP.
5. Online covers: Open Library, Google Books and Apple Books search APIs approved.
6. Distribution: unsigned personal builds.
7. Licence: **GPLv3** (`LICENSE`). calibre's Kobo code (KoboTouch driver, Kobo Utilities) may be ported directly; ported functions cite their calibre source in a comment.

## 7. Phase 1.5 — Kobo depth (before Kindle)

Goal: make ERC the only tool a Kobo owner with an existing, messy device needs. Guiding rule (user, 2026-10-31): **no fragile features.** Anything that scrapes a website, depends on an undocumented cloud service, or writes Kobo settings/firmware-specific files is skipped or kept optional.

### 7.1 Plugin review (port / partial / skip)

| Plugin | Verdict | What we take | What we leave and why |
|---|---|---|---|
| Kobo Utilities (janlarres) | **Partial — biggest source** | Set reading status / reset progress; read reading stats (status, %, last read, time spent) into the library; show device books missing from the DB and DB rows whose file is gone; clean orphaned `.kobo-images` entries; remove cover cache so Nickel regenerates; fix duplicate collections (merge same-name shelves); back up the annotation files. Already done: DB backup, integrity check, series, collections. | Reader font/margin settings (writes `Kobo eReader.conf`, changes between firmware); blocking analytics with a trigger (triggers need a restart and firmware updates can break them); set related books; update ToC (rewrites `content` chapter rows, firmware-specific); upload `.parsed` covers in device-specific sizes (clearing the cache is the robust version); firmware update check (scrapes Kobo). Store/restore reading positions: not needed, because ERC replaces books in place and keeps progress. |
| ePub Extended Metadata (un-pogaz) | **Port** | Contributors with MARC roles (translator, illustrator, editor, narrator…) read/written in the OPF for EPUB2 (`opf:role`) and EPUB3 (`refines` + `role`); shown and edited in the edit panel. Fits `erc-epub`'s byte-preserving splice. | Nothing important. |
| Goodreads (kiwidude68) | **Skip** | Nothing. | The public API has been closed since 2020, so the plugin depends on scraping and on login cookies. Replaced by the layered metadata sources in §7.4 (Hardcover is the closest Goodreads equivalent and is optional there). |
| Quality Check (kiwidude68) | **Partial** | A local "Library health" view: missing cover, low-resolution cover, missing author/series index, series gaps and duplicates, duplicate ISBNs, DRM present, EPUB problems (missing `container.xml`, files outside the manifest, manifest items missing, broken internal links), OS junk inside the zip (`__MACOSX`, `.DS_Store`, `Thumbs.db`), embedded fonts. One safe fix: strip the OS junk. | Checks tied to calibre's own columns and formats (MOBI/AZW, calibre custom columns); automatic EPUB repairs beyond the junk strip (rewriting structure risks breaking a valid book). |
| Kobo-Metadata (NotSimone) | **Skip** | Nothing. | Scrapes the Kobo store through `cloudscraper` and gets locked out by Cloudflare regularly; exactly the "breaks suddenly" class. |
| Kobo annotations (Sergius, MobileRead) | **Partial** | Read highlights and notes from the `Bookmark` table (stable for years and also used by calibre's annotation import) into the library; export per book as Markdown and JSON; keep date-read and % in the library. | Generating annotated EPUB copies (rewrites book content and positions; the plugin is new and only tested on one device). |

### 7.2 New features requested

1. **Overscroll lock** — done: `html, body, #root` never scroll; list panels use `overscroll-behavior: contain`.
2. **List and cover views on both screens** — one toggle per screen, remembered. The grid and list share selection, search and sort.
3. **Tri-state column sort** — clicking a header sorts descending (e.g. status → most read first), a second click sorts ascending, a third click returns to the default order. Every column sortable; the grid uses the same sort state through a menu.
4. **ERC state on the device** — inspired by calibre's `driveinfo.calibre` (device UUID, library UUID, calibre version, last connected) and `metadata.calibre` (its view of every book). ERC writes `/.erc/` at the volume root (Nickel ignores dot folders; `._` companions removed as usual):
   - `device.json`: device UUID ERC assigned, library UUID, ERC version and firmware/DbVersion at first and last write, last connected time.
   - `books.json`: every book ERC sent or modified: content ID, library book ID, source SHA-256, device-file SHA-256 and size at write time, ERC version, operations applied. Lets ERC tell "ours and untouched" from "changed by something else", and recognise its books from another computer.
   - `log.jsonl`: append-only operation log (time, ERC version, operation, content IDs, backup file name, result), capped in size. Used for debugging and shown in an "About this Kobo" panel and `erc kobo history`. Written inside the safe-write session; a missing or corrupt file never blocks anything (ERC rebuilds from the DB).
5. **Import from the Kobo into the library** — sideloaded books only (store books are DRM-protected and excluded). KEPUBs are cleaned back to plain EPUB by removing `koboSpan` markup and Kobo's injected scripts and styles (port of calibre's KEPUB input cleanup), keeping a kepub-as-is fallback when cleaning fails. Carries over what Nickel knows (series, reading state) and links the device copy so the book shows as "On Kobo" without being re-sent.
6. **Enhance and organise** — a wizard for devices with books in folders or one unorganised folder: (1) import from the device, (2) propose metadata from the layered sources in §7.4 (local first, then online; subjects and categories mapped to a short fixed genre list), (3) review every proposed change as a diff, accepting per book or per field, (4) apply to the library, push updated books back with in-place replace (progress kept), (5) create collections from folders, series or genre through the existing rules preview. Nothing is written to the device before the final preview.
7. **Duplicate review** — groups by identical file, identical ISBN, same normalised title+author, and same book in several formats (EPUB + KEPUB, store copy + sideloaded copy). Reviewed one group at a time with covers, sizes, formats and reading progress side by side; pick what to keep; removals use the existing safe paths (library remove, device remove with backup). A book with reading progress is never preselected for removal.

### 7.3 Milestones

| # | Milestone | Done when |
|---|---|---|
| M9 | **Views and sorting** — list/grid toggle on Library and On Kobo, shared tri-state header sort (pure, unit-tested sort state), overscroll lock. | vitest for sort state; UI checked in the mock. |
| M10 | **ERC device state** — `/.erc/` files written in every write session; atomic writes; size-capped log; "About this Kobo" panel; `erc kobo history`. | Tests on fixture + FAT32 image; corrupt/missing files tolerated. 🔌 Files appear after a send; Nickel shows no ghost books. |
| M11 | **Import from Kobo** — sideloaded books → library with KEPUB cleanup, Nickel series/status, linked device copies; DRM/store books listed as skipped. | Round trip: clean → kepubify → identical text; all 94 real-device books import or skip with a reason. |
| M12 | **Duplicate review** — grouping engine (library + device), one-group-at-a-time review UI. | Engine unit tests; real-DB dry run lists groups. |
| M13 | **Enhance and organise wizard** — metadata proposals from the §7.4 sources (`erc-metadata` crate, also used by a "Fill metadata" action on any library book), diff review, apply, replace on device, collections via rules. | Proposal engine tests with recorded responses from each source (no live network in tests); 🔌 a folder-organised device ends with series + genre collections and progress intact. |
| M14 | **Kobo Utilities ports** — set reading status/reset progress, reading stats into the library, ghost/missing book report, orphaned image cleanup, duplicate collection merge. | Fixture + real-DB-copy tests through `DeviceWriter`. |
| M15 | **Library health + extended metadata** — Quality Check subset with the OS-junk fix; contributors with roles in `erc-epub` and the edit panel. | Fixture EPUBs for each check; OPF round-trip tests for EPUB2/3 roles. |
| M16 | **Annotations** — highlights/notes from `Bookmark` into the library, Markdown/JSON export, annotation-file backup. | Tests on the real DB copy; 🔌 export matches what the Kobo shows. |

### 7.4 Automatic metadata (replaces Goodreads)

No single free source has everything, so ERC layers them. Each source fills only fields still empty or weaker; every result is a proposal shown as a diff, never applied silently. Each source is independent: a failing or changed source is skipped with a note and the others still work.

| Order | Source | Good for | Access | Fragility |
|---|---|---|---|---|
| 1 | Local: EPUB OPF (incl. calibre/EPUB3 series), Kobo DB series, folder names | Title, author, series, ISBN, publisher, description, genre hint | Offline | None |
| 2 | Open Library | ISBN/edition match, authors, publish date, subjects → genres; sparse series | Free, no key | Low (stable public API) |
| 3 | Google Books | Descriptions, categories, ISBNs, page count | Free with a personal API key (1,000/day; setting); keyless requests often 429 | Low |
| 4 | Apple Books search (iTunes Search API) | Description, primary genre, high-res cover | Free, no key | Low |
| 5 | Wikidata (SPARQL) | Series and reading order (P179 + P1545) for well-known books | Free, no key | Low |
| 6 | Hardcover (GraphQL) | Best series data; closest Goodreads equivalent | **Optional**, user's own API token (setting); off without one | Medium: API officially beta |

Matching: ISBN first, then normalised title + author with a similarity threshold; low-confidence matches are shown but unticked. Responses are cached in the library DB so re-runs are cheap and work offline. Skipped entirely: Amazon, Goodreads and the Kobo store (scraping only).

Order: M9 is pure UI. M10 comes before every new device write (M11–M14 log through it). Phase 2 (Kindle) follows M16.
