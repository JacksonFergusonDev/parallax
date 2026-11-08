# Kobo comics spike

Question: is it worth converting manga to KEPUB for the Kobo, and should pages be processed for the screen, or is copying the CBZ as it is already the best option? Source is in [spikes/kobo-comics](../spikes/kobo-comics/README.md). It is outside the Cargo workspace and isn't part of the product.

**Verdict: processing for the screen is the clear win, for page-turn speed rather than picture quality.** The user compared all four on their Libra 2 on 2026-11-08 (see Device results).

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

The user's Libra 2 (firmware 4.38), 2026-11-08:

- **Quality:** the user could barely tell the four apart. The processed versions may look slightly better, and none looked noticeably worse.
- **Page turns:** the original CBZ (A) and the lossless KEPUB (B) both take about 2 seconds a page. The processed versions (C and D) turn almost immediately. So the full-size 1810 × 2560 JPEGs, not the container, make the Kobo slow.
- **Spreads:** A and B show the opening spread whole, on one screen. C and D show it as two separate pages, because processing splits it.
- **Not reported:** titles, series grouping, right-to-left page turns, ComicInfo in C, progress and stats. Without those, the case for KEPUB over CBZ is still open.

What it means:

- **Process pages by default:** greyscale, sized to the model's screen, 16 levels. This alone fixes the slow page turns, and it saves about 75% of the space on the device.
- **Spreads need a choice:** split them (fast, readable, but the whole picture is never seen at once), keep them whole sized to the screen, or show both (the whole spread, then its halves, as KCC can). This should be a setting.
- **Container:** still to decide, from the unreported metadata and RTL checks.

## Device checklist

Copy the files from `spikes/kobo-comics/out/` into a folder such as `Comics spike` on the Kobo with Finder, eject, and let it import. They take 3.5 GB; v01 alone is enough for most checks. Fill in each cell with what you see.

| Check | A | B | C | D |
| --- | --- | --- | --- | --- |
| Opens at all | | | | |
| Title shown in the library | | | | |
| Author shown | | | | |
| Cover thumbnail | | | | |
| Grouped as a series, in volume order (v01–v03) | | | | |
| Swiping or tapping left goes forward (RTL) | | | | |
| Time to open the book | | | | |
| Page turn time (a slow-motion phone video helps) | | | | |
| Sharpness of small text and screentone, side by side | | | | |
| Spread (v01 pages 9–10): whole, split, readable? | | | | |
| Zoom, if needed, works | | | | |
| Reading progress kept after closing and reopening | | | | |
| Shows in reading stats / "% read" | | | | |
| Survives a sleep and wake | | | | |

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
