# KCC (Kindle Comic Converter) image processing pipeline: code-level description for a Rust port

Source snapshot: `git clone --depth 50 https://github.com/ciromattia/kcc`, HEAD `5b7f803` "bump 13.0.0" (commit date 2026-11-07). Latest release is **v13.0.0, published 2026-11-08T03:29Z** ([release](https://github.com/ciromattia/kcc/releases/tag/v13.0.0)); its notes list only GUI/settings/Windows packaging changes (PRs #1498–#1512), no image-pipeline changes. All file paths below are relative to `kindlecomicconverter/` in that snapshot, which is browsable at [github.com/ciromattia/kcc/tree/v13.0.0/kindlecomicconverter](https://github.com/ciromattia/kcc/tree/v13.0.0/kindlecomicconverter). Line numbers refer to that tag. Nothing was run, built or installed; code was only read.

Abbreviation: KCC = Kindle Comic Converter. "Pillow" = Python Imaging Library fork that KCC uses for all raster operations.

## 1. Device profiles (resolutions, palettes, Kobo models, colour handling)

### Takeaway

Every profile is a tuple `(name, (width, height), palette, gamma)`; all modern profiles (every Kobo) use the 16-level grey palette 0x00,0x11,…,0xff and gamma 1.0, so "auto gamma" is a no-op today. Colour Kobos (Clara Colour `KoCC`, Libra Colour `KoLC`) have the *same* profile data as their greyscale siblings; colour is purely a GUI default that ticks `--forcecolor`, and the CLI outputs greyscale for them unless `--forcecolor` is passed.

### Cited Findings

- Profile data structure and palettes are defined in `image.py` lines 37–171, class `ProfileData` — [image.py](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L37-L171).
  - `Palette4` = 0x00, 0x55, 0xaa, 0xff (lines 41–46); `Palette15` = 0x00…0xdd step 0x11 then 0xff (**0xee is missing**, lines 48–64; used only by Kindle 2); `Palette16` = 0x00, 0x11, …, 0xee, 0xff, i.e. 16 evenly spaced grey levels (lines 66–83). Each entry is an RGB triple with equal components — [image.py#L41-L83](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L41-L83).
- Kobo profiles (`ProfilesKobo`, lines 121–137), all `Palette16`, gamma 1.0 — [image.py#L121-L137](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L121-L137):

  | Key | Name in code | Resolution (w×h) |
  |---|---|---|
  | KoMT | Kobo Mini/Touch | 600×800 |
  | KoG | Kobo Glo | 768×1024 |
  | KoGHD | Kobo Glo HD | 1072×1448 |
  | KoA | Kobo Aura | 758×1024 |
  | KoAHD | Kobo Aura HD | 1080×1440 |
  | KoAH2O | Kobo Aura H2O | 1080×1430 |
  | KoAO | Kobo Aura ONE | 1404×1872 |
  | KoN | Kobo Nia | 758×1024 |
  | KoC | Kobo Clara HD / Clara 2E / Clara BW | 1072×1448 |
  | KoCC | Kobo Clara Colour | 1072×1448 |
  | KoL | Kobo Libra H2O / Libra 2 | 1264×1680 |
  | KoLC | Kobo Libra Colour | 1264×1680 |
  | KoF | Kobo Forma | 1440×1920 |
  | KoS | Kobo Sage | 1440×1920 |
  | KoE | Kobo Elipsa | 1404×1872 |

  There is no separate Elipsa 2E profile in the code (Elipsa is used for it by implication, not stated).
- Kindle profiles (`ProfilesKindlePDOC`, lines 91–114): K1 600×670 Palette4; K2 600×670 Palette15; KDX 824×1000; K34, K57, K810 600×800; KPW 758×1024; KV, KPW34, K11 1072×1448; KO 1264×1680; KPW5 1236×1648; KPW6 1272×1696; K1920 1440×1920; KS1860 1860×1920; KS1920 1920×1920; KS1240 1240×1860; KS1324 1324×1986; KS (Scribe 1/2) 1860×2480; KCS (Colorsoft) 1272×1696; KS3 and KSCS (Scribe 3 / Scribe Colorsoft) 1986×2648. All except K1/K2 use Palette16 — [image.py#L91-L114](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L91-L114).
- reMarkable profiles: Rmk1, Rmk2 1404×1872; RmkPP 1620×2160; RmkPPMove 954×1696 (lines 139–144). Generic fixed-resolution profiles named `WxH` for 1072×1448, 1264×1680, 1404×1872, 1536×2048, 1650×2200, 1860×2480, 2400×3200 (lines 146–163); `OTHER` (0,0) (line 169) — [image.py#L139-L171](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L139-L171).
- `--customwidth/--customheight` create a "Custom" profile with Palette16 and the base profile's gamma — [comic2ebook.py#L1706-L1716](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1706-L1716).
- Any profile not in `ProfilesKindle` is treated as Kobo (`options.isKobo = True`), and Panel View and `--hq` are force-disabled when `'Ko' in profile` — [comic2ebook.py#L1629-L1633, #L1690-L1696](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1622-L1700).
- GUI per-profile defaults (`KCC_gui.py` lines 1642–1688): every Kobo profile has `DefaultFormat: 1` (EPUB, which becomes KEPUB), `PVOptions: False`. `DefaultUpscale` is True for Aura HD, Aura H2O, Aura ONE, Clara HD, Clara 2E/BW, Clara Colour, Libra H2O, Libra 2, Libra Colour, Forma, Sage, Elipsa and Nia, and False for Mini/Touch, Glo, Glo HD and Aura. `ForceColor: True` only for "Kobo Clara Colour" (KoCC) and "Kobo Libra Colour" (KoLC) — [KCC_gui.py#L1642-L1688](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L1642-L1688).
- On device change the GUI sets the colour box from `ForceColor`, and unticks the rainbow eraser unless the profile is KCS, KSCS, KoLC or KoCC — [KCC_gui.py#L1200-L1214](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L1200-L1214); v13.0.0 note "uncheck rainbow eraser when selecting BW device" (#1500) — [release](https://github.com/ciromattia/kcc/releases/tag/v13.0.0).
- Reported in 2024: with CLI `--profile KoLC`, output is greyscale; the maintainer's answer is to pass the force-colour argument — [issue #710](https://github.com/ciromattia/kcc/issues/710) (open).
- README states that choosing a Kobo profile with EPUB output produces KEPUB — [README](https://github.com/ciromattia/kcc#faq).

### Inferences

- For the Libra 2 a port needs only (1264, 1680), Palette16 and gamma 1.0. A "Kaleido" path is: keep RGB (`forcecolor`), skip quantisation (colour output never goes through `quantizeImage`), save JPEG, and optionally apply the FFT rainbow eraser.
- Because every gamma is 1.0, "Auto" gamma does nothing in v13; older KCC versions used non-1.0 profile gammas (git history not checked because of the depth-50 clone).

### Gaps

- No resolution is given for Kobo Elipsa 2E, Clara BW vs 2E differences, or the Libra Colour's colour-layer resolution (Kaleido 3 colour is lower resolution than the mono layer); KCC does not model this.

## 2. Pipeline order in the code (extraction to encoding)

### Takeaway

Order: extract → (EPUB spine reorder / PDF render) → metadata → remove non-images → sanity/size check → natural-sort rename → (optional spread pre-join) → cover → (webtoon panel split) → parallel per-file processing. Per file, `ComicPageParser` handles colour check, crop and spread split. Then, for each resulting page, `ComicPage` does colour/grey conversion, gamma, greyscale, autocontrast (with optional autolevel), resize, rainbow erase, optional quantise (PNG only) and save.

### Cited Findings

**A. Book-level steps (`comic2ebook.makeBook`, lines 1882–2060)** — [comic2ebook.py#L1882-L2060](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1882-L2060)

1. **Workspace and extraction** in `getWorkFolder` (lines 925–1072). It refuses if free disk space is under 2.5× the source size. Folders are copied (`copytree`). Archives go through `comicarchive.ComicArchive.extract`, which tries external tools in order: `tar` (`bsdtar` on Linux), `7zz` (macOS) or `7z`, then `unar`, then `unrar`. `__MACOSX`, `.DS_Store` and `thumbs.db` are excluded — [comicarchive.py#L31-L32, #L66-L95](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comicarchive.py#L31-L95). If the archive has a single top-level folder, its contents are moved up a level (lines ~991–1003). For EPUB input, pages are reordered by the OPF spine, taking the largest image (by file size) referenced per XHTML page (lines ~1008–1066). PDFs are rendered in parallel with PyMuPDF at the device size × 1.2 (cropping=1) or × 1.25 (cropping=2) to leave room for later cropping (lines ~963–975) — [comic2ebook.py#L925-L1072](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L925-L1072).
2. `getMetadata` reads ComicInfo.xml, then **`removeNonImages`** (lines 1219–1243) deletes any file whose extension is not in `IMAGE_TYPES` = .png .jpg .jpeg .gif .webp .jp2 .avif ([shared.py#L30](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/shared.py#L30)) and removes empty folders — [comic2ebook.py#L1219-L1243](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1219-L1243).
3. **`detectSuboptimalProcessing`** (lines 1380–1424) raises an error on zero-byte or unopenable images and warns if filenames contain `-kcc` (already processed). It also warns if more than 25% of images are smaller than the device in *both* dimensions while upscale/stretch are off. It does **not** detect duplicate or blank pages — [comic2ebook.py#L1380-L1424](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1380-L1424).
4. **Sorting/renaming: `sanitizeTree`** (lines 1246–1280) sorts files with natsort's `os_sort_keygen` (OS-style natural sort, same as Finder/Explorer ordering; imported at line 42) and renames them `kcc-0001.ext`, `kcc-0002.ext`… The counter is global across folders, with a 9999-page limit comment. Directories are also natural-sorted and slugified into chapter names. The first renamed file becomes the cover path — [comic2ebook.py#L42, #L1246-L1280](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1246-L1280). `imgDirectoryProcessing` also iterates `os_sorted(filenames)` (line 700).
5. **Optional spread pre-join**: if `<source>.json` exists with a `spreads` list (written by the GUI spread labeller, `KCC_spread_label.py`), pages i and i+1 are pasted side by side into `-merged.png`, in reversed order for manga — [comic2ebook.py#L1932-L1955](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1932-L1955); described by the maintainer as "KCC 11 adds a way to pre-join spreads that are pre-split" — [issue #594](https://github.com/ciromattia/kcc/issues/594).
6. **Cover**: `image.Cover` (image.py lines 638–719). It converts to RGB, applies `autocontrast(preserve_tone=True)`, converts to L unless forcecolor, optionally runs smart cover crop for wide images (ratio bands >2, >1.83, >1.7, >1.34, >1.0 with fixed crop fractions), then `thumbnail` (Lanczos, fit inside) or `fit` (`--coverfill`) to the profile size. It is saved as JPEG at `jpegquality` — [image.py#L638-L719](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L638-L719).
7. **Webtoon**: `comic2panel.main` with the profile's x/y, in-place and merge mode (see section 6) — [comic2ebook.py#L1986-L1988](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1986-L1988).
8. **`imgDirectoryProcessing`** (lines 691–727) runs one `imgFileProcessing` task per source file in a `multiprocessing.Pool`. Only the first file gets `is_first_page=True` — [comic2ebook.py#L691-L727](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L691-L727).

**B. Per-file steps (`imgFileProcessing`, comic2ebook.py lines 744–777)** — [comic2ebook.py#L744-L777](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L744-L777)

```text
workImg = ComicPageParser((dirpath, afile), is_first_page, opt)   # load, bg detect, crop, split
for each payload page:
    img = ComicPage(opt, *page)          # colour check -> convert RGB or L
    img.gammaCorrectImage()
    if not img.colorOutput: img.convertToGrayscale()
    img.autocontrastImage()              # (+ autolevel if enabled)
    img.resizeImage()
    img.optimizeForDisplay(opt.eraserainbow, img.colorOutput)
    if colorOutput: pass
    elif opt.forcepng:
        if not noquantize: img.quantizeImage()
        if PDF, or KF7 Kindle + CBZ, or pnglegacy: convertToGrayscale()   # back to 8-bit L
    img.saveToDir()
```

**C. `ComicPageParser.__init__` (image.py lines 174–213)** — [image.py#L174-L213](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L174-L213)

- Opens the image and copies it. `ImageFile.LOAD_TRUNCATED_IMAGES = True` (line 34). 16-bit `I;16` images are divided by 256 down to uint8 (lines 189–191).
- Background detection: `fillCheck()` (lines 283–319), see section 3. `--blackborders`/`--whiteborders` override only the *padding* fill, not the detected background used for cropping (lines 195–198).
- If it is the first page **and** colour, all cropping is skipped (lines 203–204). Otherwise: cropping==2 → `cropPageNumber`; cropping==1 → `cropMargin` (both skipped in webtoon mode); interpanelcrop>0 → `cropInterPanelEmptySections` (lines 206–211).
- Then `splitCheck()` (lines 224–281), see section 4.

**D. `ComicPage` (image.py lines 444–636)**

- `__init__`: `colorCheck` decides colour. Colour pages become RGB, others `L`. `colorOutput = color and opt.forcecolor`. `--hq` multiplies the target size by 1.5 (Kindle Panel View only; forced off for Kobo). Output filename suffix: `-kcc-x` normal, `-kcc-b` split part 1, `-kcc-c` split part 2, `-kcc-d` rotated spread (or `-kcc-a` with `--rotatefirst`) — [image.py#L444-L475](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L444-L475).
- Gamma (lines 527–536): if `opt.gamma < 0.1` (the default 0.0 means Auto), use the profile gamma, forced to 1.0 for colour pages. If gamma ≠ 1.0, apply the per-channel LUT `out = int(255 * (in/255) ** gamma)` — [image.py#L527-L536](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L527-L536).
- Greyscale: `image.convert('L')` (line 575), i.e. Pillow's ITU-R 601-2 luma L = R·299/1000 + G·587/1000 + B·114/1000 (Pillow-documented behaviour; not restated in KCC).
- Autocontrast/autolevel (lines 538–572), see section 5.
- Resize (lines 591–635), see section 5.
- Rainbow eraser (lines 586–589 → `rainbow_artifacts_eraser.py`), see section 5.
- Quantise (lines 577–584): `convert("RGB")`, then `quantize(palette=palImg)`, where `palImg` is a 1×1 `P` image holding the profile palette (48 bytes for Palette16). No `dither` argument is passed, so Pillow's default `Dither.FLOYDSTEINBERG` applies: `Image.quantize(colors=256, method=None, kmeans=0, palette=None, dither=Dither.FLOYDSTEINBERG)`, and with a palette the code calls `self.im.convert("P", dither, palette.im)` — [image.py#L577-L584](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L577-L584); [Pillow Image.py](https://github.com/python-pillow/Pillow/blob/main/src/PIL/Image.py) (lines ~1288–1355 on main). README confirms output is "dithered 4-bit PNG" — [README](https://github.com/ciromattia/kcc#readme).
- Save (`save_with_codec`, lines 498–525) — [image.py#L498-L525](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L498-L525):
  - PNG branch when `forcepng and (not colorOutput or force_png_rgb)`: drop transparency; `--webp` gives lossless WebP; Kindle AZW3 gives GIF; otherwise `PNG, optimize=1`.
  - Else JPEG/WebP: lossy WebP at `jpegquality`; `--mozjpeg` encodes with Pillow `JPEG optimize=1 quality=q`, then runs `mozjpeg_lossless_optimization.optimize()` on the bytes (lossless re-optimisation, not a mozjpeg encode); otherwise Pillow `JPEG optimize=1 quality=q`.
  - Default quality is 85, or 90 for profiles starting `KS` and `KCS` — [comic2ebook.py#L1718-L1722](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1718-L1722). No `subsampling` is set, so libjpeg's default applies ([Pillow docs](https://pillow.readthedocs.io/en/stable/handbook/image-file-formats.html)).
  - The source file is deleted after saving (lines 492–493).
- PNG bit depth: Pillow's PNG writer sets `colors = min(1 << bits(default 8), len(palette)//3, 256)`. If `colors <= 16` it writes 1, 2 or 4 bits, so a 16-entry palette gives a **4-bit indexed PNG** with a 16-entry PLTE — [Pillow PngImagePlugin.py](https://github.com/python-pillow/Pillow/blob/main/src/PIL/PngImagePlugin.py) (lines ~1406–1425, 1485–1487 on main). `optimize` only raises zlib effort (compress_level 9) — [Pillow file formats](https://pillow.readthedocs.io/en/stable/handbook/image-file-formats.html). `--pnglegacy` converts back to 8-bit `L` after quantising (values still restricted to the 16 levels) — [comic2ebook.py#L770-L775](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L744-L777).

### Inferences

- For a Rust port: the file-level order is fixed and a page's crop happens **before** the spread split. Each half of a split spread is then gamma'd, autocontrasted and resized independently (each half gets its own autocontrast LUT).
- The cover is processed twice: once as `Cover` (a separate JPEG) and once as a normal page through the main loop (first file). This is inferred from `sanitizeTree` returning the first file as `cover_path` while the file stays in the image directory; the EPUB build code was not checked line by line.
- CLI default output (no `--forcepng`) is greyscale 8-bit JPEG q85 with no palette quantisation. Quantisation/dithering happens only on the PNG path.

### Gaps

- Exact EPUB/KEPUB XHTML wrapping and viewport (`buildHTML`/`buildOPF`, lines 108–517) were not analysed; they are outside image processing.

## 3. Cropping, background detection, duplicate/blank handling

### Takeaway

Cropping is threshold-based bounding-box detection on an autocontrasted, box-blurred greyscale copy. Threshold = 240 − 64·power (176 at default power 1.0). Edge noise is ignored, and a page-number heuristic examines the bottom 2.5% of the content box. Each side is cropped by at most 10%, `--preservemargin` backs the crop off by a percentage, and `--croppingminimum` vetoes crops that leave too little area. There is no duplicate or blank page detection.

### Cited Findings

- **Background (fill) detection `fillCheck`** (image.py lines 283–319): binarise at 128 (`L`, `<128 → 0`). Compute the bbox of the non-black pixels (A) and of the inverted image (B), take their areas, and `diff = (max−min)/min·100`. If diff > 0.5%: the smaller white-region bbox means 'white', otherwise 'black'. Else, scan 5-px horizontal strips and 5-px vertical strips. Each strip scores −1 if it has no pure-black pixels, +1 if it has no pure-white pixels, else 0. A positive sum means 'black', otherwise 'white' — [image.py#L283-L319](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L215-L319). (The code literally returns 'white' when `surfaceW < surfaceB`.)
- **Threshold**: `threshold_from_power(power) = 240 − power·64` — [common_crop.py#L1-L2](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/common_crop.py#L1-L2). Docstring says power is in [0, 3] — [page_number_crop_alg.py#L31-L37](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/page_number_crop_alg.py#L31-L37).
- **Margin crop `get_bbox_crop_margin`** (page_number_crop_alg.py lines 128–151) — [page_number_crop_alg.py#L128-L151](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/page_number_crop_alg.py#L128-L151):
  1. greyscale; invert if background ≠ white;
  2. `ImageOps.autocontrast(img, cutoff=1)` (1% clipped each end) then `BoxBlur(1)`;
  3. binarise: pixel ≤ threshold → 255 (content), else 0;
  4. `ignore_pixels_near_edge` (see below);
  5. `getbbox()` of content.
- **`ignore_pixels_near_edge`** (lines 153–185): returns immediately if the image is too small (`int(0.02h) == int(0.025h)` or the same for w). For each side it checks an "inner" band (2%–2.5% from that edge). If that band's content fraction is >0 and <0.001 it is zeroed. If it is <0.001, the outer 2% edge band is also zeroed when it has any content. This removes scanner specks and edge lines that would block cropping — [page_number_crop_alg.py#L153-L185](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/page_number_crop_alg.py#L153-L185). A division-by-zero bug here on a corrupted (zero-height) image was reported in Jan 2026 — [issue #1219](https://github.com/ciromattia/kcc/issues/1219).
- **Page number crop `get_bbox_crop_margin_page_number`** (lines 38–117) — [page_number_crop_alg.py#L15-L117](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/page_number_crop_alg.py#L15-L117):
  - Constants: max page-number size (4.5% w, 2% h), min size (0.3% w, 0.6% h), window height = 2%·1.25 = 2.5% of image height, max inter-digit distance (1% w, 0.2% h) (lines 15–18).
  - Same preprocessing as the margin crop, then take the content bbox and crop a window of height 2.5% h at the bottom of that bbox.
  - For each row of the window, group content x-positions (≤ threshold) whose gaps are ≤ 1% w (`group_close_values`, [common_crop.py#L8-L28](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/common_crop.py#L8-L28)). These become row boxes, which `merge_boxes` (lines 198–224) iteratively merges when within the (1% w, 0.2% h) Chebyshev distance.
  - Drop boxes smaller than the minimum size. "Lowest boxes" are those touching the window's last row. Keep boxes whose bottom is ≥ the minimum top of the lowest boxes.
  - If exactly one such box remains and it is ≤ max size (w ≤ 4.5%, h ≤ max(2%·h, 3 px)), cut the bottom to just above that box and recompute the content bbox on the remaining region. Otherwise fall back to the plain content bbox.
- **Clamp and apply** (image.py lines 321–350): the bbox is clamped so at most 10% is removed per side: `(min(0.1w, left), min(0.1h, top), max(0.9w, right), max(0.9h, bottom))`. `--preservemargin P`: `ratio = 1 − P/100`; left and top are multiplied by ratio; right and bottom move outward by `(w−right)·P/100`. The crop is applied only if `box_area / image_area ≥ croppingminimum` (default 0.0) — [image.py#L321-L350](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L321-L350).
- CLI defaults: `--cropping 2` (margins + page numbers), `--croppingpower 1.0`, `--preservemargin 0`, `--croppingminimum 0.0`, `--interpanelcrop 0` — [comic2ebook.py#L1584-L1593](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1474-L1619). GUI default options: `croppingBox: 2` (Checked → cropping 2), `croppingPowerSlider: 100` (power 1.0) — [KCC_gui.py#L1534, #L320-L329, #L1551](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L1534).
- **Inter-panel crop** (`inter_panel_crop_alg.py`): same binarisation at power 1. Rows (and optionally columns) whose max is 0 are found and grouped into contiguous runs. Runs not within 1% of the borders are removed except for `keep=4%` of each run (2% kept at each end). The rows/columns are deleted with `np.delete` — [inter_panel_crop_alg.py#L20-L75](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/inter_panel_crop_alg.py#L20-L75). Note: the border check uses `img.size[1]` (height) for both axes, so the vertical (column) variant compares column indices against height (lines 66–67). Diagonal gutters are not supported — [issue #958](https://github.com/ciromattia/kcc/issues/958).
- **No duplicate/blank page handling**: no code path in `comic2ebook.py` or `image.py` hashes, compares or drops pages. The only removals are non-image files, zero-byte files (which raise an error) and dotfiles — [comic2ebook.py#L1219-L1243, #L1380-L1424](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1380-L1424).

### Inferences

- The `L`-mode greyscale before thresholding means coloured margins are judged by luma only.
- Because the crop runs before the split, a spread is cropped as a whole. The *halves* can still end up misaligned in landscape when whitespace is uneven, which was the complaint in issue #594 (the maintainer's workaround was rotate+split, or the pre-join tool) — [issue #594](https://github.com/ciromattia/kcc/issues/594).

### Gaps

- No published accuracy figures for the page-number heuristic.

## 4. Spread detection, splitting, rotation (`--splitter`, manga order)

### Takeaway

A page counts as a spread when its orientation differs from the device (landscape image on a portrait device) and width/height > 1.16. Mode 0 (CLI default, "Split") cuts at exactly width/2. For manga (`-m`) the right half comes first. Spreads with w/h ≥ 1.8 are rotated instead of split. Mode 1 rotates only; mode 2 emits both halves and then the rotated whole. The GUI's default is mode 2 (split + rotate).

### Cited Findings

- `splitCheck` (image.py lines 224–281) — [image.py#L224-L281](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L224-L281):
  - `--maximizestrips` (1×4 → 2×2): halves are stacked vertically, right half on top for manga (lines 227–239).
  - Webtoon: never split (lines 240–241).
  - Rotate-only special case: orientation differs, the image fits the device when rotated (`w ≤ dst_h and h ≤ dst_w`) and splitter == 1 → rotate (lines 243–250; the code comments "TODO dead code?").
  - Main test: `(w > h) != (dst_w > dst_h) and w/h > 1.16` (line 252).
  - `BISECT_THRESHOLD = 1.8` (line 254). If splitter ≠ 1 and w/h < 1.8: crop boxes `(0,0,w/2,h)` and `(w/2,0,w,h)` (integer division of the float). The `righttoleft` flag (set by `-m/--manga-style`) puts the right box first. Payload entries are `S1` and `S2` (lines 255–269).
  - If splitter > 0, or splitter == 0 and w/h ≥ 1.8: append a rotated copy `R`. The rotation is `rotate(90, BICUBIC, expand=True)`, which is counter-clockwise in Pillow, or −90 with `--rotateright`. It is not rotated at all with `--norotate` (lines 272–279).
- Ordering in the book comes from filename suffix sort: `-kcc-b` (S1) < `-kcc-c` (S2) < `-kcc-d` (rotated), or `-kcc-a` (rotated first with `--rotatefirst`) — [image.py#L463-L472](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L463-L472).
- `--norotate` spreads in mode 1/2 are resized to fit (2·device_w, device_h), so they get double resolution for zooming. With Kindle AZW3 they are capped at 1920 — [image.py#L594-L600](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L591-L600). Maintainer: "in 9.3.3, non rotated spreads are double resolution, so zooming is sharp" — [issue #594](https://github.com/ciromattia/kcc/issues/594).
- CLI: `-r/--splitter` default 0 ("0: Split 1: Rotate 2: Both") — [comic2ebook.py#L1557-L1558](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1474-L1619). The GUI maps rotateBox PartiallyChecked → splitter 2 and Checked → 1 ([KCC_gui.py#L294-L297](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L294-L297)), and its first-run defaults are `rotateBox: 1` (PartiallyChecked → **splitter 2**) and `mangaBox: 2` (manga **on**) — [KCC_gui.py#L1534](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L1534).
- Related features: `--spreadshift` and `--onepagelandscape` affect EPUB layout, not pixels (parser lines 1532–1535). Issue #807 asked "Don't split spreads if another spread immediately after" (closed) — [issue #807](https://github.com/ciromattia/kcc/issues/807).

### Inferences

- The detection is purely aspect-based (no gutter or content analysis). Landscape single illustrations get split too; very wide (≥1.8) images are rotated rather than split.
- Parallax's spike behaviour (split, right half first) matches KCC `-m --splitter 0`. KCC's GUI default (mode 2) addresses the "never seen whole" trade-off by also appending the rotated full spread after the halves.

### Gaps

- No bug reports specifically about wrong spread *detection* were found; the issues found concern alignment after cropping (#594).

## 5. Resize, gamma, autocontrast/autolevel, colour detection, greyscale, rainbow eraser

### Takeaway

Downscaling uses Lanczos and upscaling uses bicubic (only with `--upscale`). Images within 1.5% of the device aspect ratio are cropped to fill (`ImageOps.fit`). Otherwise KEPUB/EPUB uses `ImageOps.contain` (no padding) and CBZ/PDF pads with the detected background colour. Autocontrast is Pillow's min/max linear stretch with `preserve_tone`, skipped for low-contrast pages. Colour detection uses Cb/Cr histogram spreads at three cutoff levels.

### Cited Findings

- **Resize method** (image.py lines 631–635): BICUBIC if the image is ≤ the device size in both dimensions, else LANCZOS — [image.py#L631-L635](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L631-L635).
- **resizeImage** (lines 591–629) — [image.py#L591-L629](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L591-L629):
  - `--wallpaper`: no resize at that point (line 592).
  - KFX: pad or fit to a book-wide common resolution (lines 605–614).
  - `--stretch`: `resize(size)`, aspect ignored (lines 615–616).
  - If the method is BICUBIC (small image) and not `--upscale`: **no resize** (lines 619–620).
  - Else, `AUTO_CROP_THRESHOLD = 0.015` (line 33). If |h/w(image) − h/w(device)| < 0.015 (× 3 for KDX), use `ImageOps.fit` (scale to cover and centre-crop). Else, for CBZ/PDF without `--whiteborders`, use `ImageOps.pad` with the fill colour. Else `ImageOps.contain` (fit inside, keep aspect) — lines 622–629.
  - `shared.get_contain_resolution` mirrors Pillow's `contain` rounding: `new_h = round(h/w · W)` or `new_w = round(w/h · H)` — [shared.py#L64-L79](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/shared.py#L64-L79).
- CLI defaults: `--upscale` off, `--stretch` off — [comic2ebook.py#L1553-L1556](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1474-L1619). GUI upscaleBox PartiallyChecked → stretch, Checked → upscale ([KCC_gui.py#L308-L311](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L308-L311)), with Libra 2 `DefaultUpscale: True` (section 1).
- **Gamma**: see section 2D. The `-g` default "0.0" means Auto = profile gamma (1.0 for all current profiles). The GUI passes gamma only if its box is checked and the value is > 0.09 — [KCC_gui.py#L312-L313](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L312-L313).
- **Autocontrast** (lines 538–554): skipped in webtoon mode, with `--noautocontrast`, and for colour pages unless `--colorautocontrast`. Also skipped if the L extrema range is < 255 − 96 = 159 ("extremely low contrast, that was probably intentional"). Otherwise it runs the optional autolevel, then `ImageOps.autocontrast(image, preserve_tone=True)` with cutoff 0 — [image.py#L538-L554](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L538-L554). Pillow's implementation with `preserve_tone`: histogram of `convert("L")`, lo/hi = first/last non-zero bin, LUT `ix·255/(hi−lo) − lo·255/(hi−lo)` clamped to 0..255, applied to each channel — [Pillow ImageOps.py](https://github.com/python-pillow/Pillow/blob/main/src/PIL/ImageOps.py) (lines ~96–191 on main).
- **Autolevel** (`--autolevel`, default off; lines 556–572): on L (or the Y of YCbCr for colour), the black point = the index of the most frequent value among bins 0–63. Pixels at or below it are set to the black point (`p if p > bp else bp`), and autocontrast then maps bp → 0 — [image.py#L556-L572](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L556-L572).
- **Colour detection** (`colorCheck`/`calculate_color`, lines 356–441):
  - Original mode L or 1 means grey. Webtoon means colour.
  - Otherwise convert to YCbCr and take the Cb and Cr histograms. Three passes of (cutoff %, diff threshold): ((0,0), 22), ((0.2,0.2), 10), ((3,3), 4). Each pass clips cutoff% of pixels from both ends of each histogram, then computes spread = last non-zero − first non-zero.
  - If both spreads < 7 → **grey (stop)**. If any min ≤ 128 − T or any max ≥ 128 + T → **colour (stop)**. Otherwise go to the next pass. Final default is grey.
  - With `--forcecolor` the early-grey exit is replaced by "colour if any extreme is on the far side of 128" — [image.py#L356-L441](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L356-L441).
- **Greyscale**: non-colourOutput pages are `convert('L')` (lines 574–575). Before quantising, the image is converted to RGB "to remove all color pixels" (lines 579–580).
- **Rainbow eraser** (`--eraserainbow`, default off; for Kaleido/Colorsoft): `rfft2` of L (or of the Y channel of a manual RGB→YUV conversion for colour). Frequency components with radius ≥ 0.30 cycles/px whose angle is within ±10° of 135°, 315°, 225° or 45° (the diagonals) are multiplied by 0.10 (90% attenuation). Then `irfft2`, clip and back to uint8 — [rainbow_artifacts_eraser.py#L7-L249](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/rainbow_artifacts_eraser.py#L20-L112). README: it "fixes the rainbow effect on Kaleido 3 color eink … without blur using a 2D Discrete Fourier Transform" — [README](https://github.com/ciromattia/kcc#readme). It runs after resize, so it operates at device resolution (image.py ordering, comic2ebook.py lines 765–766).

### Inferences

- With default settings, a greyscale manga page for the Libra 2 is: L → (gamma 1.0 no-op) → autocontrast stretch → Lanczos `contain` to ≤1264×1680 (or `fit` if within 1.5% aspect, or upscale bicubic if smaller and upscale is on) → (PNG path: Floyd–Steinberg to 16 levels, 4-bit PNG; JPEG path: q85 8-bit).
- Parallax's spike (no dithering mentioned, Lanczos3, 16 levels) differs from KCC's PNG path mainly in KCC's Floyd–Steinberg dithering. Rust's `image` crate `FilterType::Lanczos3` corresponds to Pillow LANCZOS (a=3), an inference based on Pillow's Lanczos support of 3.

### Gaps

- Pillow's exact Floyd–Steinberg variant (serpentine or not, error clamping) lives in C (`libImaging/Convert.c`) and was not read.

## 6. Webtoon / long-strip handling (`--webtoon`)

### Takeaway

Webtoon mode merges each directory into one tall strip and finds panel gaps with an edge-detection-plus-sliding-band scan. It splits overly tall panels (with overlap for very long ones) and packs panels greedily into virtual pages of the device's aspect ratio, capped at 1072 px width.

### Cited Findings

- Forced options: no Panel View, LTR (`righttoleft=False`), no upscale, no hq, white borders — [comic2ebook.py#L1680-L1687](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1680-L1687). Cropping is skipped, autocontrast is skipped, and pages are never split by `splitCheck` (image.py lines 206–208, 240–241, 539–540). The default target size is 100 MB for webtoon (parser help).
- **Merge** (`comic2panel.mergeDirectory`, lines 44–82): the target width is the *mode* of the image widths. Each image is converted to RGB, rescaled to that width with `ImageOps.fit` BICUBIC, stacked and saved as PNG. It errors if the total height is > 524288 px — [comic2panel.py#L44-L82](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2panel.py#L44-L82).
- **Panel detection** (`splitImage`, lines 100–224) — [comic2panel.py#L100-L224](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2panel.py#L100-L224):
  1. Edges = `convert('L').filter(FIND_EDGES)`, binarised `p > 6 → 255` ("threshold of 8 is too high. 5 is too low") and converted to mode 1 without dithering (lines 110–112).
  2. Only processed if image height > target height. `h_pad = w/20`, `v_pad = w/80` rounded up to even. The scan moves a band of `[h_pad, w−h_pad] × v_pad` down in steps of `v_pad/2`. A band is "solid" if it is all black or all white (`detectSolid`, lines 85–86). Panel start = first non-solid band; panel end = the next solid band; panels that are short at the very top are skipped (lines 127–149).
  3. Virtual page: `virtual_width = min(1072, device_w, img_w)`, `virtual_height = device_h/device_w·virtual_width` (or `/1072` when device_w > 1072) (lines 151–157).
  4. Panels ≤ 1.5·H are kept. Panels ≤ 2·H are split into two H-high pieces (top and bottom, overlapping). Longer panels are split into `ceil(len/H)` H-high windows with stride `len//parts` (overlap) (lines 160–177).
  5. Greedy packing: add panels while the remaining height > 0, else start a new page. Pages shorter than 15 px are dropped. Each page is the full image width with panels concatenated, saved PNG `-NNNN.png` (lines 186–222).
- These pages then go through the normal per-file pipeline (resize via `contain`, etc.).

### Inferences

- The gutter detection assumes uniform (flat-colour) gutters. Gradient backgrounds produce "non-solid" bands, so no panel boundary is found there.

### Gaps

- No quality reports on webtoon splitting were collected.

## 7. Defaults for Kobo profiles specifically

### Takeaway

CLI and GUI defaults differ. CLI with `-p KoL`: EPUB→KEPUB, cropping 2 at power 1.0, splitter 0 (split), no manga flag, no upscale, JPEG q85, greyscale, autocontrast on, gamma 1.0, no PNG/quantise, no rainbow eraser. GUI (first run) with "Kobo Libra 2": the same, but manga on, splitter 2 (split + rotate) and upscale on.

### Cited Findings

- CLI parser defaults (`makeParser`, comic2ebook.py lines 1474–1619): profile `KV` (Kindle Voyage!) if `-p` is not given. Other defaults: format Auto (→ EPUB for Kobo, line ~1667), splitter 0, gamma 0.0 (Auto), cropping 2, croppingpower 1.0, preservemargin 0, croppingminimum 0.0, interpanelcrop 0. Upscale, stretch, forcecolor, forcepng, mozjpeg, webp, pnglegacy, noquantize, autolevel, noautocontrast, colorautocontrast, eraserainbow, norotate, rotateright, rotatefirst and manga are all False. jpegquality is None → 85 — [comic2ebook.py#L1474-L1722](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1474-L1722).
- Kobo: panelview=False and hq=False forced (lines 1690–1696). EPUB with Kobo writes `.kepub.epub` unless `--nokepub` (parser help) — [comic2ebook.py](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L1474-L1746).
- GUI first-run defaults `{'gammaSlider': 0, 'croppingBox': 2, 'croppingPowerSlider': 100, 'rotateBox': 1, 'mangaBox': 2}` — [KCC_gui.py#L1534](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L1534). The JPG/PNG box (`mozJpegBox`) is tri-state: PartiallyChecked = `--forcepng`, Checked = `--mozjpeg` ([KCC_gui.py#L385-L388](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L385-L388)). README: "You can dither by partially checking the JPG/PNG box", and 4-bit PNG gives "better quality at half the file size compared to JPG" — [README](https://github.com/ciromattia/kcc#readme).

### Inferences

- To mirror KCC's "best" Kobo B&W manga output: `-p KoL -m -r 2 -u --forcepng` (4-bit dithered PNG). Parallax's spike already used PNG at 16 levels.

### Gaps

- None significant.

## 8. Performance (multiprocessing, time per volume)

### Takeaway

KCC parallelises per source image with `multiprocessing.Pool()` (one process per CPU core by default) and logs the elapsed processing seconds. No published per-volume benchmarks were found.

### Cited Findings

- `workerPool = Pool(maxtasksperchild=100)` with no process count, so Python defaults to `os.cpu_count()` processes. One `apply_async` per file, and the whole pool terminates on the first worker error — [comic2ebook.py#L693-L743](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L691-L743). It prints `imgFileProcessing: N seconds` (line 714). The webtoon merge and split each use their own `Pool(maxtasksperchild=10)` ([comic2panel.py#L264, #L270](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2panel.py#L227-L323)). PDF rendering uses a `Pool()` sized from `cpu_count()` (comic2ebook.py lines 884–922).
- On Windows the GUI lowers its own process priority to BELOW_NORMAL — [KCC_gui.py#L1554-L1558](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/KCC_gui.py#L1554-L1558).
- README claims converting directly to CBZ/KEPUB (no kindlegen) can mean "twice as fast conversions", and that processed output can improve "page turn speed" and battery life — [README](https://github.com/ciromattia/kcc#readme). PNG with MOBI was historically slow — [issue #533](https://github.com/ciromattia/kcc/issues/533).
- A Rust crate, "comically", was reportedly built because KCC conversions are slow; its listing has no benchmark — [rust-digger listing](https://rust-digger.code-maven.com/crates/comically) (unverified secondary source).

### Inferences

- The per-image costs that dominate are probably: PIL decode, several full-image passes in cropping (autocontrast + box blur + point LUT, plus Python per-row loops in the page-number window), Lanczos resize, and PNG `optimize` (zlib level 9). A Rust port with rayon per page should easily beat it; this is not measured.

### Gaps

- No reliable minutes-per-volume figures found (web search returned none). The coordinator could measure by running KCC on a test volume, which this task's constraints forbade.

## 9. Known issues: image quality, cropping, spreads, banding

### Takeaway

The recurring complaints are perceived darkening from autocontrast (not gamma), spread halves misaligned by asymmetric cropping, inter-panel crop missing diagonal gutters, blank pages on Kindle Scribe/Colorsoft (device-side), rainbow/moiré on colour e-ink (addressed by the FFT eraser), and colour profiles defaulting to greyscale on the CLI.

### Cited Findings

- Darkening: "Image-Out is darker than Image-Input" (2020). Users fixed it with gamma 1.0 or colour mode, but one very light grey page "still breaks", consistent with autocontrast stretching — [issue #356](https://github.com/ciromattia/kcc/issues/356). The request to "(really) disable autocontrast" led to `--noautocontrast` — [issue #511](https://github.com/ciromattia/kcc/issues/511). "detail is lost in dark areas even when gamma=1.0": a commenter explains that gamma 1.0 does not disable autocontrast, and the maintainer calls it a trade-off — [issue #982](https://github.com/ciromattia/kcc/issues/982). The "images too dark on PW 11/12" report (Oct 2026) turned out to be dark source files — [issue #1457](https://github.com/ciromattia/kcc/issues/1457).
- Cropping: spreads cropped then split misalign in landscape — [issue #594](https://github.com/ciromattia/kcc/issues/594). Inter-panel crop does not handle diagonal gaps — [issue #958](https://github.com/ciromattia/kcc/issues/958). A request to keep margins for black pages is open — [issue #583](https://github.com/ciromattia/kcc/issues/583). KFX + cropping gives unexpected results (open) — [issue #1330](https://github.com/ciromattia/kcc/issues/1330). Blank pages when cropping on Kindle Scribe (open) — [issue #1224](https://github.com/ciromattia/kcc/issues/1224), [#1475](https://github.com/ciromattia/kcc/issues/1475). Division by zero in edge-ignore — [issue #1219](https://github.com/ciromattia/kcc/issues/1219).
- Colour e-ink: rainbow on greys on Colorsoft — [issue #771](https://github.com/ciromattia/kcc/issues/771). A moiré correction request was answered by pointing to the rainbow eraser (KCC 9) — [issue #1060](https://github.com/ciromattia/kcc/issues/1060). Blurry output reports: the maintainer advises turning off Kobo's own "reduce rainbow" feature and using KCC's eraser instead — [issue #1142](https://github.com/ciromattia/kcc/issues/1142). Colour bit depth request — [issue #932](https://github.com/ciromattia/kcc/issues/932).
- Kobo-specific (non-image): KEPUB bookmarks/annotations disabled (open) — [issue #704](https://github.com/ciromattia/kcc/issues/704), [#418](https://github.com/ciromattia/kcc/issues/418). Exclamation marks in names crash the Kobo driver, hence `KOBO_TOUCH_DRIVER_REGEX` — [issue #919](https://github.com/ciromattia/kcc/issues/919), [comic2ebook.py#L72-L73](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comic2ebook.py#L72-L73).
- E-ink banding (posterisation in greyscale gradients): no dedicated issue found. KCC's mitigation is Floyd–Steinberg dithering on the PNG path (section 2D).

### Inferences

- For Parallax, offering "keep spread whole (rotated)" in addition to split, as KCC's mode 2 does, directly addresses the spike's trade-off. Autocontrast should be optional because of the darkening complaints.

### Gaps

- MobileRead threads were not reviewed in depth because of the tool budget.

## 10. Licences (KCC and bundled libraries)

### Takeaway

The repository LICENSE file and setup.py say **ISC**, but `image.py` (and other modules derived from Mangle) carry **GPLv3-or-later** headers, and the bundled DualMetaFix is GPL-3. A GPLv3 project like Parallax can port either way, but should treat the derived code as GPLv3 and keep the attribution. PyMuPDF (used for PDF input) is AGPL-3.0.

### Cited Findings

- `LICENSE.txt`: "ISC LICENSE, Copyright (c) 2012-2025 Ciro Mattia Gonano; (c) 2013-2019 Paweł Jastrzębski; (c) 2021-2023 Darodi" — [LICENSE.txt](https://github.com/ciromattia/kcc/blob/v13.0.0/LICENSE.txt). `setup.py` line 139: `license='ISC License (ISCL)'` — [setup.py](https://github.com/ciromattia/kcc/blob/v13.0.0/setup.py). README: "KCC is released under ISC LICENSE" and "DualMetaFix script by K. Hendricks. Released with GPL-3 License". The app icon is CC BY-NC-SA 3.0 — [README](https://github.com/ciromattia/kcc#copyright).
- `image.py` lines 1–20: copyrights Alex Yatskov 2010, Stanislav Kosolapov 2011, Alberto Planas 2016, Gonano 2012-2014, Jastrzębski 2013-2019, under "GNU General Public License … either version 3 of the License, or (at your option) any later version" — [image.py#L1-L20](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/image.py#L1-L20). (Alex Yatskov wrote Mangle; this attribution is inferred from the copyright line.)
- Dependency licences (PyPI metadata, Nov 2026): Pillow 13.0.0 is **MIT-CMU** ([PyPI](https://pypi.org/project/pillow/)). numpy 2.5.4 is BSD-3-Clause AND 0BSD AND MIT AND Zlib AND CC0-1.0 ([PyPI](https://pypi.org/project/numpy/)). mozjpeg-lossless-optimization 1.3.2 is BSD-3-Clause ([PyPI](https://pypi.org/project/mozjpeg-lossless-optimization/)). natsort 8.4.0 is MIT ([PyPI](https://pypi.org/project/natsort/)). PyMuPDF / pymupdf-lite 2.1 are dual-licensed AGPL-3.0 or Artifex commercial ([PyPI](https://pypi.org/project/PyMuPDF/)). PySide6 6.12.0 is LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only ([PyPI](https://pypi.org/project/PySide6/)).
- setup.py requires Pillow ≥9.3.0, mozjpeg-lossless-optimization ≥1.2.0, natsort ≥8.4.0, numpy ≥1.22.4, and PyMuPDF <2.0 (py<3.12) or pymupdf-lite ≥2.0 — [setup.py#L154-L163](https://github.com/ciromattia/kcc/blob/v13.0.0/setup.py). External archive tools (7-Zip/7zz, unar, unrar, tar) are invoked as subprocesses, not linked ([comicarchive.py](https://github.com/ciromattia/kcc/blob/v13.0.0/kindlecomicconverter/comicarchive.py)).

### Inferences

- The licence situation is mixed: ISC at repository level, GPLv3+ in individual file headers. Since Parallax is GPLv3, porting algorithms from GPLv3+ files is compatible. Keep the copyright notices of image.py's authors in the ported module.

### Gaps

- Which files exactly carry GPL vs ISC headers was not enumerated (only image.py was checked). The crop-algorithm files (`page_number_crop_alg.py`, `inter_panel_crop_alg.py`, `common_crop.py`) have **no** licence header, so they fall under the repository's ISC licence by default (inference).
