# KCC output formats, Kobo behaviour and metadata, and a Rust porting assessment

Source snapshot: `git clone --depth 50 https://github.com/ciromattia/kcc`, HEAD `5b7f803` "bump 13.0.0" (2026-11-07), `kindlecomicconverter/__init__.py` says `__version__ = '13.0.0'`. Code paths below are relative to the repo root (<https://github.com/ciromattia/kcc/blob/master/>...). Line numbers are for that commit. Nothing in the clone was run or built.

## Output formats KCC supports and what it uses for Kobo

### Takeaway

KCC writes MOBI/AZW3 (dual file via KindleGen), EPUB, "KEPUB", CBZ, PDF, a folder of images, and a "KFX" EPUB intended for Send to Kindle or the KFX Output calibre plugin. For any Kobo profile, Auto and the GUI default are EPUB, and EPUB on a Kobo profile is written as `.kepub.epub`. KCC's "KEPUB" is a plain fixed-layout EPUB 3 that is only renamed: it doesn't use kepubify and adds no `koboSpan` markup.

### Cited findings

- CLI formats: `Auto, MOBI, EPUB, CBZ, KFX, MOBI+EPUB, PDF`, with `Auto` as the default (`kindlecomicconverter/comic2ebook.py` 1527–1529). The GUI also has "Preview/Folder of images", "PDF (200MB limit)", "EPUB (200MB limit)" and "MOBI + EPUB (200MB limit)" (`KCC_gui.py` 1575–1586) — [KCC repo](https://github.com/ciromattia/kcc)
- Auto resolution: KF7 Kindles get CBZ, other Kindles get MOBI, reMarkable gets PDF, everything else (Kobo included) gets EPUB (`comic2ebook.py` 1668–1676). Every Kobo GUI profile has `'DefaultFormat': 1`, which is "EPUB" (`KCC_gui.py` 1642–1683, 1575–1577) — [KCC repo](https://github.com/ciromattia/kcc)
- MOBI, KFX and the 200 MB EPUB format are refused for non-Kindle profiles ("MOBI/Send to Kindle not supported for non-Kindle profiles", `comic2ebook.py` 1637–1638). KFX is really EPUB with `options.kfx=True`, a 195 MB target size and panel view off (1706–1710) — [KCC repo](https://github.com/ciromattia/kcc)
- Kobo naming: if the profile name contains `Ko` and the format is EPUB, the extension is `.kepub.epub` unless `--nokepub` is set (`comic2ebook.py` 1080–1085, help text 1530–1531). The name has ``/\?%*:;|"'><$!`` replaced with `_` (`KOBO_TOUCH_DRIVER_REGEX`, line 73, applied 1102–1103). Collisions get a `_kcc<N>` suffix (1107–1113) — [KCC repo](https://github.com/ciromattia/kcc)
- No kepubify: no source file mentions kepubify (grep across `kindlecomicconverter/*.py`). A KCC maintainer wrote in issue #418 that "Simply renaming an epub to kepub like kcc does won't work according to kepub spec" and suggested kepubify — [kcc#418](https://github.com/ciromattia/kcc/issues/418)
- README recommendation: "KEPUB for Kobo (Kobo profile + EPUB = KEPUB)", "CBZ for Koreader", "PDF for ReMarkable" (README.md 166–168). It also advises against calibre, because modifying KCC output in calibre "can break the formatting" (155–160) — [KCC README](https://github.com/ciromattia/kcc#faq)
- Kobo profiles and resolutions (portrait px), all `Palette16` with gamma 1.0 (`image.py` 121–136): Mini/Touch 600×800; Glo 768×1024; Glo HD 1072×1448; Aura 758×1024; Aura HD 1080×1440; Aura H2O 1080×1430; Aura ONE 1404×1872; Nia 758×1024; Clara HD/2E/BW 1072×1448; Clara Colour 1072×1448; Libra H2O/Libra 2 1264×1680; Libra Colour 1264×1680; Forma 1440×1920; Sage 1440×1920; Elipsa 1404×1872 — [KCC repo](https://github.com/ciromattia/kcc)
- On Kobo profiles, panel view and HQ are forced off (`comic2ebook.py` 1702–1704) — [KCC repo](https://github.com/ciromattia/kcc)
- GUI Kobo profiles default to Upscale on, and only Clara Colour and Libra Colour force colour (`KCC_gui.py` 1670–1683) — [KCC repo](https://github.com/ciromattia/kcc)
- Default image codec: JPEG quality 85 (90 for Kindle Scribe/Colorsoft) (`comic2ebook.py` 1725–1729). PNG only with `--forcepng`, and then quantised to the 16-colour palette unless `--noquantize` (`comic2ebook.py` 1590–1599; `image.py` 498–525, 577–584). `--mozjpeg` and `--webp` are opt-in (1594–1601) — [KCC repo](https://github.com/ciromattia/kcc)

### Inferences

- KCC's `.kepub.epub` just flips the Kobo firmware onto its KEPUB (WebKit) renderer for a fixed-layout book. Parallax's spike pipeline (fixed-layout EPUB 3, then kepubify) is a superset: it also adds `koboSpan`s. Kepubify's spans may matter to Kobo's progress and stats, which KCC's output lacks. This is unverified on a device.
- Kobo's own spec names `.fxl.kepub.epub` for fixed layout. KCC uses `.kepub.epub` and relies on `rendition:layout pre-paginated` in the OPF instead. Community use suggests this works.

### Gaps

- No KCC documentation explains why EPUB/KEPUB is preferred over CBZ for Kobo, beyond the README one-liner. No benchmark was found.

## How KCC builds EPUB/KEPUB (OPF, XHTML, nav, cover, zip)

### Takeaway

KCC writes everything with string templates. There is one XHTML page per image with a `viewport` equal to the image size, and an EPUB 3 OPF with `rendition:layout pre-paginated` and `rendition:spread landscape`. On Kobo the spine has per-page `rendition:page-spread-left/right/center`, and `page-progression-direction` is `rtl` in manga mode. The Kindle-only metas (`fixed-layout`, `original-resolution`, `book-type comic`, `primary-writing-mode`, `region-mag`, and so on) are not emitted for Kobo. Series goes out as EPUB 3 `belongs-to-collection` (no `calibre:series`). Both NCX and nav are written, the cover is a manifest image only, and the result is zipped STORED with `mimetype` first.

### Cited findings

- **OPF** (`buildOPF`, `comic2ebook.py` 305–516):
  - Package: `version="3.0"`, `unique-identifier="BookID"`, no `prefix` attribute. Metadata: `dc:title`, `dc:language` (default `en-US`, `--language`), `dc:identifier` `urn:uuid:<uuid>`, `dc:contributor` "KindleComicConverter-<ver>", optional `dc:description` from the ComicInfo Summary, one `dc:creator` per author, and `dcterms:modified` (326–338, 349).
  - Series, for non-Kindle profiles only: `<meta property="belongs-to-collection" id="c02">Series</meta>` and `collection-type` = `series`. `group-position` is `Volume.Number` when both exist, otherwise Volume or Number (339–347). This was added in response to issue #1186, which asked for NickelSeries-compatible EPUB 3 series metadata ([kcc#1186](https://github.com/ciromattia/kcc/issues/1186)).
  - Cover: `<meta name="cover" content="cover"/>` plus a manifest item `Images/cover.jpg` with `properties="cover-image"` (350–351, 380–383).
  - Kindle-only block (`if options.iskindle and options.profile != 'Custom'`): `fixed-layout`, `original-resolution` (device res, or the KFX resolution), `book-type comic`, `primary-writing-mode` (`horizontal-rl/lr` or `vertical-*`), `zero-gutter`, `zero-margin`, `ke-border-color`, `ke-border-width`, `orientation-lock none`, `region-mag` (352–375).
  - All profiles: `<meta property="rendition:spread">landscape</meta>` and `<meta property="rendition:layout">pre-paginated</meta>` (376–379).
  - Manifest: `ncx`, `nav` (`properties="nav"`), one XHTML and one image per page with the media type from the extension (png/gif/webp/jpeg), and `Text/style.css` (380–411).
- **Spine** (`comic2ebook.py` 414–503):
  - Direction: `page-progression-direction="rtl"` with `righttoleft`, otherwise `ltr`. `--invertdirection` swaps it (423–440).
  - Per-item properties: Kobo gets `properties="rendition:page-spread-<side>"`, Kindle gets `linear="yes" properties="page-spread-<side>"`, and other profiles get nothing (414–420).
  - Side assignment: sides alternate, starting `right` for RTL and `left` for LTR. The start flips for PDF/EPUB sources and with `--spreadshift` (441–449). Pages from split or rotated spreads are pinned: the whole or rotated spread (`-kcc-a`/`-kcc-d`) is `center`, and the two halves (`-kcc-b`/`-kcc-c`) are right/left in RTL or left/right in LTR (451–480). A backward pass re-aligns the pages before each spread so the spread halves land facing (482–496). `--onepagelandscape` forces `center` (498–501).
  - This alignment logic was fixed after a Kobo Libra Colour report that landscape two-up showed only half of an even-numbered spread ([kcc#930](https://github.com/ciromattia/kcc/issues/930)).
- **container.xml** points to `OEBPS/content.opf` (505–515).
- **XHTML page** (`buildHTML`, 108–235):
  - Shell: XML declaration, `<!DOCTYPE html>`, XHTML namespace plus the `epub` namespace, `<title>` = file stem, and a link to `style.css`.
  - Viewport: `<meta name="viewport" content="width=W, height=H"/>`, where W and H are the image's pixel size (heights are summed for "above/below" split tall images).
  - Body: `background-color:#000000` if the page was detected as black-background. A `<div style="text-align:center;">` holds `<img width height src="../Images/...">`. Kindle panel-view markup (`app-amzn-magnify` divs) is only emitted for Kindle with panelview (159–233), so never for Kobo.
- **CSS** (`buildEPUB`, 518–599): `@page {margin:0}` and `body {display:block; margin:0; padding:0}`. The `#PV*` panel-view rules are Kindle-only.
- **NCX** (`buildNCX`, 238–266) has `dtb:uid`, `depth 1` and `totalPageCount 0`. There is one navPoint per chapter, where a chapter is a subfolder or a ComicInfo `<Page Bookmark=...>`. **nav.xhtml** (`buildNAV`, 268–303) has a `toc` nav and an `epub:type="page-list"` with the same entries. With ComicInfo bookmarks, chapter page indices are shifted to account for split spreads (`comic2ebook.py` 618–655).
- **Cover**: `cover.save_to_folder(...'OEBPS/Images/cover.jpg')` (604–606). The `Cover` class resizes to the device (thumbnail, or `fit` with `--coverfill`) and saves JPEG at `jpegquality` (`image.py` 639–720). `--smartcovercrop` crops the front cover from a wraparound, choosing the side by RTL (`image.py` 667–699). There is no separate cover XHTML page, although Kobo's spec says "Cover images must be contained in their own XHTML files" for automatic extraction ([kobolabs/epub-spec](https://github.com/kobolabs/epub-spec)).
- **Zip** (`makeZIP`, 1445–1472): with 7-Zip present, `7z a -mx0 -tzip` and the "crazy hack" of adding `!mimetype` and renaming it to `mimetype` so it sorts first. Otherwise Python `ZipFile` with `ZIP_STORED`, writing `mimetype` first. Everything is uncompressed.
- **File and folder names** inside the EPUB are ASCII-slugified (`slugify`, 1434–1443), and pages are named `kcc-NNNN-kcc...` with `-kcc-a/b/c/d` suffixes for spread variants (evident in the spine logic and [kcc#930](https://github.com/ciromattia/kcc/issues/930) snippets).
- **Batch splitting**: titles get `[n/N]` when a book is split into tomes (`comic2ebook.py` 2050–2055). `EPUB-200MB` sets a target size of 195 MB (1645–1648).

### Inferences

- KCC's choice of `rendition:spread landscape` lets Kobo show two-up spreads in landscape. Parallax's spike used `none`. Kobo's spec says it supports all five values and recommends `auto` or `landscape` for most fixed-layout books, but `none` for books with hard-coded two-page spreads ([kobolabs/epub-spec](https://github.com/kobolabs/epub-spec)). For a "keep spreads whole" mode, `none` matches Kobo's guidance. For split spreads, `landscape` with page-spread pinning is how KCC reconstitutes spreads in landscape.
- Parallax's spike OPF already writes both `calibre:series` and `belongs-to-collection` (`spikes/kobo-comics/src/main.rs` 463–485), which is a superset of KCC.

### Gaps

- I didn't read `shared.py` `walkSort`/`getImageFileName` in detail. The exact page-naming scheme is inferred from the spine code.

## Metadata: ComicInfo.xml handling and title, series and author sources

### Takeaway

KCC reads a small subset of ComicInfo.xml: Series, Volume, Number, Title, Summary, Writer, Penciller, Inker, Colorist, and `<Page Bookmark>`. It builds the title as `Series Vol. NN #NNN` and the authors from the creator fields. It writes these into EPUB/MOBI metadata and then deletes ComicInfo.xml from the output. ComicInfo is copied into CBZ output only with `--keepcomicinfo`, and the code comments that it "may break readers like the Kobo native CBZ reader". Otherwise metadata comes from the filename or folder name, CLI `--title`/`--author`, or the GUI metadata editor, which writes ComicInfo back into the source archive with 7z.

### Cited findings

- Parser fields (`metadata.py` 30–80): Series, Volume, Number, Summary, Title; Writer, Penciller, Inker and Colorist, split on `", "`, de-duplicated and sorted; and `Page` elements with `Bookmark` and `Image` attributes as chapter marks. `saveXML` (82–126) writes the same fields back, adding ComicInfo.xml into the archive with `7z a` (RAR is not supported: `comicarchive.py` 107–113) — [KCC repo](https://github.com/ciromattia/kcc)
- `getMetadata` (`comic2ebook.py` 1125–1197):
  - Default title and author: the title defaults to the file stem or folder name, and the author to "KCC" (1133–1148).
  - Series-based title: with a ComicInfo and no user title, title = Series + ` Vol. ` + Volume zero-padded to 2 + `#` + Number zero-padded to 3. `--metadatatitle 1` appends `: Title`, and `2` uses Title alone (1155–1168, help text 1518–1520).
  - Authors: the union of Writers, Pencillers, Inkers and Colorists, otherwise "KCC" (1169–1178). Bookmarks become chapters, Summary becomes `dc:description`, and Series becomes `options.series` (1179–1184).
  - CBZ: the raw XML is kept only if `--keepcomicinfo` and the format is CBZ, with the comment "ComicInfo.xml in output may break readers like the Kobo native CBZ reader". The XML is then deleted from the work tree (1185–1189). For PDFs, title and author come from the PDF metadata (1191–1196).
- CBZ writing puts `ComicInfo.xml` into the image folder when `comicinfo_xml` is set (`comic2ebook.py` 2056–2067). `--keepcomicinfo` defaults to 0 (1521–1522) — [KCC repo](https://github.com/ciromattia/kcc)
- `ComicInfo.xml` is read from archives with `7z x -so <archive> ComicInfo.xml` (`comicarchive.py` 115–122) — [KCC repo](https://github.com/ciromattia/kcc)
- A maintainer said KCC "removes anything but image files from final output", that "putting a ComicInfo.xml file onto Kobos causes issues", and that "KCC transfers metadata to final KEPUB/EPUB/MOBI and deletes the comicinfo.xml" — [kcc#912](https://github.com/ciromattia/kcc/issues/912)
- A Mihon user reported their Kobo "cannot read the cbz proprely because of this XML file" (ComicInfo.xml) and had to unzip, remove it and re-zip. The maintainers declined to change Mihon — [mihon#664](https://github.com/mihonapp/mihon/issues/664)
- There is no Kobo-specific series field other than EPUB 3 `belongs-to-collection` and `group-position` (see the OPF section). KCC never writes `calibre:series` (grep of `comic2ebook.py`) — [KCC repo](https://github.com/ciromattia/kcc)

### Inferences

- The ComicInfo-breaks-Kobo-CBZ claim is anecdotal: one Mihon report and a vague maintainer recollection. Parallax's variant C (CBZ with ComicInfo, `Manga=YesAndRightToLeft`) is a direct test of it on firmware 4.38.
- Parallax already has better metadata plumbing in mind (series, number, year, author from its library). KCC's ComicInfo mapping is worth mirroring only as a fallback source.

### Gaps

- KCC ignores ComicInfo `Manga`, `Year`, `LanguageISO`, `Publisher`, `Genre` and `PageCount`. It does not use `Manga=YesAndRightToLeft` to switch RTL on automatically.

## RTL / manga mode

### Takeaway

`-m/--manga-style` (`righttoleft`) changes three things. Split spreads emit the right half first. The EPUB spine gets `page-progression-direction="rtl"` and the page-spread sides start on the right. Smart cover crop and panel-view order also flip. For CBZ, the README admits that RTL "only affects splitting order", because the CBZ reader decides the turn direction.

### Cited findings

- Help: "Manga style (right-to-left reading and splitting)" (`comic2ebook.py` 1489–1490) — [KCC repo](https://github.com/ciromattia/kcc)
- Spread detection and split (`image.py` `splitCheck`, 224–281):
  - Detection: a page counts as a spread when its orientation differs from the screen's and width/height > 1.16.
  - Split: in split modes (`--splitter` 0 = split, 2 = split + rotate), pages under a 1.8 aspect ratio are cut in half, right half first when `righttoleft` (257–268).
  - Rotate: in rotate mode, or for very wide pages (aspect ≥ 1.8, even in split mode), the whole spread is rotated 90° (−90° with `--rotateright`, or not at all with `--norotate`) and emitted as an `R` page (270–279).
  - Rotate-only special case: when a rotated spread fits the screen, it is just rotated (243–250).
  - Default: `--splitter` defaults to 0 (`comic2ebook.py` 1560–1561), with `--rotatefirst` to put the rotated version first (1543–1544). — [KCC repo](https://github.com/ciromattia/kcc)
- Spine direction and page-spread sides depend on `righttoleft` and `invertdirection` (`comic2ebook.py` 423–496) — [KCC repo](https://github.com/ciromattia/kcc)
- Webtoon mode forces `righttoleft=False` (`comic2ebook.py` 1690–1697) — [KCC repo](https://github.com/ciromattia/kcc)
- README FAQ: "RTL mode only affects splitting order for CBZ output. Your cbz reader itself sets the page turn direction." (README.md 179–180) — [KCC README](https://github.com/ciromattia/kcc#faq)
- Kobo supports the spine-level `page-progression-direction` attribute for right-to-left page flow, in EPUB 2 and EPUB 3 — [kobolabs/epub-spec](https://github.com/kobolabs/epub-spec)

### Inferences

- KCC's "split and rotate" (`--splitter 2`) is the "show both" option the Parallax spike doc mentions. KCC emits the halves and a rotated whole spread, not an unrotated whole spread scaled down. An unrotated whole-spread page shrunk to fit a portrait screen is only reachable with `--norotate`.

### Gaps

- None for KCC's own behaviour.

## Kobo community findings: ComicInfo, CBZ vs KEPUB, speed, RTL, series, bugs, limits

### Takeaway

Community evidence agrees that stock Kobo firmware doesn't take title, author or RTL from ComicInfo.xml in a sideloaded CBZ. CBZ shows the filename and "Unknown Author", and RTL for CBZ has to be set in the device database (Kobo Utilities plugin or SQL). KEPUB from KCC gets a proper title, author and ToC, and RTL through the OPF. Kobo's own spec says sideloaded `.kepub.epub` disables bookmarks and notes. Series grouping for sideloaded books generally requires calibre's Kobo driver or the NickelSeries mod. Kobo's spec recommends at most about 3.8 MP (about 1950×1950) per fixed-layout page image, at most 10 MB per HTML file and at most 1 GB per EPUB. No measured CBZ vs KEPUB page-turn benchmark exists.

### Cited findings

- **CBZ metadata: no.**
  - A December 2024 MobileRead thread: a Libra 2 user's CBZ showed "Unknown Author" despite editing ComicInfo with KCC's editor, ComicTagger and by hand. A KCC maintainer (axu2) said "cbz metadata support is not as easy as epub metadata support on kobo". Switching to KCC KEPUB gave the correct title, author and ToC — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=4482947)
  - Older MobileRead posts: Kobo devices "do not read the metadata" from CBZ, so the title is the filename and the author "Unknown Author". One 2017 post claimed newer firmware read some metadata. The calibre Kobo driver can populate series for CBZ, and subtitle only via the Kobo Utilities plugin — [MobileRead p=3580477](https://www.mobileread.com/forums/showthread.php?p=3580477), [MobileRead p=3585000](https://mobileread.com/forums/showthread.php?p=3585000). These conflict, and the 2017 claim is not corroborated by the 2024 report.
  - A gist describes editing `KoboReader.sqlite` to set title, author and `PageProgressDirection=rtl` for CBZ, noting there's otherwise no reading direction — [goncalor gist](https://gist.github.com/goncalor/aef56914c8a72faa3d056aadaeb5fab5)
- **CBZ RTL**: for CBZ/CBR the direction can't be set inside the file, only in the device database after import (Kobo Utilities). Once set, the page-turn buttons follow it — [MobileRead p=4023616](https://www.mobileread.com/forums/showthread.php?p=4023616) (search snippet; not fully fetched)
- **ComicInfo in CBZ may break Kobo CBZ reading**: one report — [mihon#664](https://github.com/mihonapp/mihon/issues/664). KCC's code comment repeats this (`comic2ebook.py` 1185).
- **KEPUB caveats**:
  - Kobo spec: "Sideloaded EPUBs with the '.fxl.kepub.epub' or '.kepub.epub' will disable bookmarking and note keeping", and cover thumbnails may not display. Leaving the file as `.epub` uses the ADE engine with bookmarks enabled — [kobolabs/epub-spec](https://github.com/kobolabs/epub-spec)
  - KCC issues #704 and #418 confirm bookmarks and annotations are disabled for KCC KEPUBs on Kobo (Libra Colour; firmware 4.32+). One commenter said kepubify didn't help, and one user downgraded to 4.31. Both are still open as of 2026 — [kcc#704](https://github.com/ciromattia/kcc/issues/704), [kcc#418](https://github.com/ciromattia/kcc/issues/418)
- **Exclamation-mark crash**: KEPUBs in a folder whose name contains `!` crash the Kobo reader, while a CBR in the same folder works. KCC strips special characters from KEPUB filenames (`KOBO_TOUCH_DRIVER_REGEX`), and commit `4e619fd` (2026-11-07) extends this to output folder names. README: "KEPUB crashes on Kobo? Remove any special characters like `!` from the filepath, including any intermediate folders." — [kcc#919](https://github.com/ciromattia/kcc/issues/919), [KCC README](https://github.com/ciromattia/kcc#faq)
- **Filenames**: users complained that Kobo profiles mangled filenames (spaces and hyphens removed). Maintainers found no evidence that spaces break Kobos. The older alnum-only rule has since become the narrower regex — [kcc#613](https://github.com/ciromattia/kcc/issues/613)
- **Series**:
  - NickelSeries "Adds native support for parsing series (and subtitle) metadata from EPUB files", works on EPUB and KEPUB, is compatible with calibre or EPUB 3 series metadata (calibre takes precedence), and "Will not override Nickel if Kobo ends up implementing it" — [NickelSeries](https://github.com/pgaskin/kobo-mods/tree/master/NickelSeries), [kepubify NickelSeries page](https://pgaskin.net/kepubify/ns/)
  - KCC added `belongs-to-collection` specifically so NickelSeries could group volumes — [kcc#1186](https://github.com/ciromattia/kcc/issues/1186)
  - A July 2024 thread reports that sideloaded books get `SeriesID` = series name while store books get a GUID, which can split one series in two — [MobileRead p=4482385](https://www.mobileread.com/forums/showthread.php?p=4482385) (search snippet)
  - Kobo's spec says "most fields in the metadata section of the OPF file are not read" for store ingestion — [kobolabs/epub-spec](https://github.com/kobolabs/epub-spec)
- **Spreads in landscape**: Kobo renders synthetic spreads from `rendition:spread`. KCC fixed misalignment on a Libra Colour — [kcc#930](https://github.com/ciromattia/kcc/issues/930)
- **Image and file limits** (Kobo publisher spec): at most 3,800,000 pixels per viewport (about 1950×1950) for a single-image FXL page; at most 10 MB of embedded content per HTML file; at most 1 GB per EPUB; PNG preferred where quality matters — [kobolabs/epub-spec](https://github.com/kobolabs/epub-spec)
- **Page-turn speed**:
  - An old test measured about 1.26 s per page for KEPUB vs 1.93–3.52 s for sideloaded EPUB (no CBZ) — [MobileRead p=3053906](https://www.mobileread.com/forums/showthread.php?p=3053906) (search snippet)
  - A July 2025 r/kobo guide recommends processing images for the device for "smaller file sizes, faster page turns", and prefers CBZ because reading headers and footers shrink images in EPUB/KEPUB — [r/kobo snapshot](https://reddit.sentinel-team.org/posts/1luhqei/snapshots/2025-12-16T21%3A41%3A47.471747Z)
  - No current head-to-head CBZ vs KEPUB benchmark was found.

### Inferences

- Parallax's spike already shows that page-turn speed is dominated by image size (1810×2560 JPEG ≈ 2 s; 1264×1680 4-bit PNG ≈ instant) and not by container. This fits Kobo's ~3.8 MP guideline: 1810×2560 is about 4.6 MP, above it, and 1264×1680 is about 2.1 MP.
- On metadata and RTL, the community evidence favours KEPUB/EPUB over CBZ on stock firmware. The cost is that a `.kepub.epub` loses bookmarks and notes, which are rarely needed for manga. A plain `.epub` fixed-layout book keeps bookmarks but uses the slower ADE engine. This is worth a device check.
- Series grouping on stock firmware is uncertain for any sideloaded format. Parallax manages the Kobo directly, so it could write `Series`, `SeriesNumber`, `SeriesID` and `PageProgressDirection` straight into `KoboReader.sqlite` after import, like calibre's driver and Kobo Utilities do. That would work for CBZ too. This is an inference from the gist and the plugin behaviour, not verified on firmware 4.38.

### Gaps

- No primary source confirms whether firmware 4.38 reads ComicInfo.xml or OPF series natively. Parallax's own checklist (variants A–D) is the best evidence available.
- No source quantifies a maximum CBZ size or page count for the Kobo CBZ reader. Only the EPUB limit (1 GB) is documented.
- Reddit threads were only reachable via mirrors or snapshots.

## Licensing: KCC, bundled tools, and CBR/RAR handling

### Takeaway

KCC is labelled ISC (`LICENSE.txt`, `__license__ = 'ISC'`, setup.py "ISC License (ISCL)"), but `image.py` carries a GPL-3.0-or-later header inherited from Mangle, and `dualmetafix.py` is credited as GPL-3. The effective licence of the whole is therefore ISC AND GPL-3.0-or-later. Both are compatible with porting into a GPL-3.0-only project. KCC bundles no archivers: it shells out to system `7z`/`7zz`, `tar`/`bsdtar`, `unar` (macOS) and `unrar` (Linux). KindleGen comes from Kindle Previewer. mozjpeg is used through the `mozjpeg-lossless-optimization` Python package.

### Cited findings

- `LICENSE.txt` begins "ISC LICENSE", Copyright 2012–2025 Ciro Mattia Gonano, 2013–2019 Paweł Jastrzębski, 2021–2023 Darodi. `setup.py` line 139 has `license='ISC License (ISCL)'`, and `__init__.py` has `__license__ = 'ISC'`. README.md 515: "KCC is released under ISC LICENSE" — [KCC repo](https://github.com/ciromattia/kcc)
- `image.py` lines 3–20: copyright Alex Yatskov (2010), Kosolapov (2011), Planas (2016), Gonano and Jastrzębski, with "GNU General Public License ... either version 3 of the License, or (at your option) any later version". README.md 485–486: `DualMetaFix` "Released with GPL-3 License", and `image.py` comes "from Alex Yatskov's Mangle". `metadata.py` and `comicarchive.py` carry ISC headers (`metadata.py` 1–17, `comicarchive.py` 1–18) — [KCC repo](https://github.com/ciromattia/kcc)
- The icon is CC BY-NC-SA 3.0 (README.md 487), so don't copy assets — [KCC README](https://github.com/ciromattia/kcc)
- Python deps (`requirements.txt`; `setup.py` 152–164): PySide6, Pillow, psutil, requests, python-slugify, mozjpeg-lossless-optimization, natsort, numpy, and pymupdf/pymupdf-lite — [KCC repo](https://github.com/ciromattia/kcc)
- Archive handling (`comicarchive.py` 30–135):
  - Type detection: `7z l` for the type, plus `unrar l` on Linux.
  - Extraction tries, in order: `unar` (macOS only), then `7z x`, then `tar -xf` (`bsdtar` on Linux), then `unrar x` (Linux only). All exclude `__MACOSX`, `.DS_Store` and `Thumbs.db`.
  - Adding files to RAR is refused.
  - README: CBZ/ZIP, CBR/RAR and CB7/7Z need the `7z` executable, and "7-Zip ... is optional but will make conversions much faster" (README.md 199–215). Issue #1233 added `bsdtar` because 7z failed on some CBRs on Linux ([kcc#1233](https://github.com/ciromattia/kcc/issues/1233)).
  - [KCC repo](https://github.com/ciromattia/kcc)
- KindleGen is detected from Kindle Previewer (README.md 189–197; `comic2ebook.py` 1760–1808) — [KCC repo](https://github.com/ciromattia/kcc)
- UnRAR source licence: freeware. It "may be used in any software to handle RAR archives without limitations free of charge, but cannot be used to develop RAR (WinRAR) compatible archiver and to re-create RAR compression algorithm" — [unrar.rs vendored license](https://github.com/muja/unrar.rs/blob/master/unrar_sys/vendor/unrar/license.txt)

### Inferences

- Porting KCC logic into GPL-3.0-only Rust is permitted. ISC is permissive, and GPL-3.0-or-later code may be relicensed as GPL-3.0-only in a derivative. Keep the copyright notices of the ported files (Mangle authors for `image.py`; Gonano, Jastrzębski and Darodi for the rest), plus the ISC notice for ISC-derived parts.
- The UnRAR restriction ("cannot be used to ... re-create RAR compression algorithm") is an extra restriction. It is widely regarded as GPL-incompatible (FSF lists the UnRAR licence as non-free; not fetched in this session). Linking `unrar`/`unrar_sys` into a GPL-3.0-only binary is a licence risk. Safer CBR routes: libarchive (BSD-2, with its own clean-room RAR reader) through `compress-tools`, or shelling out to a user-installed tool as KCC does.
- PyMuPDF is AGPL-3.0 (from general knowledge; not verified this session). It is irrelevant if PDF input is skipped.

### Gaps

- The exact libarchive RAR5 coverage and the macOS distribution story (static libarchive in a Tauri bundle) were not researched.

## Rust porting: crates, size of code worth porting, and what to skip

### Takeaway

The Kobo-relevant core of KCC is about 1,800–2,000 lines of Python: page parsing and processing (`image.py`), the crop algorithms, the EPUB/CBZ builder parts of `comic2ebook.py`, ComicInfo parsing, and archive extraction. That is roughly 2.5–4k lines of Rust. Parallax's spike already covers maybe a third: zip I/O, greyscale, a naive split, Lanczos resize, 16-level quantisation, fixed-layout OPF and kepubify. Every piece has a permissively licensed crate. The main exceptions are imagequant (GPL-3.0-or-later, which is fine for Parallax but unnecessary, since a fixed 16-grey palette needs no quantiser) and unrar (licence risk). The GUI (about 3.4k lines plus 25k of generated Qt resources), MOBI/KindleGen/DualMetaFix/KFX/panel view, PDF and the Kindle device helpers are not worth porting.

### Cited findings

- **Line counts** (`wc -l`, `kindlecomicconverter/`), from the KCC repo:

  | File | Lines | Contents |
  | --- | --- | --- |
  | `comic2ebook.py` | 2234 | orchestration, EPUB, CBZ, MOBI and PDF builders, CLI |
  | `image.py` | 727 | profiles, spread split, fill, crop, gamma, autocontrast, resize, quantise, save, cover |
  | `page_number_crop_alg.py` | 224 | |
  | `inter_panel_crop_alg.py` | 78 | |
  | `common_crop.py` | 28 | |
  | `rainbow_artifacts_eraser.py` | 249 | colour Kaleido only, FFT |
  | `metadata.py` | 126 | |
  | `comicarchive.py` | 135 | |
  | `shared.py` | 160 | |
  | `comic2panel.py` | 323 | webtoon panel splitter |
  | `dualmetafix.py` | 185 | MOBI only |
  | `kindle.py` | 48 | |
  | `pdfjpgextract.py` | 75 | |
  | GUI files: `KCC_gui.py` 2217, `KCC_ui.py` 952, `KCC_ui_editor.py` 199, `KCC_spread_label.py` 91, `KCC_rc.py` 25344 (generated) | | |

- **Processing pipeline per page** (`comic2ebook.py` `imgFileProcessing`, 744–778):
  1. Parse the page and split or rotate spreads (`ComicPageParser`).
  2. `gammaCorrectImage`.
  3. Convert to greyscale (unless colour).
  4. `autocontrastImage`.
  5. `resizeImage` (Lanczos; `image.py` 591–635).
  6. `optimizeForDisplay` (rainbow eraser, opt-in).
  7. With `--forcepng`, `quantizeImage` to the profile palette (Pillow `quantize(palette=Palette16)`, `image.py` 577–584).
  8. `saveToDir`: JPEG q85 by default; PNG, WebP or mozjpeg on request; tall pages split above and below 1920 px.

  Cropping defaults to mode 2, margins plus page numbers (`comic2ebook.py` 1572–1573). — [KCC repo](https://github.com/ciromattia/kcc)
- **Crate facts** (crates.io API, 2026-11-08):

  | Crate | Version | Licence | Role |
  | --- | --- | --- | --- |
  | [`image`](https://crates.io/crates/image) | 0.25.10 | MIT OR Apache-2.0 | decode/encode, greyscale, crop, rotate, basic resize |
  | [`fast_image_resize`](https://crates.io/crates/fast_image_resize) | 6.1.0 | MIT OR Apache-2.0 | SIMD Lanczos3 |
  | [`zip`](https://crates.io/crates/zip) | 9.4.2 | MIT | CBZ/EPUB read and write, STORED mimetype first |
  | [`compress-tools`](https://crates.io/crates/compress-tools) | 0.16.1 | MIT OR Apache-2.0 | libarchive bindings: RAR/7z/tar read |
  | [`unrar`](https://crates.io/crates/unrar) | 0.5.8 | MIT OR Apache-2.0 wrapper; vendored UnRAR under its own freeware licence | RAR read |
  | [`sevenz-rust2`](https://crates.io/crates/sevenz-rust2) | 0.25.0 | Apache-2.0 | pure-Rust 7z, maintained, updated 2026-11-06 |
  | [`sevenz-rust`](https://crates.io/crates/sevenz-rust) | 0.6.1 | Apache-2.0 | last updated 2024-07, superseded |
  | [`quick-xml`](https://crates.io/crates/quick-xml) | 0.42.0 | MIT | XML reading and writing |
  | [`roxmltree`](https://crates.io/crates/roxmltree) | 0.21.2 | MIT OR Apache-2.0 | read-only ComicInfo parsing |
  | [`mozjpeg`](https://crates.io/crates/mozjpeg) / `mozjpeg-sys` | 0.10.13 / 2.2.3 | IJG AND Zlib AND BSD-3-Clause | builds C mozjpeg statically |
  | [`jpegli`](https://crates.io/crates/jpegli) | 0.1.0 | BSD-3-Clause | immature, about 8.7k downloads |
  | [`oxipng`](https://crates.io/crates/oxipng) | 10.2.1 | MIT | lossless PNG optimiser |
  | [`imagequant`](https://crates.io/crates/imagequant) | 4.4.1 | GPL-3.0-or-later | palette quantiser |
  | [`rayon`](https://crates.io/crates/rayon) | 1.12.0 | MIT OR Apache-2.0 | |
  | [`natord`](https://crates.io/crates/natord) | 1.0.9 | MIT | last release 2015 |
  | [`epub-builder`](https://crates.io/crates/epub-builder) | 0.8.3 | MPL-2.0 | |
  | [`libarchive`](https://crates.io/crates/libarchive) | 0.1.1 | Apache-2.0 | abandoned, 2016 |

  The `unrar` README says the vendored C/C++ library "uses its own license" — [unrar.rs](https://github.com/muja/unrar.rs)

### Inferences

- **Worth porting**, in rough effort order:
  1. Spread detection and split/rotate with RTL order (`splitCheck`, about 60 lines): about 0.5 day.
  2. Page-spread side assignment and spread alignment for the spine (`buildOPF` 441–501, about 60 lines): about 0.5 day, worth it if `rendition:spread landscape` is adopted.
  3. Margin and page-number cropping (`image.py` `cropMargin`/`cropPageNumber`/`fillCheck` plus `page_number_crop_alg.py` and `common_crop.py`, about 350 lines with numpy): 2–4 days. Numpy array ops map to plain loops over `GrayImage` or `ndarray`, and this needs fixture tests.
  4. Gamma and autocontrast/autolevel (about 50 lines): about 0.5 day.
  5. ComicInfo read/write (about 130 lines): about 0.5 day with `quick-xml`.
  6. Cover handling and smart cover crop (about 90 lines): about 0.5 day.
  7. Chapter nav from folders and ComicInfo bookmarks, NCX and nav (about 120 lines): about 1 day.
  8. Archive input: zip natively, and CBR/CB7 via `compress-tools` (libarchive) or `sevenz-rust2`: 1–2 days, including packaging libarchive for macOS.

  Total about 7–11 developer-days on top of the spike, plus device testing. Low confidence; based on line counts, not a design.
- **Not worth porting**: the Qt GUI; MOBI/AZW3 (KindleGen subprocess and `dualmetafix.py`); KFX; Kindle panel view and `region-mag`; Kindle-specific OPF metas; `kindle.py` (Kindle cover thumbnails); PDF input and output (PyMuPDF, AGPL); `rainbow_artifacts_eraser.py` (Kaleido colour only; revisit for colour Kobos); webtoon `comic2panel.py` (unless webtoons matter); 200 MB tome splitting (a Kindle email limit; Kobo's documented limit is 1 GB per EPUB); and the 7-Zip `!mimetype` rename hack (the `zip` crate writes `mimetype` first natively).
- **Quantisation**: KCC maps to a fixed 16-grey palette with Pillow `quantize(palette=...)`, with Floyd–Steinberg dithering by default in Pillow (Pillow default; not verified in KCC). A fixed-palette mapping with optional error diffusion is about 40 lines of Rust and needs no `imagequant`. That avoids a GPL-3.0-or-later dependency, although it would be licence-compatible with Parallax anyway.
- **Container choice**: Parallax should keep its kepubify step rather than copy KCC's rename-only KEPUB. Kepubify adds Kobo spans, and Parallax already bundles it. Whether spans change anything for an image-only fixed-layout book is unverified.

### Gaps

- No Rust crate replicates Pillow's exact `ImageOps.autocontrast` and `quantize` semantics. Output parity with KCC would need golden-image tests.
- I didn't benchmark `fast_image_resize` against `image::imageops` for this workload.
