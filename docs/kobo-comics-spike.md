# Kobo comics spike

Question: is it worth converting manga to KEPUB for the Kobo, and should pages be processed for the screen, or is copying the CBZ as it is already the best option? Source is in [spikes/kobo-comics](../spikes/kobo-comics/README.md). It is outside the Cargo workspace and isn't part of the product.

**Verdict: send manga as a processed KEPUB (variant D).** Processing makes pages open and turn 4–7 times faster, with no visible loss. KEPUB adds the author, which CBZ can't show. Series grouping needs the app's existing Kobo database writes whatever the format. The user compared all four on their Libra 2 on 2026-11-08 (see Device results).

## The four versions

Each test volume (Berserk v01–v03, Dark Horse digital, 1810 × 2560 JPEG pages, no ComicInfo.xml, some double-page spreads) is made four ways. This separates the effect of the container from the effect of processing the pages.

| | Original images | Processed for the Libra 2 |
| --- | --- | --- |
| **CBZ** | **A**: the original file, renamed | **C**: processed pages, plus a ComicInfo.xml |
| **KEPUB** | **B**: the original image bytes, repackaged | **D**: processed pages |

- **KEPUB packaging (B and D):** a fixed-layout EPUB 3 (`rendition:layout` `pre-paginated`, `page-progression-direction="rtl"`, no spreads) with one XHTML page per image. Its metadata has title, author, year, cover, and the series and number as both `calibre:series` and EPUB 3 `belongs-to-collection`. It then goes through the same kepubify 4.0.4 the app bundles, with `--no-add-dummy-titlepage`. Kepubify kept all of that and wrapped each image in a `koboSpan`.
- **Processing (C and D):**
  - convert to greyscale;
  - split double-page spreads, right half first for manga;
  - shrink to fit 1264 × 1680 with Lanczos3;
  - reduce to the panel's 16 grey levels;
  - save as 4-bit PNG, which is lossless at that depth.

  No margin cropping, contrast or gamma changes were applied.
- **Labels on the device:** titles carry the variant, and series names are "Berserk B", "Berserk C" and "Berserk D", so the variants don't group together. A shows its file name, "Berserk v01 [A original]". C's ComicInfo.xml says `Manga=YesAndRightToLeft`, to find out whether the Kobo reads ComicInfo at all.

## Desktop results

| Variant | Size of 3 volumes | Time per volume | Notes |
| --- | --- | --- | --- |
| A original CBZ | 1,501 MB | copy | 219–233 pages per volume |
| B lossless KEPUB | 1,521 MB (+1%) | 3.1–3.2 s | every image byte-identical to the CBZ (SHA-256 checked) |
| C processed CBZ | 369 MB (−75%) | 7.0–7.6 s | 228–240 pages after splitting spreads |
| D processed KEPUB | 369 MB (−75%) | 7.8–8.5 s | |

Measured on this Mac (release build, 4.3 cores busy). The whole run, three volumes and all four versions, took 37 s.

- **Lossless repackaging works.** B is the original bytes in a different zip, so its image quality can't be worse than A's. The quality loss you saw with calibre came from its image processing, not from KEPUB.
- **Processing is fast enough to do when sending.** About 8 s per 470 MB volume means converting at send time, with no permanent cache, is realistic.
- **Processed pages looked the same on the desktop.** A 400 px crop of a page at device size looked the same as macOS's own downscale of the original. Spreads split in the right order: the title half of the opening spread comes first. Whether 16 levels look right on e-ink is for the device test.

## Device results

The user's Libra 2 (firmware 4.38), 2026-11-08. The full checklist is below.

