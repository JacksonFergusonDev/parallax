# Bindery vs Parallax: code comparison

Bindery was cloned at commit `730dd98` (v4.3.2, 2026-09-09) into `/private/tmp/claude-501/-Users-jacksonferguson-Developer-parallax/34c77541-5d73-4dd5-8ad8-408d704eda57/scratchpad/research-src/bindery-b`. Nothing from it was run. Bindery links below are `https://github.com/jarynclouatre/bindery/blob/730dd98/<file>#L..`, and Parallax paths are relative to `/Users/jacksonferguson/Developer/parallax`.

## What bindery is, and which areas overlap at all

### Takeaway

Bindery is a small Dockerised Python/Flask service (about 2,100 lines of app code and 2,200 lines of tests) that watches drop folders and runs two external tools: kepubify for EPUB→KEPUB, and KCC (`kcc-c2e`, pinned v11.0.1) for CBZ/CBR/ZIP/RAR/PDF→EPUB/CBZ. It has no library, metadata parsing, online lookup, series handling, duplicate detection, cover work, Kobo device detection, KoboReader.sqlite access, collections, thumbnails, device backups or progress sync. Most of the requested areas therefore have no bindery counterpart. The useful material is in conversion orchestration, comic-folder bundling and a few robustness heuristics.

### Cited Findings

