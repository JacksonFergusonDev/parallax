# Comics and manga plan

Status: **C1–C7 implemented** (2026-11-08). The physical Kobo checklist is open, and C8 is deferred. The plan for first-class comics and manga in Parallax, across the library, sending to any supported Kobo, and the reader. It builds on [the Kobo comics spike](kobo-comics-spike.md) and [the bindery and KCC research](../reports/Bindery%20and%20Kindle%20Comic%20Converter.md).

## 1. What's already decided

| Question | Decision | Evidence |
| --- | --- | --- |
| What to send to a Kobo | Pages processed for that model's screen, in a fixed-layout KEPUB made through the bundled kepubify | Libra 2 spike: processed pages open in 0.5 s and turn in 0.2 s, against 2 s and 1.5 s for the originals, with no visible loss. Only KEPUBs showed the author. |
| How series reach the Kobo | Write `content.Series`, `SeriesNumber` and `SeriesID` through the existing safe-write layer on reconnect after Nickel imports the books | No spike variant grouped as a series; firmware 4.38 ignores OPF series and ComicInfo for sideloaded books. |
| Caching converted output | None. Prepare in a temporary folder at send time, write to the Kobo, delete | About 8 s per 470 MB volume on this Mac; calibre's growing cache was the user's main complaint. |
| Default page processing | Greyscale, Lanczos3 to fit the screen, 16 grey levels as 4-bit PNG, no dithering, no autocontrast, no cropping | Spike quality matched the originals. KCC users report autocontrast darkening pages and crops removing content. |
| Spreads | A setting: **Split** (default), **Keep whole**, **Whole then halves**. Very wide images (aspect ≥ 1.8) are rotated rather than split | KCC supplies the thresholds and half order. Parallax keeps the whole image unrotated in both whole modes for ordinary spreads, and puts it first in Whole then halves. |
| Where code comes from | Adapt KCC v13.0.0 for spreads, colour detection and spine alignment. Reuse the Parallax spike for resize, 4-bit encoding and fixed-layout EPUB. C2 implemented ComicInfo, natural sorting and bindery-inspired bundling. Keep the upstream notices with the Rust code | Research report. |
| CBR | Read through libarchive and convert to CBZ on import, keeping the original's hash as its identity, like Amazon formats. Never link unrar | UnRAR's licence forbids recreating RAR compression, which conflicts with GPLv3. |

## 2. Supporting the range of Kobos

Parallax writes to any Kobo with Kobo firmware 4.x and database schema 53–222 (`docs/device-support.md`). Comics should work on all of them, so every Kobo gets a **screen profile**: the screen size pages are processed to, and whether it shows colour. The model id comes from `.kobo/version`; ids and names are from pgaskin's koboutils, screen sizes from KCC's profiles and Kobo's specifications.

| Model id | Model | Screen (portrait) | Colour |
| --- | --- | --- | --- |
| 310, 320 | Touch | 600 × 800 | no |
| 330 | Glo | 758 × 1024 | no |
| 340 | Mini | 600 × 800 | no |
| 350 | Aura HD | 1080 × 1440 | no |
| 360 | Aura | 758 × 1024 | no |
| 370 | Aura H2O | 1080 × 1430 | no |
| 371 | Glo HD | 1072 × 1448 | no |
| 372 | Touch 2.0 | 600 × 800 | no |
| 373, 381 | Aura ONE (and Limited Edition) | 1404 × 1872 | no |
| 374, 378 | Aura H2O Edition 2 | 1080 × 1440 | no |
| 375, 379 | Aura Edition 2 | 758 × 1024 | no |
| 376 | Clara HD | 1072 × 1448 | no |
| 377, 380 | Forma (and 32 GB) | 1440 × 1920 | no |
| 382 | Nia | 758 × 1024 | no |
| 383 | Sage | 1440 × 1920 | no |
| 384 | Libra H2O | 1264 × 1680 | no |
| 386 | Clara 2E | 1072 × 1448 | no |
| 387 | Elipsa | 1404 × 1872 | no |
| 388 | Libra 2 (reference device) | 1264 × 1680 | no |
| 389 | Elipsa 2E | 1404 × 1872 | no |
| 390 | Libra Colour | 1264 × 1680 | yes (Kaleido 3) |
| 391 | Clara BW | 1072 × 1448 | no |
| 393 | Clara Colour | 1072 × 1448 | yes (Kaleido 3) |