- **Speed is the big difference.** Opening a book takes 2 s for the originals (A, B) and 0.5 s processed (C, D). Turning a page takes 1.5 s against 0.2 s. The container makes no difference; the full-size 1810 × 2560 JPEGs (about 4.6 megapixels, above Kobo's guidance of about 3.8 for fixed-layout pages) make the Kobo slow.
- **Quality:** basically identical for all four. The processed versions may look slightly better.
- **Author:** shown only for the KEPUBs (B, D). C's ComicInfo.xml has a `Writer`, but the Kobo ignored it, which matches community reports that the stock firmware doesn't read ComicInfo. Titles showed for all four, but A and C's titles are their file names, so that proves nothing about ComicInfo.
- **Series:** not grouped for any of them. The KEPUBs carry both `calibre:series` and EPUB 3 `belongs-to-collection`, so firmware 4.38 doesn't read series from a sideloaded book's metadata. The app must write series into the Kobo's database itself, as it already does for books ("Kobo series on connect").
- **Right to left:** reported working for all four, including A, which has no direction information at all. Either the Kobo's comic viewer turns pages the same way whatever the book says, or the checklist wording ("swiping or tapping left goes forward") was read differently from what was meant. Recheck when the reader direction matters: tap the left edge on A and on B and see whether both go forward.
- **Spreads:** A and B show the opening spread whole on one screen; C and D split it into two pages.
- **Everything else** worked for all four: cover thumbnails, zoom, progress after reopening, reading stats, and sleep and wake.

What it means for the app:

- **Process pages by default:** greyscale, sized to each model's screen, 16 levels. It fixes the slow pages and saves about 75% of the space on the device.
- **Send KEPUB.** It's the only container that shows the author, it costs under a second on top of processing, and the app's send and update flows already expect `.kepub.epub`.
- **Write series to the Kobo's database** through the existing safe-write layer, the same as for books.
- **Spreads become a setting:** split (the spike's behaviour), keep whole sized to the screen, or the whole spread followed by its halves, which is KCC's default in its desktop app. Very wide images should be rotated rather than split, as KCC does.
- **Not yet known:** whether the KEPUB's right-to-left setting does anything beyond what the Kobo does for a CBZ, and whether sideloaded KEPUBs lose bookmarks and notes as reported elsewhere. Neither blocks the plan.

## Device checklist

Copy the files from `spikes/kobo-comics/out/` into a folder such as `Comics spike` on the Kobo with Finder, eject, and let it import. They take 3.5 GB; v01 alone is enough for most checks. Fill in each cell with what you see.

| Check | A | B | C | D |
| --- | --- | --- | --- | --- |
| Opens at all | y | y | y | y |
| Title shown in the library | y | y | y | y |
| Author shown | n | y | n | y |
| Cover thumbnail | y | y | y | y |
| Grouped as a series, in volume order (v01–v03) | n | n | n | n |
| Swiping or tapping left goes forward (RTL) | y | y | y | y |
| Time to open the book | 2s | 2s | 0.5s | 0.5s |
| Page turn time (a slow-motion phone video helps) | 1.5s | 1.5s | 0.2s | 0.2s |
| Sharpness of small text and screentone, side by side | identical | identical | identical | identical |
| Spread (v01 pages 9–10): whole, split, readable? | whole | whole | split | split |
| Zoom, if needed, works | y | y | y | y |
| Reading progress kept after closing and reopening | y | y | y | y |
| Shows in reading stats / "% read" | y | y | y | y |
| Survives a sleep and wake | y | y | y | y |

Then delete the spike folder from the Kobo and `spikes/kobo-comics/out/` from the Mac.

## How the result decides the design

- **If B beats A on metadata, series or RTL, and pages turn as fast:** the app sends manga as a lossless KEPUB by default. It takes seconds and loses nothing.
- **If D also looks as good or better and turns pages faster:** add processing as the "Prepare for Kobo" option, or make it the default, sized per Kobo model. It also saves about three quarters of the space on the device.
- **If C shows the ComicInfo metadata:** CBZ stays a valid choice, and the app only needs to write ComicInfo.
- **If no KEPUB variant beats A:** keep copying CBZs, and spend the effort on the library and reader instead.
- **In every case, nothing converted is stored on the Mac.** The Kobo's copy is the only one.

## Not tested here

- Colour Kobos.
- Other screen sizes.
- Margin cropping.
- Contrast and gamma curves.
- Rotating spreads instead of splitting them.
- Kobo's own store manga, as a reference.
- Kepubify's handling of very large books beyond these three.