- Code size: `processor.py` 1,366 lines, `app.py` 420, `config.py` 143, `raw_processor.py` 213, `templates/index.html` 1,328; tests: `test_processor.py` 1,124, `test_app.py` 365, `test_status_browser.py` 383, `test_raw_processor.py` 164, `test_config.py` 163 (about 160 test functions) — [bindery tree](https://github.com/jarynclouatre/bindery/tree/730dd98)
- It describes itself as "Self-hosted Docker e-book and comic converter", requires Python ≥3.13, and depends on Flask, gunicorn, watchdog, apprise and KCC's runtime libraries (Pillow, natsort, numpy, PyMuPDF, mozjpeg-lossless-optimization) — [pyproject.toml](https://github.com/jarynclouatre/bindery/blob/730dd98/pyproject.toml), [requirements.txt](https://github.com/jarynclouatre/bindery/blob/730dd98/requirements.txt)
- The Dockerfile builds kepubify from source at revision `8e959eda…` (v4.0.4) and installs KCC with `pip install --no-deps git+https://github.com/ciromattia/kcc.git@v11.0.1`. It also installs `p7zip-full`, `p7zip-rar` and `unrar`, enabling Debian's `non-free` components to do so — [Dockerfile L1-L68](https://github.com/jarynclouatre/bindery/blob/730dd98/Dockerfile)
- Folder layout: `Books_in` → `Books_out`, `Comics_in` → `Comics_out`, `Comics_raw` (loose image folders are zipped to CBZ), and `config/` holding `settings.json`, `jobs.json` and `converted.json` — [README "Folder Layout"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- The output goes to a folder, not to a device. The README suggests Calibre-Web-Automated, Calibre auto-add or rclone for delivery — [README "Use Cases"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- Parallax, by contrast, has a Rust workspace (about 23,100 lines in `crates/`) covering a library, metadata, covers, a Kobo database writer, backups, collections, stats and conversion — `crates/*/src` (local, `wc -l`)

### Inferences

- Bindery is a conversion front end, not a library manager or device manager, so for most of Parallax's areas the honest answer is "bindery doesn't cover it". It is closer to Parallax's `spikes/kobo-comics` and `crates/convert` than to anything else.

### Gaps

- None for this question.

## Area-by-area comparison

### Takeaway

Bindery and Parallax overlap only in KEPUB conversion, comics conversion, filename and page ordering, and file safety. In each of these bindery mostly hands the work to a tool (kepubify, KCC or 7z) and adds orchestration and edge-case handling around it. Parallax is far ahead everywhere device or library state is involved.

### Cited Findings

#### Library import and storage

- Bindery: none. Files pass through. Job history is a JSON file capped at 500 entries (`MAX_JOBS`), and the "converted ledger" records sources it has already converted — [processor.py L53-L66](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L53-L66)
- Parallax: a SQLite library with per-book folders. `Library::import_from` hashes the original with SHA-256 (`hash_file`), reads the EPUB's OPF and inserts the title, authors, series, index, tags and identifiers. Only EPUB, plus Amazon formats converted to EPUB first, can be imported; CBZ cannot — `crates/library/src/lib.rs` L270-L330, L824 (`hash_file`); `crates/convert/src/lib.rs` L1-L5

#### Duplicate detection

- Bindery: none. Its only "already seen" logic is the keep-in-place ledger. Its signature is `size:mtime` for a file, or `count:total_size:newest_mtime` for a folder, and a change that only touches the mtime is deliberately treated as unchanged — [processor.py L141-L163, L200-L226](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L141-L226)
- Parallax: content hashes (SHA-256) plus ISBN, alphanumeric title + author keys and per-device groups — `crates/core/src/duplicates.rs` L25-L341, `crates/library/src/lib.rs` L727 (`similarity_key`)

#### Metadata parsing and online lookup

- Bindery: none of its own. Titles come from the file stem (`--title=<stem>`). Turning on "Use ComicInfo.xml" just passes `--metadatatitle 1`, so KCC reads ComicInfo; bindery never parses the XML itself — [processor.py L765-L772](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L712-L783); [CHANGELOG v4.2.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)
- Parallax: OPF parsing (`crates/epub/src/opf.rs`, 555 lines), metadata fetching (`crates/metadata/src/fetch.rs`, `lib.rs`), genre canonicalisation (`crates/metadata/src/genres.rs`) and calibre import (`crates/core/src/calibre.rs`). There is no ComicInfo.xml parser anywhere in `crates/`; the only ComicInfo code is a hand-written template in the spike — `spikes/kobo-comics/src/main.rs` L342-L365

#### Series handling

- Bindery: none, beyond what KCC takes from ComicInfo. "Bundle Chapter Folders" turns a folder of chapter archives into one volume, which is a grouping feature but not series metadata — [README "Bindery Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- Parallax: series and series index in the library schema, OPF/calibre series, Kobo series writing (`crates/device-kobo/src/ops.rs`). The spike writes both `calibre:series` and EPUB 3 `belongs-to-collection` for comics — `docs/kobo-comics-spike.md` "The four versions"

#### Filename parsing

- Bindery: no series or volume parsing at all. It strips a leading dash, because KCC passes basenames to 7z, which reads them as switches — [processor.py L785-L805](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L785-L805). Output is renamed from `.kepub.epub` to `.kepub` or a chosen extension — [processor.py L664-L689](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L664-L689)
- Parallax: the spike's `parse_name` only handles `<Series> vNN (YYYY)` — `spikes/kobo-comics/src/main.rs` L186-L207. Titles of removed books are guessed from file names in `crates/core/src/stats.rs` (test `titles_from_file_names`, L328). `device_file_name` cleans names for FAT32 — `crates/device-kobo/src/transfer.rs` L172-L198

#### Cover extraction and generation

- Bindery: none (KCC makes the cover inside its output).
- Parallax: `crates/covers` (592 lines), Amazon/Indigo sources, Core ML upscaling (`crates/upscale`), Kobo thumbnails (`crates/device-kobo/src/covers.rs`)

#### Format conversion

- Bindery KEPUB: a kepubify command builder that exposes `--calibre` (`.kepub` extension), `--smarten-punctuation`, `--fullscreen-reading-fixes`, `--hyphenate` or `--no-hyphenate`, `--add-dummy-titlepage` or `--no-add-dummy-titlepage`, `--css=`, `--replace=` (only lines that contain `|`, because "kepubify aborts the whole conversion on a malformed rule") and `--charset=`. Values are passed in `--opt=value` form so a value that starts with a dash is never read as an option — [processor.py L808-L849](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L808-L849); [README "Books Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- Parallax KEPUB: `Kepubify::convert` runs only `kepubify --output <out> <in>`. It treats "exit 0 **and** the output file exists" as success, because "kepubify reports per-file errors in its summary and may still exit 0" — `crates/convert/src/lib.rs` L86-L118. The spike adds `--no-add-dummy-titlepage` for comics — `spikes/kobo-comics/src/main.rs` L545-L560
- Bindery comics: a KCC command builder (profile, format EPUB/CBZ, splitter 0/1/2, cropping, cropping power and minimum (UI percent → 0–1 ratio), batchsplit, gamma, manga style, HQ, two-panel, webtoon, black or white borders, force colour, colour autocontrast, rainbow eraser, mozjpeg, stretch, upscale, norotate, rotateright, rotatefirst, nokepub, title and author) — [processor.py L712-L783](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L712-L783); defaults in [config.py L12-L42](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L12-L42) (default profile `KoLC`, stretch on, force colour on, cropping `2`, splitter `1`)
- Parallax comics: only the spike, which does greyscale, right-first spread splitting, Lanczos3 to 1264×1680, 16 levels and 4-bit PNG, and builds the fixed-layout RTL EPUB by hand before kepubify — `spikes/kobo-comics/src/main.rs` L249-L325, L369-L527; `docs/kobo-comics-spike.md`

#### Kobo device detection and writing; collections; thumbnails; backups; progress sync

- Bindery: none. It never touches a device. The Kobo-specific parts are the kepubify settings and a KCC profile picker for Kobo models — [templates/index.html L509-L523](https://github.com/jarynclouatre/bindery/blob/730dd98/templates/index.html#L509-L523)
- Parallax: `crates/device-kobo` (detection via `.kobo/version`, `version.rs`; a write session with backup, WAL checkpoint and integrity check, `writer.rs` L38-L170; zstd backups and held removed files, `backups.rs`; collections, `collections.rs`; annotations; stats; AppleDouble cleanup, `transfer.rs` L143-L170; eject via `diskutil`, L114)

#### Backups and file safety (bindery's analogue: never losing a source)

- Bindery writes to a temp dir, moves outputs with collision-free `_2`/`_3` names under a lock, renames failures to `<name>.failed` (or `_2.failed` and so on) so they aren't retried, and offers three originals modes: delete, archive to `.archive` mirroring the folders, or keep in place with the ledger — [processor.py L629-L689, L864-L973](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L629-L973)
- It saves config and ledger JSON atomically (temp file then `os.replace`) — [config.py L135-L143](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L135-L143); [processor.py L183-L192](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L183-L192)
- It refuses to convert books when `Books_in` and `Books_out` overlap, comparing directory identity and parsing `/proc` mount info for bind mounts, so it can't convert its own output in a loop — [processor.py L388-L477](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L388-L477)
- Parallax: temp file + rename for `/.parallax/` records, database backups before every write, removed files held for 30 days, library snapshots before imports — `docs/device-notes.md` "Parallax records", "Backups"; `crates/library/src/snapshots.rs`; `crates/core/src/import.rs` L58-L60

#### Comics image processing

- Bindery: entirely KCC. The only images it handles itself are in `Comics_raw`: a flat image folder is zipped `ZIP_STORED` into a CBZ, and folders with subfolders or without images go to `unprocessed/` — [raw_processor.py L72-L140](https://github.com/jarynclouatre/bindery/blob/730dd98/raw_processor.py#L72-L140)
- Parallax: its own Rust pipeline in the spike (see above), using `image`, `png`, `rayon` and `zip` — `spikes/kobo-comics/Cargo.toml`

### Inferences

- For a desktop app that writes directly to the Kobo, bindery's drop-folder machinery (stability polling, job registry, watcher modes, Apprise) mostly doesn't apply. The exceptions are folder-readiness checks if Parallax ever adds a watched import folder, and the keep-in-place ledger idea.

### Gaps

- Bindery has nothing to compare with for metadata lookup, covers, Kobo DB, collections, thumbnails, backups or sync, so no comparison was possible there.

## Bindery code worth porting or adapting

### Takeaway

There is little code worth copying line for line, since most of bindery calls other tools. Five small, well-tested ideas are worth reimplementing in Rust: the kepubify option set, natural sort, chapter-archive bundling with wrapper-folder hoisting, the KCC option and profile map as a feature checklist, and atomic writes and loop guards for any future watched folder.

### Cited Findings

1. **More kepubify options** → `crates/convert/src/lib.rs` `Kepubify::convert`. Bindery's builder ([processor.py L808-L849](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L808-L849)) and its tests (`test_build_kepubify_cmd_flag_mappings`, `..._maps_text_options`, `..._omits_null_text_options`, [tests/test_processor.py L198-L250](https://github.com/jarynclouatre/bindery/blob/730dd98/tests/test_processor.py#L198-L250)) cover smarten punctuation, hyphenation, dummy title page, fullscreen fixes, CSS, find/replace and charset. Parallax passes none of these, and the spike shows comics need `--no-add-dummy-titlepage` — `spikes/kobo-comics/src/main.rs` L545-L560
2. **Natural sort** → a shared helper for CBZ page order. Bindery's `_natural_key` splits on `(\d+)` and compares digit runs as integers ([processor.py L1111-L1113](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1111-L1113)), tested by `test_extract_chapter_folder_orders_chapters_naturally` ([tests L779](https://github.com/jarynclouatre/bindery/blob/730dd98/tests/test_processor.py#L779)). Parallax's spike says "the app needs a natural sort" but sorts by lowercase — `spikes/kobo-comics/src/main.rs` L218-L228
3. **Chapter bundling** → future Parallax "folder of chapters → one volume". `_extract_chapter_folder` collects archives recursively while skipping dot-dirs, sorts them naturally by relative path, extracts each into `NNN - <stem>` (zero-padded, since "KCC sorts chapter dirs lexicographically"), and "hoists" single-folder wrappers until the chapter directory holds the images. It cleans up the temp dir on failure — [processor.py L1116-L1162](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1116-L1162). `_is_bundle_folder` decides when a folder counts as one volume: images only → yes; archives only → only when the setting is on; mixed with PDFs or images → no; nothing comic-typed → not a job — [processor.py L1075-L1108](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1075-L1108); table test at [tests L749](https://github.com/jarynclouatre/bindery/blob/730dd98/tests/test_processor.py#L749)
4. **KCC option and profile map** as a settings checklist for "Prepare for Kobo": Kobo profile codes `KoMT, KoG, KoGHD, KoA, KoAHD, KoAH2O, KoAO, KoN, KoF, KoS, KoC (Clara HD/2E), KoCC (Clara Colour), KoL (Libra H2O/2), KoLC (Libra Colour), KoE (Elipsa)` — [templates/index.html L509-L523](https://github.com/jarynclouatre/bindery/blob/730dd98/templates/index.html#L509-L523). The changelog records profile-key corrections (`KoCE`→`KoCC`, `KoM`/`KoT`→`KoMT`, `KoE2` removed because "no KCC profile exists for it") — [CHANGELOG](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md). Spread options in KCC terms: splitter 0 = split, 1 = rotate, 2 = both — [README "KCC Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md), [config.py L17-L36](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L17-L36). Parallax maps Kobo model ids to names (`crates/device-kobo/src/version.rs` L40-L60) but has no screen sizes; the spike hard-codes 1264×1680
5. **Flaky-input guards**, if Parallax adds a watched import folder: `wait_for_file_ready` needs 3 identical non-zero sizes 2 s apart, because "copy tools like FileBrowser pause briefly between write chunks" — [processor.py L514-L545](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L514-L545). Folders must be quiet for the timeout, at least 30 s — [processor.py L570-L604](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L570-L604). Hidden files to ignore: `.ds_store`, `thumbs.db`, `desktop.ini`, `.localized`, plus dot-dirs (Syncthing, `.uploading`) — [raw_processor.py L18](https://github.com/jarynclouatre/bindery/blob/730dd98/raw_processor.py#L18), [processor.py L141-L163](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L141-L163)
6. **Settings migration pattern**: `_migrate_kcc_options` rewrites obsolete keys in place on load, and unsaved keys fall back to the defaults — [config.py L60-L111](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L60-L111)

### Inferences

- All of these are small: about 15–60 lines of Python each, which would be roughly 20–100 lines of Rust. Natural sort can come from an existing crate (for example `natord` or `alphanumeric-sort`) rather than a port, in line with AGENTS.md's "use existing libraries". Its key behaviour, and its tests, are still worth copying as cases.
- Parallax does in-process Rust image work rather than calling KCC, so it would reimplement the KCC options it wants (cropping, gamma, autocontrast, rotate-or-split) rather than port bindery code. Bindery only builds KCC's command line.

### Gaps

- I did not verify how KCC itself implements these options (that is KCC's code, not bindery's); a separate KCC study would be needed.

## Where bindery handles something better or more completely

### Takeaway

Bindery is more complete only in conversion options and comic input handling: CBR/RAR/PDF input, chapter folders, raw image folders, KCC's spread, crop and colour options, profile-per-device settings, and kepubify options. Parallax's comic work is still a spike that reads only zero-padded JPEG/PNG CBZ files.

### Cited Findings

- Bindery accepts `.cbz .cbr .zip .rar .pdf`; RAR goes through 7z/unrar in the image — [processor.py L24-L30](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L24-L30), [Dockerfile](https://github.com/jarynclouatre/bindery/blob/730dd98/Dockerfile). The Parallax spike reads only `zip` CBZ files with `jpg/jpeg/png` pages and skips `__MACOSX` — `spikes/kobo-comics/src/main.rs` L209-L228
- Image extensions bindery recognises: `.jpg .jpeg .png .gif .webp` — [processor.py L31](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L31). The Parallax spike has no GIF or WebP
- Spread handling: bindery exposes split, rotate or both (and rotate-right, rotate-first) — [processor.py L742-L762](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L742-L762). Parallax's spike only splits, and its device test concluded "Spreads need a choice … This should be a setting" — `docs/kobo-comics-spike.md` "Device results"
- Colour devices: bindery turns on `--forcecolor`, `--colorautocontrast` and the optional `--eraserainbow` (for colour e-ink interference) — [config.py L19-L21](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L19-L21), [CHANGELOG](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md). Parallax lists "Colour Kobos" as not tested — `docs/kobo-comics-spike.md` "Not tested here"
- Margin cropping and gamma: present in bindery through KCC (`--cropping`, `--croppingpower`, `--croppingminimum`, `--gamma`); listed as not tested in Parallax — `docs/kobo-comics-spike.md` "Not tested here"
- Webtoon mode (`--webtoon`) and two-panel mode exist in bindery — [processor.py L752-L753](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L752-L753); Parallax has neither
- Edge-case knowledge that comes with tests: dash-leading file names break 7z (`test_strip_leading_dash_renames_and_updates_job`), formats it can't produce fall back to EPUB (`test_build_kcc_cmd_legacy_mobi_falls_back_to_epub`), and a library manager touching files must not cause re-conversion (`test_converted_ledger_ignores_timestamp_only_touch`) — [tests/test_processor.py L153, L982, L1016](https://github.com/jarynclouatre/bindery/blob/730dd98/tests/test_processor.py)
- Its changelog gives KCC's reason for one limitation: KCC doesn't accept EPUB as input, so graphic-novel EPUBs only get kepubify — [CHANGELOG](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)

### Inferences

- The gaps in the spike's "Not tested here" list (colour, cropping, gamma, rotating spreads) are exactly the options bindery exposes through KCC. Bindery's defaults (`KoLC`, stretch, force colour, autocontrast, cropping 2) are one maintainer's choices, not measured results, so they are a starting point for Parallax's device tests, not evidence.

### Gaps

- Bindery has no device-side data (page-turn timing, RTL behaviour or ComicInfo support on Nickel), so it cannot answer the spike's open checklist items.

## Where Parallax is already stronger

### Takeaway

Parallax is stronger in everything stateful and in identity. Bindery has nothing comparable to its library, SHA-256 identity, Kobo database writing with backups and verification, or collections, stats, covers and metadata. Some of bindery's own heuristics are weaker than what Parallax already does.

### Cited Findings

- Change detection: bindery's ledger treats a source as unchanged when only its mtime moved and the size is the same, so a same-size edit (such as a ComicInfo rewrite of identical length) would be missed — [processor.py L200-L226](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L200-L226). Parallax identifies files by SHA-256 — `crates/library/src/lib.rs` L824; `docs/device-notes.md` (`books.json` stores `sha256` and `size`)
- Bindery's raw-folder zipper sorts pages by **lowercase name, not naturally** (`sorted(..., key=lambda x: os.path.basename(x).lower())`), so unpadded `2.jpg`/`10.jpg` would be stored out of order. Only chapter archives use `_natural_key` — [raw_processor.py L118-L120](https://github.com/jarynclouatre/bindery/blob/730dd98/raw_processor.py#L72-L140) vs [processor.py L1111](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1111). Parallax's spike has the same limitation but documents it — `spikes/kobo-comics/src/main.rs` L218-L219
- Archive extraction: bindery shells out to `7z x -y -o<dir> <archive>` with no path checks of its own — [processor.py L1138-L1142](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1116-L1162). Parallax reads CBZ in-process through the `zip` crate — `spikes/kobo-comics/src/main.rs` L220-L240
- Lossless KEPUB packaging, with SHA-256 verification that the images are byte-identical and that kepubify kept the RTL and fixed-layout settings, exists only in Parallax — `spikes/kobo-comics/src/main.rs` L563-L590; `docs/kobo-comics-spike.md` "Desktop results"
- Failure detection: both check for real output. Parallax also gathers error lines from kepubify's stdout and stderr into the error message — `crates/convert/src/lib.rs` L86-L118; bindery's `process_file` "no output produced" — [processor.py L951-L956](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L864-L973)
- Device safety: write sessions with database backups, WAL checkpoint, integrity check and restore, and AppleDouble cleanup — `crates/device-kobo/src/writer.rs` L38-L200, `crates/device-kobo/src/transfer.rs` L143-L170, `docs/device-notes.md`. Bindery has nothing comparable
- Security: bindery's README says the WebUI "has no authentication" — [README "Behaviour"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md). This is irrelevant to Parallax, which is a local desktop app

### Inferences

- The `7z` point is an inference from reading the code: 7-Zip's own path-traversal handling wasn't checked here. Parallax should guard zip-slip itself whatever it ports, by rejecting `..` and absolute entry names when it extracts or repackages CBZ files.

### Gaps

- I didn't benchmark either tool; bindery has no published performance numbers.

## Comics and manga specifics (ComicInfo, filename series/volume, CBR, natural sort, RTL)

### Takeaway

Bindery does almost none of this itself. ComicInfo is read by KCC behind a flag and never written by bindery; RTL is KCC's `--manga-style`; CBR goes through 7z/unrar; there is no filename series/volume parser; natural sort covers chapter archives only. Parallax will need its own Rust implementations of all five. Bindery only adds folder-bundling rules and a few test cases.

### Cited Findings

- ComicInfo read: delegated, `--metadatatitle 1` when `kcc_comicinfo` is on. The changelog example says a file named `Chapter 1 (2).cbz` "still arrives on your reader as *Berserk #001: The Black Swordsman*" — [processor.py L765-L770](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L712-L783), [CHANGELOG v4.2.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md). ComicInfo write: none in bindery. Parallax spike: writes Title, Series, Number, Volume, Year, Writer, PageCount, LanguageISO, `Manga=YesAndRightToLeft` — `spikes/kobo-comics/src/main.rs` L342-L365
- RTL: bindery's `kcc_manga_style` → `--manga-style`, off by default — [config.py L13](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L13), [processor.py L750](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L750). Parallax spike: `page-progression-direction="rtl"` in the spine, and the right half of a spread first — `spikes/kobo-comics/src/main.rs` L258, L485
- CBR: bindery needs `unrar` or `p7zip-rar` from Debian `non-free` — [Dockerfile](https://github.com/jarynclouatre/bindery/blob/730dd98/Dockerfile). Parallax has no RAR support
- Filename series/volume: bindery none; Parallax spike `<Series> vNN (YYYY)` only — `spikes/kobo-comics/src/main.rs` L186-L207
- Natural sort: bindery chapters only (see above); Parallax none (spike lowercase sort)

### Inferences

- For CBR on macOS under GPL-3.0-only, bindery's route (bundling unrar) carries a licence question: the UnRAR licence restricts its use, which is why Debian ships it in `non-free`. A pure-Rust or libarchive-based reader, or converting CBR→CBZ on import, deserves a separate check. That is an inference from bindery's use of `non-free`; the UnRAR licence terms weren't fetched here.
- Bindery's `_is_bundle_folder` and `_extract_chapter_folder` give a tested definition of "a folder is one volume". It suits manga scanlation downloads, which come as one CBZ per chapter.

### Gaps

- No filename-parsing corpus or test fixtures for comics in bindery; Parallax would need another source (for example Komga, Kavita or ComicTagger) for series/volume/chapter regexes.

## Licence compatibility and porting effort

### Takeaway

Bindery is MIT, which is compatible with GPL-3.0-only. Parallax can copy or adapt it if it keeps the MIT copyright and permission notice. It is Python, so "porting" means rewriting in Rust; the useful pieces are small, about a day of work in total plus tests.

### Cited Findings

- Bindery is "MIT License, Copyright (c) 2026 jarynclouatre" — [LICENSE](https://github.com/jarynclouatre/bindery/blob/730dd98/LICENSE)
- Parallax is `license = "GPL-3.0-only"`, and its LICENSE file is the GNU GPL — `Cargo.toml` L10, `LICENSE`. PROGRESS.md already follows the practice of citing ported sources in a comment: "port calibre's Kobo logic … cite it in a comment" — `PROGRESS.md` "Notes for agents"
- Bindery's tools have their own licences: kepubify (already bundled by Parallax, pinned v4.0.4 by SHA-256 in `scripts/fetch_kepubify.py`) and KCC. Bindery doesn't vendor either; it installs them at image build time — [Dockerfile](https://github.com/jarynclouatre/bindery/blob/730dd98/Dockerfile)

### Inferences

- MIT → GPLv3 is a standard one-way compatible combination: keep the MIT notice with any substantially copied part (for example in a header comment or a THIRD-PARTY notice). Since everything would be rewritten in Rust rather than copied, the obligation mostly applies to closely translated logic and test cases.
- Effort estimate (inference): kepubify flags plus tests about 2–3 h; natural sort through a crate plus bindery's cases about 1 h; chapter bundling with wrapper hoisting and dot-dir skipping, done in-process with `zip`, about 3–4 h; folder-type classification about 1 h. KCC's image options are not a port from bindery; they would be new work in Parallax's image pipeline, or a port from KCC, whose licence and code need checking separately.

### Gaps

- KCC's licence wasn't checked here. It matters only if Parallax ports KCC's own image code, not bindery's.

## Tests, fixtures and device knowledge Parallax lacks

### Takeaway

Bindery's tests are unit tests of command building and file shuffling, using `tmp_path` and mocks, with no binary fixtures. Its device knowledge is limited to KCC profile names and kepubify's extension behaviour. There is nothing about KoboReader.sqlite, firmware or Nickel. Parallax's device knowledge (`docs/device-notes.md`, hardware-verified on a Libra 2) is far deeper.

### Cited Findings

- Test areas: KCC and kepubify command mapping, output renaming, ledger semantics, bundle-folder decisions, natural chapter order, wrapper hoisting, failure cleanup, file-readiness polling, folder overlap and bind mounts, profiles, and notification gating — [tests/test_processor.py](https://github.com/jarynclouatre/bindery/blob/730dd98/tests/test_processor.py) (test names listed L9-L1099), [tests/test_raw_processor.py](https://github.com/jarynclouatre/bindery/blob/730dd98/tests/test_raw_processor.py)
- Kobo file-extension knowledge: "`.kepub.epub` is what a Kobo recognises when you copy books to it over USB", while `.kepub` is what Calibre/CWA ingest expects — [README "Books Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md). Parallax already writes `KEpub/<name>.kepub.epub` — `crates/core/src/send.rs` L59-L75
- Kepubify has no plain `.epub` output, and `--calibre` switches its output extension to `.kepub` — [processor.py L808-L816](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L808-L816)
- KCC profile-key corrections, the list of Kobo profile codes, and KCC's removal of MOBI/KFX output because they need kindlegen or a Calibre plugin — [CHANGELOG](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md), [README "KCC Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- Parallax already has real-device-derived fixtures and notes: `tests/fixtures/kobo`, `tests/fixtures/calibre/schema.sql`, `docs/device-notes.md` (DbVersion 174, WAL behaviour, Shelf/ShelfContent schemas, AppleDouble hazard)

### Inferences

- What Parallax should take from bindery's tests is the case list for comic inputs: nested wrapper folders, hidden dirs, mixed PDF+archive folders, empty folders, dash-leading names, and unpadded numbers. These should become Rust tests with small generated CBZ fixtures.

### Gaps

- Bindery has no Kobo firmware quirks, database schema notes or Nickel behaviour for comics (for example whether Nickel reads ComicInfo from a CBZ). Those remain open in Parallax's spike checklist, `docs/kobo-comics-spike.md` "Device results: Not reported".