- **Unknown models** use the size of the Kobo's own full-screen cover cache (`N3_FULL` in `.kobo-images`, which device reports already measure). If there are no cached covers yet, they fall back to 1072 × 1448, the most common size, and the send dialog says so. A device report adds the model properly.
- **Tolino readers** stay read-only, as today.
- **Colour Kobos** keep colour pages in colour, saved as JPEG at full screen size, and still turn greyscale pages into 16-level PNG. A page counts as colour by KCC's chroma test. No colour Kobo has been tested; this needs someone with a Libra Colour or Clara Colour before it's called supported.
- **Old and slow Kobos** (600 × 800 and 758 × 1024) benefit most from processing. Whether their firmware handles fixed-layout KEPUBs is unknown. If one doesn't, that model gets a processed CBZ instead, recorded in its profile; the processing is the same.
- **Profiles are data, not code paths.** One table, `MODELS` in `crates/device-kobo/src/screen.rs`, gives each model id its name, screen size and colour. A container field (KEPUB or CBZ) is added only if a model turns out to need CBZ.

## 3. Milestones

Each milestone ends with `just check` passing and is committed on its own. Sizes are rough.

### C1. Kobo screen profiles (done)

- `screen::MODELS` lists every id above. `DeviceInfo.screen` (`erc_device::Screen`: width, height, colour, and source `model`, `covers` or `assumed`) comes from the model id, then the tallest of up to 24 `N3_FULL` covers (width from a known model of that height, or 3:4), then 1072 × 1448.
- About this Kobo shows the screen and says when it was measured or assumed.
- Tests: unique ids and portrait screens; known models; the Libra 2 report's cover sizes give 1264 × 1680; covers measured on disk; the assumed fallback; the About text.

### C2. Comic archive crate, `erc-comic` (done)

`crates/comic`, with no app code using it yet:

- **Archives.** `sniff` recognises the format from its first bytes, since extensions are often wrong. `Cbz` lists pages and reads them one at a time. `convert_to_cbz` turns CBR, CB7, or anything else libarchive reads into a stored CBZ, keeping ComicInfo.xml and listing what it left out.
- **libarchive decision.** CBR and CB7 are read by the libarchive built into macOS (3.7.4 here: RAR4, RAR5 and 7z), through `compress-tools`. A pkg-config shim, `scripts/pkg-config-libarchive.sh`, set as `PKG_CONFIG` in `.cargo/config.toml`, links it as `-larchive` against the SDK's `libarchive.tbd`. Nothing is bundled, and unrar isn't used.
- **Pages.**
  - Images (JPEG, PNG, GIF, WebP) in natural order of their full paths.
  - `__MACOSX`, dot files, `Thumbs.db` and `desktop.ini` are skipped as hidden.
  - Absolute paths, `..` and drive letters are skipped as unsafe, and case-insensitive duplicates are skipped. Every skipped file is listed with its reason.
  - A single wrapper folder is reported.
  - Limits: 20,000 files, 256 MB per page, 200 MP per image.
  - Page sizes come from headers: about 40 ms for a 230-page volume.
- **ComicInfo.xml.**
  - Every simple field is kept and `<Pages>` is kept verbatim.
  - Getters cover the common fields, plus credits by role, `Manga`, and the FrontCover page.
  - `write_comic_info` rewrites a CBZ beside itself with every other entry copied raw, then swaps it in.
- **File names.** `parse_file_name` reads series, volume (`v01`, `Vol. 12`, `Volume 3`), chapter (`c097`, `ch.45.5`), issue (`#12`), bare trailing numbers, year in brackets, and a title after a dash (`v01 - Title`). It ignores scan-group brackets and keeps numbers that belong to the series ("Mob Psycho 100").
- **Shapes.** Spreads are wider than 1.16 and Wide pages ≥ 1.8 (KCC's thresholds), Strips are ≥ 2.5 tall, and `Shapes::is_webtoon` is true when most pages are strips.
- **Bundling.** `classify_folder` gives Images, ChapterArchives, Mixed or NotComic, following bindery's rules. `folder_to_cbz` and `chapters_to_cbz` write `NNN - <chapter>/` folders in natural order; chapters may be zip, RAR or 7z.
- **Tests.**
  - 26 unit tests and 10 integration tests. Fixtures in `tests/fixtures/comics`: a bsdtar-made 7z comic, and libarchive's own RAR4 and RAR5 files, since RAR can't be made without the proprietary tool.
  - Edge cases from bindery and KCC: unpadded numbers, wrappers, hidden folders, dash-leading names, `..` entries, zero-height images, thresholds, and truncated archives.
- **Checked on real files.** The three Berserk volumes: 219, 233 and 227 pages in order, "Berserk" with volumes 1–3 and years, and 9, 7 and 13 spreads (v01 matches the spike's count).
- **Not covered.** A real CBR comic, since there's no RAR comic to test with, and password-protected archives beyond mapping libarchive's error.

### C3. Comics in the library (done)

- **Schema 15:** `format` (`epub`, `cbz`), `kind` (`book`, `comic`, `manga`) and `direction` (`ltr`, `rtl`) on `books`. The page count goes in `pages` with `pages_estimated` off, and `words` stays empty. The opening backfills skip comics.
- **Storage:** comics are `books/<id>/book.cbz`. `Library::book_file` picks `book.cbz` or `book.epub` from the folder, so snapshots, restore and every caller work unchanged.
- **Import:**
  - `Library::import` tells comics from EPUBs by content: a zip without an EPUB container, or a RAR or 7z.
  - CBZ is stored byte for byte. CBR and CB7 are converted to CBZ and identified by the original's hash, so adding the original again finds the book.
  - `ingest::import_file`, the Add Books dialog, drag and drop (`cbz`, `cbr`, `cb7` added to `BOOK_EXTENSIONS`) and Add from Kobo all go through it. Comics on a Kobo are imported as they are.
  - Bundling folders isn't wired to the app yet; it needs the folder picker in C4.
- **Metadata** (`crates/library/src/comics.rs`):
  - ComicInfo first, the file name for what it lacks.
  - Title: ComicInfo's Title, else "Series Vol. N", else "Series #N", else the series. Series index is Volume, else Number.
  - Authors are the writers, or the pencillers when no writer is listed. Other credits become contributors with MARC codes: `art` for penciller and inker, `clr`, `cov`, `edt`, `trl`, and `ctb` for letterer. The editor offers Artist and Colourist.
  - Genre and Tags become tags. Year and month become published; Summary, Publisher and LanguageISO map directly; GTIN becomes the ISBN.
- **Kind and direction:**
  - ComicInfo `Manga`: YesAndRightToLeft is manga read right to left, Yes is manga read left to right (as the schema defines it), and No is a comic.
  - Otherwise the setting `comics_kind` decides. Its default is manga; `comic` makes them Western comics. The Settings UI for it comes in C4.
  - `Library::set_comic_kind` changes a comic's kind and direction; its command and UI come in C4.
- **Edits:** like EPUBs, the stored original is never rewritten. `Library::export` writes the library's metadata into the exported copy's ComicInfo.xml, keeping fields the library doesn't manage. (This corrects the earlier plan, which said edits would rewrite the CBZ.)
- **Covers:** the FrontCover page or the first page gives the thumbnail, cover size, cover source and reset. Custom covers still work, but they aren't written into exported CBZs.
- **Check library:**
  - Hidden files count as clutter, and other non-page files as extra files.
  - A new check, "Damaged pages", covers pages whose size can't be read; the import review counts those as damaged.
  - Comics without ComicInfo get "No author".
- **Tests:** 8 library tests, 2 core tests and 4 metadata unit tests. A real Berserk volume imports as "Berserk Vol. 1", series Berserk #1, 219 pages, manga read right to left, with a 1810 × 2560 cover, in 0.46 s.

### C4. Comics in the library UI (done)

- A Books / Comics / Manga filter in the library toolbar.
- A series view where volumes stack under their series, for all kinds.
- **Detail panel:** pages instead of words, art credits, direction, and spread count.
- Edit kind and direction.
- Add comic folder imports a folder of pages or chapter archives as one volume. Settings › Library sets the fallback kind for archives without ComicInfo reading metadata.
- Checked at 1280 × 900 and 800 × 560 with browser mock data.

### C5. Prepare for Kobo (done)

- **`erc_comic::prepare::prepare`** writes a temporary KEPUB and uses rayon in bounded batches. [KCC image notes](../research_notes/Bindery%20and%20Kindle%20Comic%20Converter/kcc_image_pipeline.md) map spread and chroma logic to `image.py`. The [spike implementation](../spikes/kobo-comics/src/main.rs) is the reference for resize, PNG and EPUB packaging.
  - decode;
  - test for colour;
  - greyscale, unless the page is colour on a colour Kobo;
  - handle spreads by the setting, ordered by direction;
  - rotate very wide pages;
  - resize with `fast_image_resize` Lanczos3, never upscaling by default;
  - quantise to 16 levels;
  - encode as 4-bit PNG, or JPEG for colour pages.
- **Package:**
  - fixed-layout EPUB 3 with title, credits, series, cover and `page-progression-direction`;
  - KCC’s `buildOPF` aligns `rendition:page-spread-left/right` for facing halves. The implementation gates it on the profile, with no models enabled pending a landscape hardware check;
  - preserve the source or custom cover separately from page processing. Keep JPEG and PNG bytes and use lossless PNG for other supported cover formats;
  - kepubify with `--no-add-dummy-titlepage`;
  - `safe_kobo_component` applies the KCC character list to device filenames. C6 uses a fixed safe parent directory.
- **Settings:**
  - spreads mode;
  - optional white-margin crop, greyscale autocontrast and upscale for small pages, all off by default. Crop trims each part after a spread is split, so the split stays on the gutter. Autocontrast stretches between the 0.5% percentiles and leaves nearly flat pages alone;
  - pages that aren't readable images are left out and named in the send result. A comic with no readable pages is refused;
  - a per-book override of direction.
- **Checks** (as the spike did):
  - output images match the processing;
  - kepubify kept fixed layout and direction;
  - page count matches the source plus splits.
- **Performance:** the spike took about 8 s per volume. Pages are read in order, processed on the rayon pool as soon as memory allows (32 MP decoded, 64 MB encoded in flight), and written back in order, so no core waits for a batch. On an M3, preparing Berserk v01 (219 pages) for a Libra 2 takes about 4.3 s in release, against 5.1 s with batches and 16 s on one thread. About 73% of the work is decoding its progressive JPEGs: libjpeg-turbo was no faster and ImageIO slower, and the remaining resize and greyscale steps are too small a share for GPU or Accelerate to pay off. Development builds optimise erc-comic, which was 34 s unoptimised. Progress is reported per input page.
- **Dithering:** serpentine Floyd–Steinberg diffusion to the 16 greys, as Pillow and so KCC do, is on by default and can be turned off in Settings › Kobo. On a Libra 2 comparison of four Berserk pages it looked very subtly better. On Berserk v01 it halved tone error at reading distance for 4% more space, and ordered (Bayer) dithering gave no gain. Turning it on changes the update fingerprint, so comics sent earlier are offered for update.
- **Colour pages on greyscale screens:** every page is checked for colour (KCC's chroma histograms on every second pixel of every second row, which decided all 1,403 pages of seven Berserk volumes as the full check did). Greyscale pages keep the plain path. On a greyscale Kobo, colour pages go through `prepare/tone.rs`, each step chosen in Libra 2 comparisons of five colour pages:
  - colour-aware greys, a simplified form of Grundland and Dodgson's Decolorize. Chroma along the direction luma loses most is added to luminance at 0.75 of the lost contrast; 0.5 was invisible on the device and 1.0 darkened brown skin too far;
  - a contrast stretch between the 0.5% percentiles;
  - darker mid-tones on mostly light pages only, rising from gamma 1.0 at a mean of 0.45 to 1.4 at 0.65. Fixed darkening helped light pages but muddied dark ones.
- **Margin cleaning:** not adopted. On four typical pages no difference was visible, but across seven volumes a fixed white point of 232 would erase a pale title-page logo whose content lies almost entirely in that band, and a fixed black point of 20 merges real shadow detail on dark pages.
- **Settings:** Settings › Comics holds every page option with its recommended default: spreads, trim and enlarge, dithering and contrast for black-and-white pages, and for colour pages on greyscale Kobos a contrast switch plus colour separation and mid-tone darkening strengths (off, 50–200% of the recommended strength). "Reset to recommended" clears all comic page settings together. Options at their recommended values are left out of the serialised form, so fingerprints of existing copies don't change.
- **CPU:** aarch64-apple-darwin builds target Apple M1 features, NEON included. The JPEG decoder (zune-jpeg) and resizer (fast_image_resize) use their own NEON code, with SSE and AVX2 chosen at runtime on Intel; zlib-rs is SIMD-accelerated. Our per-pixel loops (luma, 4-bit packing, margin scan) work over raw byte slices so they compile to vector code. Floyd–Steinberg dithering carries each pixel's error to the next, so it can't be vectorised within a row; it uses integer sixteenths and a level table, 9.8 ms per page against 12.0 ms for `image::imageops::dither`, which also lacks serpentine scanning. Pages run in parallel on the rayon pool. On an M3, Berserk v01 takes 4.3 s (18.4 s on one thread, down from 25.2 s), about 60% of it JPEG decoding. CI builds and tests the workspace on an Intel Mac runner.
- **Page encoding:** 4-bit greyscale PNG with no row filter at the png crate's Balanced level. For Berserk v01 the KEPUB is 115.5 MB, from a 448 MB CBZ; Fast with filters was 148 MB, High saved only 2% more at four times the encode time, and 8-bit JPEG needed quality 75 to match the size. Stock Kobo firmware shows WebP blank (its Qt predates WebP), so pages stay PNG, or JPEG for colour.

### C6. Sending and updating comics (done)

- `send_library_books` and `update_on_device` prepare comics for the connected Kobo's profile in a temporary folder, then write through the existing safe-write layer to `KEpub/<Series> v<NN> - <Title>.kepub.epub`.
- **Series:** use the existing reconnect reconciliation after Nickel imports the book. The S1 spike found that import triggers needed a Kobo restart, so they were dropped from the [device plan](PLAN.md). Updates to an already imported comic write its series in the update session.
- **Device copies:** recorded per Kobo with a hash of the effective screen and settings. A mismatch offers "Update on Kobo" where a copy already exists. A screen guessed from cover sizes or assumed is left out of the hash, as it changes while Nickel makes covers. A raw comic added from the Kobo is never offered for update, as replacing it would put a KEPUB in an archive's file.
- **Space:** the send and update dialogs show an estimate from page count and screen size and the available space. The finished KEPUB is measured again before copying.
- **No cache:** nothing converted is kept on the Mac after a send, including after a failure.
- **Tests:** the FAT32 disk-image tests send and update a CBZ; fixture devices of three screen sizes produce correctly sized pages.

### C7. Reading comics in Parallax (done)

- A dedicated raster viewer uses `reader_entry` to load pages from the open CBZ, with two pages preloaded ahead of the visible ones. The cover and landscape images appear alone, and ordinary portrait pages pair automatically in wide windows.
- **Direction:** right to left flips horizontal keys, click zones, spread order and the slider.
- **Viewing:** fit width, height or page, with zoom and drag to pan. Vertical scroll is available for all comics and is the default for mostly long strips.
- **Positions and bookmarks:** the zero-based page index is saved as `comic-page/<index>` in `reader_positions`. Bookmarks reuse the reader’s book-scoped annotation storage.
- **Window access:** the existing reader grants cover the raster and bookmark operations. The pinned grant test still enforces the same scope.
- **Tests:** `just test-reader` opens generated CBZs, turns pages in both reading directions, checks bookmarks and resume, and exercises vertical scroll. The browser mock was checked at 860 × 1000 and 420 × 520.

### C8. Online comic and manga metadata

- Find metadata and import review search Mangainfo for manga series aliases, volumes and ISBNs. Mangainfo needs no account. Its synopsis and creator credits describe the series, so they start unticked in review.
- Western comic issues use Metron. Add a personal API token in Settings › Comics. Parallax uses known catalog IDs or the series, run and issue clues in ComicInfo and the filename, and compares the archive cover to break ties. If more than one issue still matches, select it in the review before saving.
- Accepted changes fill gaps and retain the catalog source. Generated KEPUBs include the synopsis and source, and chapter navigation uses embedded chapter bookmarks or chapter folders.

### Later

- Kobo reading progress mapped to pages.
- KCC's webtoon panel splitter and colour "rainbow" filter.
- PDF comics.
- Kindle.

## 4. Device verification

Nothing here counts as working on a Kobo until someone runs it on one.

- **Libra 2** (the user's): the full send, update and series flow for each spread mode, and the right-to-left recheck. Tap the left edge in a raw CBZ and in an RTL KEPUB; if both go forward, the Kobo ignores the book's direction.
- **Other models:** a "Comics" section in `docs/hardware-checks.md`, so anyone with another Kobo can run the same checks and send a device report. The model table in §2 is from published specifications until then.
- **Colour Kobos:** colour pages, rainbow artefacts and whether greyscale PNGs look right on Kaleido.
- **Older Kobos:** whether fixed-layout KEPUB opens at all; if not, switch that profile to CBZ.

## 5. Risks and open questions

- **Bookmarks and notes:** reportedly unavailable in sideloaded fixed-layout KEPUBs (KCC issues 418 and 704). Acceptable for manga; noted in the send dialog's help.
- **CBR support:** depends on C2's libarchive choice. If neither route reads RAR5 cleanly, CBR import ships later and CBZ goes first.
- **Spread pinning:** whether `rendition:spread` and page-spread pinning matter on models that rotate to landscape is untested; the spike used `none` with split pages, which worked.
- **Large libraries:** volumes are 0.5 GB each; import copies them, as for books. Linking files in place instead of copying would save disk space, but it affects the whole library and is a separate decision.
