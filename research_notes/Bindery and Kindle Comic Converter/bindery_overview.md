# Bindery (jarynclouatre/bindery): project profile

Repository examined: <https://github.com/jarynclouatre/bindery>, default branch `master`, HEAD `730dd98` (v4.3.2, 2026-09-09). I read it from a shallow clone (`--depth 200`) at `/private/tmp/claude-501/-Users-jacksonferguson-Developer-parallax/34c77541-5d73-4dd5-8ad8-408d704eda57/scratchpad/research-src/bindery-a`. I did not build, run or install anything from it. Code links below point at `blob/730dd98`. The name "bindery" is ambiguous. A separate, unrelated Go project, `vavallee/bindery`, does ebook and audiobook automation ([gittrend](https://gittrend.io/repo/vavallee/bindery)). These notes cover only the jarynclouatre project the brief named.

## What problem does bindery solve, who is it for, and what features does it have?

### Takeaway

Bindery is a small headless converter for self-hosters, delivered as a Docker image (NAS, Raspberry Pi, homelab). It watches drop folders. EPUBs go through kepubify to KEPUB, and comic archives and PDFs go through Kindle Comic Converter (KCC). A Flask web UI on port 5000 controls settings, shows job status and lets users download output. It is not a library manager, a metadata fetcher, a device manager or a reader. It is a "hot folder" wrapper around two external CLIs.

### Cited Findings

- The README tagline says you drop ebooks and comics into a folder and they come out ready for your e-reader. The repo describes itself as "a self-hosted, Dockerized converter" that needs no manual steps. — [README.md](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- GitHub description: "kepubify + KCC with a WebUI, in one Docker image". Topics include calibre-web-automated, kobo, kindle, kindlecomicconverter and self-hosted. — [GitHub API repo metadata](https://api.github.com/repos/jarynclouatre/bindery)
- Folder layout: `Books_in`, `Books_out`, `Comics_in` (with `.archive/`), `Comics_out`, `Comics_raw` (with `processed/` and `unprocessed/`) and `config/` (`settings.json`, `jobs.json`, `converted.json`). — [README.md "Folder Layout"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md); constants at [processor.py L17-L66](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L17-L66)
- Input types: books are `.epub` only. Comics are `.cbz .cbr .zip .rar .pdf`. Raw folders of images (`.jpg .jpeg .png .gif .webp`) also count as comics. — [processor.py L23-L31](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L23-L31), [raw_processor.py L17](https://github.com/jarynclouatre/bindery/blob/730dd98/raw_processor.py#L17)
- Supported devices: Kindle, Kobo, reMarkable, and any device KCC has a profile for. The default KCC profile is `KoLC` (Kobo Libra Colour). — [README.md](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md), [config.py L14](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L14)
- Web UI features:
  - drag-and-drop upload that routes each file to the right watch folder
  - live job table (queued, processing, success or failed), with before and after sizes and a Retry button
  - job history capped at 500 entries
  - file browser for downloading from `Books_out` and `Comics_out`
  - live log
  - light and dark themes
  - Apprise push notifications (60+ services)

  Sources: [README.md "WebUI"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md). Routes `/health`, `/api/upload`, `/api/logs`, `/api/status`, `/api/stats`, `/api/profiles`, `/api/retry`, `/api/files`, `/api/files/download`, `/api/restart` and `/` are at [app.py L112-L301](https://github.com/jarynclouatre/bindery/blob/730dd98/app.py#L112-L301).
- "Device Profiles" are named KCC setting sets (e.g. `kobo`, `kindle`), each with its own `Comics_in/<name>` and `Comics_out/<name>` folders. A profile overrides only `kcc_*` keys and inherits everything it hasn't saved from the main settings. — [README.md "Device Profiles"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md); [config.py L63-L65, L121-L133](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L63-L133)
- Automation has two watcher modes. `poll` scans every 10 s and is NFS/SMB-safe. `inotify` uses watchdog and adds a 60 s backstop scan. A file-stability timeout (default 60 s, range 10-300 s) applies, and folders must be quiet for at least 30 s. — [README.md "Bindery Settings"/"Behaviour"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md); [processor.py L1235-L1366](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1235-L1366)
- Integrations are documented: point Calibre-Web Automated or Calibre Auto-Add at the output folders, sync with rclone, use `/api/stats` for a Homepage widget, and use `/health` for Uptime Kuma. — [README.md "Use Cases"/"Dashboard integration"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- The web UI has no authentication. The README tells users not to expose port 5000 to the internet. — [README.md "Behaviour"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- MOBI and KFX output were removed in v3.4.0 because kindlegen and the Calibre KFX plugin can't ship in the image. Kindle users are told to make EPUB and use Send to Kindle. — [CHANGELOG.md v3.4.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)
- An Unraid Community Apps listing exists. — [Unraid apps](https://ca.unraid.net/apps/bindery-0skqhy81oaa84s)

### Inferences

- Features Bindery does not have, confirmed by reading every source file: library database, metadata lookup from online sources, series database, cover handling or upscaling of covers, reader, device detection or device sync. It is server and web only, with no desktop app.
- For Parallax, the transferable ideas are workflow and UX: per-device conversion profiles, bundling chapters into volumes, a conversion ledger and the folder-stability rules. Bindery has no comic-handling algorithms of its own to port. KCC does all the image work.

### Gaps

- I couldn't find Docker Hub pull counts for `dinkeyes/bindery`. I didn't fetch them.

## Language, frameworks, dependencies, architecture and external tools

### Takeaway

It is roughly 2,150 lines of Python 3.13 in four modules, plus a 1,328-line Jinja HTML template. It runs Flask under gunicorn with one worker and uses a thread per job. All conversion is done by shelling out to `kcc-c2e` (KCC 11.0.1, pip-installed from git) and `kepubify` (v4.0.4, built from pinned source). It also shells out to `7z` for chapter extraction. It does not use calibre.

### Cited Findings

- Module layout and line counts:
  - `app.py` (420): Flask app factory, form validation and routes
  - `config.py` (143): defaults, migrations, profile overlay and atomic save
  - `processor.py` (1,366): job registry, ledger, command builders, file and folder processing, scanners and watchers
  - `raw_processor.py` (213): zips raw image folders into CBZ
  - `templates/index.html` (1,328): single-page UI
  - `tests/`: about 2,200 lines of pytest

  Source: repository tree at [730dd98](https://github.com/jarynclouatre/bindery/tree/730dd98) (`wc -l` on the clone).
- Python dependencies: Flask>=3.1.3, gunicorn>=26, watchdog>=6, apprise>=1.12, and KCC's CLI runtime deps (Pillow, psutil, requests, python-slugify, packaging, mozjpeg-lossless-optimization, natsort, numpy, PyMuPDF). KCC is installed `--no-deps` so that PySide6, which only the GUI needs, isn't pulled in. — [requirements.txt](https://github.com/jarynclouatre/bindery/blob/730dd98/requirements.txt), [Dockerfile](https://github.com/jarynclouatre/bindery/blob/730dd98/Dockerfile)
- Docker build:
  - A multi-stage build compiles kepubify at commit `8e959ed…` with Go 1.26.5, bumps `golang.org/x/sync` and `x/text`, and stamps the binary v4.0.4.
  - The runtime base is `python:3.13-slim`.
  - It installs `p7zip-full`, `p7zip-rar`, `unrar` (from Debian non-free) and `gosu`.
  - It pip-installs `git+https://github.com/ciromattia/kcc.git@v11.0.1`.
  - The CMD is `gunicorn --workers 1 … app:app`.

  Source: [Dockerfile](https://github.com/jarynclouatre/bindery/blob/730dd98/Dockerfile)
- `entrypoint.sh` maps PUID and PGID to an `abc` user, optionally chowns the volumes (`SKIP_CHOWN`), and drops privileges with gosu. — [entrypoint.sh](https://github.com/jarynclouatre/bindery/blob/730dd98/entrypoint.sh)
- Concurrency: every file or folder job gets its own daemon `threading.Thread`, and a `PROCESSING_LOCKS` set dedupes them. KCC runs are serialised by `kcc_semaphore = threading.Semaphore(1)`. Book (kepubify) jobs run in parallel. — [processor.py L42-L50, L918, L1018, L1165-L1233](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1165-L1233)
- The KCC invocation is built in `_build_kcc_cmd`. It maps settings to `--profile --format --splitter --cropping --croppingpower --croppingminimum --batchsplit --gamma --manga-style --hq --two-panel --webtoon --blackborders/--whiteborders --forcecolor --colorautocontrast --eraserainbow --mozjpeg --stretch --upscale --norotate --rotateright --rotatefirst --nokepub --metadatatitle 1 / --title= --author= --customwidth/--customheight`. — [processor.py L712-L782](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L712-L782)
- The kepubify invocation is `kepubify --inplace --output <tmp> [--calibre] [--smarten-punctuation] [--fullscreen-reading-fixes] [--hyphenate|--no-hyphenate] [--add-dummy-titlepage|--no-add-dummy-titlepage] [--css=] [--replace=find|replace…] [--charset=]`. — [processor.py L808-L849](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L808-L849)
- Subprocess output is streamed line by line into a 300-line in-memory log and `/app/config/bindery.log`. A non-zero exit raises `ConversionError`. — [processor.py L44-L52, L699-L709](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L699-L709)
- Hardening details:
  - Filenames starting with `-` are renamed, because KCC passes the bare basename to 7z, which reads it as a switch.
  - `--title=` and `--author=` use the `=` form so values can't be parsed as options.

  Source: [processor.py L785-L805, L764-L773](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L764-L805)
- Settings are a flat JSON dict with defaults. Saves are atomic (tmp file, then `os.replace`). Legacy keys are migrated in place, e.g. `kcc_rotate` and `kcc_nosplitrotate` become splitter modes, and `preserve_originals` becomes `originals`. — [config.py L13-L143](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L13-L143)
- CI: GitHub Actions runs pytest on Python 3.13 and a Docker build. A release workflow pushes multi-arch (`linux/amd64,linux/arm64`) images to Docker Hub `dinkeyes/bindery`. — [.github/workflows/test.yml, docker.yml](https://github.com/jarynclouatre/bindery/tree/730dd98/.github/workflows)

### Inferences

- The architecture is a simple "watch, shell out, move" loop. Parallax already has a kepubify sidecar, so the only new thing that matters is a KCC sidecar, or a native Rust reimplementation of KCC's image pipeline. Bindery's `_build_kcc_cmd` is a useful, up-to-date map of the KCC 11 CLI flags a UI would need to expose. The flag names themselves are facts about KCC's interface, not Bindery code.
- A one-worker gunicorn process plus threads means all state lives in process globals. That model doesn't apply to a Tauri app.

### Gaps

- I didn't check whether `kcc-c2e` 11.0.1 has extra runtime needs on macOS, such as 7z or unrar. That belongs to KCC research.

## Licence, copyright holders and bundled third-party code

### Takeaway

Bindery is MIT, copyright 2026 jarynclouatre. MIT is GPL-3.0-compatible, so Parallax (GPL-3.0-only) may copy Bindery's code if it keeps the MIT copyright and permission notice. The repo bundles only two OFL fonts. KCC, kepubify, unrar and 7-Zip are pulled in at Docker build time, not vendored. Their licences matter for the image, not for copying Bindery's source.

### Cited Findings

- `LICENSE`: "MIT License, Copyright (c) 2026 jarynclouatre". GitHub detects the SPDX ID as `MIT`. — [LICENSE](https://github.com/jarynclouatre/bindery/blob/730dd98/LICENSE); [GitHub API](https://api.github.com/repos/jarynclouatre/bindery)
- No source file carries its own copyright or SPDX header. The only other copyright notices are on the OFL font licences, Fraunces and IBM Plex Mono, in `static/fonts/`. — [static/fonts/](https://github.com/jarynclouatre/bindery/tree/730dd98/static/fonts)
- The second contributor, `tekgnosis-net`, wrote the v4.3.0 Books settings work (#14) and sends PRs under the repo's MIT licence. No CLA was found. — [CHANGELOG.md v4.3.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md); [contributors API](https://api.github.com/repos/jarynclouatre/bindery/contributors)
- In a MobileRead announcement, the author describes Bindery as MIT-licensed automation around KCC and kepubify. — [MobileRead thread](https://www.mobileread.com/forums/showthread.php?p=4598275)

### Inferences

- MIT to GPL-3.0-only is a well-established one-way compatibility. Copying any Bindery function into Parallax is fine with attribution, e.g. a `NOTICE` or third-party licences entry naming "Copyright (c) 2026 jarynclouatre" with the MIT text. In practice there is little algorithmic code worth copying, and Parallax is Rust, so a reimplementation is more likely than a port.
- The Docker image ships `unrar`, which has a non-free licence, and `p7zip-rar`. Parallax should not copy that approach for CBR support. Use a permissively licensed RAR decoder or an external tool. (This is my inference, not verified against unrar's licence text here.)
- KCC's own licence decides whether Parallax can bundle or port KCC. I didn't verify it in this task; it belongs to the KCC research.

### Gaps

- I didn't confirm the exact licences of KCC 11.0.1 and kepubify v4.0.4 from their repos.

## Maintenance status and community

### Takeaway

It is a young, very active project with a single main maintainer. It started in March 2026, has about 205 commits and 23 versioned releases (v2.x through v4.3.2), and its last push was 2026-09-09. It has 67 stars, 3 forks, 2 contributors, and 0 open issues or PRs. It is small and responsive but has a bus factor of one.

### Cited Findings

- The repo was created 2026-03-05T00:36:44Z and last pushed 2026-09-09T07:24:09Z. Stars 67, forks 3, watchers 2, open issues 0. — [GitHub API](https://api.github.com/repos/jarynclouatre/bindery)
- The earliest commit in the clone is `395f24e` (2026-03-04). The latest is `730dd98` (2026-09-09, "chore: bump version to 4.3.2"). `git log` counts 205 commits; `git shortlog` shows 195 by jarynclouatre and 10 by tekgnosis-net. The contributors API counts 196 and 10. — local clone; [contributors API](https://api.github.com/repos/jarynclouatre/bindery/contributors)
- Releases: v4.3.2 (2026-09-09), v4.3.1 (2026-08-10), v4.3.0 (2026-08-09), v4.2.1 and v4.2.0 (2026-07-15). The CHANGELOG lists versions from v2.4.0 to v4.3.2, about 23 headings. — [releases API](https://api.github.com/repos/jarynclouatre/bindery/releases); [CHANGELOG.md](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)
- Issues and PRs (#1-#14) are all closed. They include:
  - a raw-comics pipeline PR (#1)
  - Kindle Colorsoft (#2)
  - files picked up mid-copy (#3)
  - dot-folders (#4)
  - folder structure and originals preservation (#5)
  - chown failing on NFS/LXC (#7)
  - PDF/EPUB scanning (#8)
  - per-folder conversion (#9)
  - rainbow eraser (#10)
  - mozjpeg (#11)
  - keep originals in place (#13)
  - Books settings (#14)

  Source: [issues API](https://api.github.com/repos/jarynclouatre/bindery/issues?state=all)
- Notable bugs fixed, per the changelog:
  - Cropping was silently disabled: a percentage was passed where KCC wants a ratio (v3.4.0).
  - inotify never processed folder jobs (v3.4.0).
  - `.failed` folders were rescanned (v3.4.0).
  - Dash-leading filenames broke KCC's 7z call (v3.4.0).
  - Library-manager timestamp touches caused endless re-conversion loops with `_2`, `_3` copies (v4.2.1).
  - Overlapping `Books_in`/`Books_out` caused self-reconversion (v4.3.0).
  - A blocked extension discarded the whole settings save (v4.3.1).

  Source: [CHANGELOG.md](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)
- Community footprint: one MobileRead announcement by "Dinkeyes" (2026-07-15) with no replies, newreleases.io tracking, and an Unraid CA listing. — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=4598275); [newreleases.io](https://newreleases.io/project/github/jarynclouatre/bindery/release/v4.3.0); [Unraid](https://ca.unraid.net/apps/bindery-0skqhy81oaa84s)

### Inferences

- The fixed-bug list is a ready-made checklist of pitfalls for Parallax's comic import and conversion pipeline: mid-transfer files, dash-prefixed names, idempotency when output sits next to input, and units for the cropping minimum.
- Tags in the clone start at v3.4.0 only because the clone is shallow; the earlier tags fall outside its depth.

### Gaps

- The clone is shallow (`--depth 200`), so the commit count may be undercounted by a few. The contributors API total (206) suggests it is complete or nearly so.
- I didn't search for Reddit threads.

## How bindery handles comics

### Takeaway

Bindery does no image processing, page ordering or metadata parsing itself. It passes everything to KCC (`kcc-c2e`). Its own comic logic is limited to:

- deciding what counts as a volume (single archive, folder of images, or a folder of chapter archives bundled into one volume)
- natural-sort ordering of chapters
- zipping raw image folders into CBZ
- naming output and placing it next to the source

Output is EPUB (KEPUB by default, `.kepub.epub` normalised to `.kepub`) or CBZ.

### Cited Findings

- Accepted comic inputs are `.cbz .cbr .zip .rar .pdf`. Only `.cbz .cbr .zip .rar` can be bundled. PDF support was added in v3.3.0. — [processor.py L24, L30](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L24-L30); [CHANGELOG v3.3.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)
- Volume detection (`_is_bundle_folder`):
  - A top-level folder holding only images is one volume, with subfolders as chapters.
  - A folder of only archives bundles only when `bundle_chapter_folders` is on.
  - A PDF or loose images mixed with archives forces per-file conversion.
  - Folders with no comic files are never jobs.

  Source: [processor.py L1075-L1108](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1075-L1108)
- Chapter bundling (`_extract_chapter_folder`):
  1. Natural-sort the archives so `ch2` comes before `ch10` (`_natural_key` splits on digit runs).
  2. Extract each one with `7z x` into a `NNN - <stem>` subfolder, because KCC sorts chapter directories lexicographically.
  3. "Hoist" single-directory wrappers so each chapter folder holds the images directly.

  Source: [processor.py L1111-L1162](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L1111-L1162)
- Raw pipeline: a flat folder of images in `Comics_raw` is checked: subfolders or no images send it to `unprocessed/`. Otherwise its images are zipped with `ZIP_STORED`, sorted by lowercase basename (a plain sort, not a natural sort), into `Comics_in/<folder>.cbz`. The original moves to `processed/`. — [raw_processor.py L72-L140](https://github.com/jarynclouatre/bindery/blob/730dd98/raw_processor.py#L72-L140)
- Metadata:
  - With `kcc_comicinfo` on, Bindery passes `--metadatatitle 1`, and KCC reads title, series and number from the embedded ComicInfo.xml.
  - Otherwise, if `kcc_metadatatitle` is on (the default), the filename stem becomes `--title=`.
  - An optional global `--author=` is available.
  - Bindery does not parse filenames for series or volume.

  Source: [processor.py L764-L773](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L764-L773); [config.py L36-L38](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L36-L38); [templates/index.html L731-L734](https://github.com/jarynclouatre/bindery/blob/730dd98/templates/index.html#L731-L734). The v4.2.0 changelog example says `Chapter 1 (2).cbz` arrives as "Berserk #001: The Black Swordsman" ([CHANGELOG v4.2.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)).
- RTL and manga: right-to-left page order is the `--manga-style` toggle (`kcc_manga_style`, off by default). Webtoon mode (`--webtoon`) and two-panel mode (`--two-panel`) are also exposed. — [processor.py L747-L750](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L747-L750); [README "KCC Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)
- Image-processing options exposed (all done by KCC). The UI choices are at [templates/index.html L504-L676](https://github.com/jarynclouatre/bindery/blob/730dd98/templates/index.html#L504-L676) and the defaults at [config.py L14-L41](https://github.com/jarynclouatre/bindery/blob/730dd98/config.py#L14-L41).

  | Option | Choices | Default |
  |---|---|---|
  | Cropping | disabled / margins / margins and page numbers | 2 |
  | Cropping power | | 1.0 |
  | Cropping minimum | percentage in the UI, converted to a 0-1 ratio for KCC | |
  | Landscape splitter | 0 Split / 1 Rotate / 2 Split and rotate | 1 |
  | Gamma | auto or 0.5-2.2 | |
  | Borders | auto / black / white | black |
  | Force colour | on / off | on |
  | Colour autocontrast | on / off | on |
  | Rainbow eraser | for colour e-ink | off |
  | MozJPEG re-encode | on / off | off |
  | Stretch | on / off | on |
  | Upscale | on / off | |
  | HQ | on / off | |
  | Rotation | norotate / rotateright / rotatefirst | |
  | Batch split | disabled / automatic / one volume per subdirectory | |
  | Custom width and height | used with profile `OTHER` | |

- Output: the format is EPUB or CBZ, and anything else falls back to EPUB. `--nokepub` is optional. KCC's `.kepub.epub` output is renamed to `.kepub`. — [processor.py L716, L755, L683-L685](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L664-L689)
- v4.3.0 aligned the settings with KCC 11, adding the Split, Rotate, and Split-and-rotate modes plus the Kindle Paperwhite and Scribe profiles. — [CHANGELOG v4.3.0](https://github.com/jarynclouatre/bindery/blob/730dd98/CHANGELOG.md)
- Graphic-novel EPUBs are not accepted as KCC input. They go in the books folder and through kepubify. — [awesome.ecosyste.ms summary of README/FAQ](https://awesome.ecosyste.ms/projects/github.com%2Fjarynclouatre%2Fbindery). (This is a secondary source; I didn't find it in the README at HEAD.)

### Inferences

- If Parallax wants KCC-quality output, it must either bundle KCC (Python plus Pillow, numpy and PyMuPDF) as a sidecar the way it bundles kepubify, or reimplement KCC's pipeline in Rust. Bindery shows the sidecar approach works headlessly with `--no-deps`, which avoids PySide6.
- Patterns worth copying conceptually:
  - Natural-sort the chapter archives, then extract them into zero-padded `NNN - stem` directories so ordering survives a lexicographic sort downstream.
  - Hoist single-folder wrappers.
  - Treat "folder of images" as one volume.
  - Expose ComicInfo, or the filename as a fallback, for the title.
- The raw zipper's non-natural sort (`sorted(..., key=lower)`) would misorder `page2` and `page10`. This looks like a latent bug; I didn't see a test for it. KCC may re-sort pages internally, which I didn't verify.

### Gaps

- What exactly KCC writes to the EPUB/KEPUB (series metadata, page-progression-direction, cover) is KCC behaviour, not covered here.

## How bindery handles Kobo devices

### Takeaway

It doesn't handle Kobo devices directly. Bindery never touches a device: there is no USB detection, no KoboReader.sqlite access, no collections, no cover thumbnails and no file placement on the device. Its "Kobo support" means producing KEPUB files: books through kepubify, and comics through KCC's default kepub EPUB output with the `KoLC` profile. The user then moves them by hand or through Calibre, CWA or cloud sync.

### Cited Findings

- No source file references SQLite, KoboReader, collections, thumbnails, calibredb or ebook-convert. (I grepped `app.py`, `config.py`, `processor.py`, `raw_processor.py` and `templates/index.html`. The only hit was `from collections import deque`.) — [repository tree](https://github.com/jarynclouatre/bindery/tree/730dd98)
- Book output extension options and the reasons the README gives for each:
  - `.kepub` (default) is what Calibre and CWA expect.
  - `.kepub.epub` is what a Kobo recognises over USB.
  - `.epub` still contains kepubify's Kobo enhancements.

  Source: [README "Books Settings"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md); [processor.py L808-L849](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L808-L849)
- Comics: KCC's `.kepub.epub` is always renamed to `.kepub`, regardless of the book extension setting. — [processor.py L683-L685](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L683-L685)
- Delivery is left to downstream tools: CWA ingest, Calibre Auto Add or rclone. — [README "Use Cases"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)

### Inferences

- If a comic KEPUB is copied straight to a Kobo, `.kepub` alone may not be recognised; it needs `.kepub.epub`. Bindery's comic path ignores the book extension setting, so a direct USB user could hit this. Parallax should name comic outputs `.kepub.epub` when it writes to the device itself.
- Parallax already covers far more Kobo ground (DB writes, collections, backups) than Bindery does. Bindery offers nothing to port here.

### Gaps

- None. The absence of these features was confirmed by reading every source file.

## Storage model: copying, linking, caching and disk usage

### Takeaway

It is a move-based pipeline with no library store and no conversion cache:

- Each job converts into a temporary directory in `/tmp`.
- The output is moved into the mirrored path under `*_out`.
- The source is then handled by the "Originals" setting: deleted (default), moved to `Comics_in/.archive`, or kept in place. Books are always deleted.
- Keep-in-place relies on a JSON ledger of the source's size and mtime plus its output paths, so it avoids re-conversion and replaces stale outputs.

### Cited Findings

- `process_file`, step by step:
  1. Create the temporary output directory `/tmp/<uuid>_out`.
  2. Wait until the file is stable.
  3. Run the converter.
  4. Move each output to `out_base/<relative dir>` using collision-free `_2`, `_3` names.
  5. Handle the source: `keep` records it in the ledger, `archive` moves it to `.archive` mirroring its path, and `delete` removes it and prunes empty directories.
  6. On failure, rename the source to `<name>.failed`.
  7. Always remove the temporary directory.

  Source: [processor.py L864-L973](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L864-L973). Folder jobs are the same, with `shutil.rmtree` at [processor.py L976-L1072](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L976-L1072).
- "Books never keep or archive their source; only comics do." — [processor.py L929-L930](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L929-L930)
- Ledger (`/app/config/converted.json`):
  - Signature: `size:mtime` for files, or `count:total:newest-mtime` for folders, skipping dot-dirs.
  - A change in mtime alone with the same size is not treated as a change, which avoids loops with library managers.
  - Entries are never dropped for missing paths, because of NAS mount blips.
  - The stored outputs are deleted before a re-convert.

  Source: [processor.py L141-L252](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L141-L252)
- Upload safety: uploads are written into a hidden `.uploading/<uuid>.part` and then `os.replace`d into place, so scanners only ever see complete files. — [app.py L120-L161](https://github.com/jarynclouatre/bindery/blob/730dd98/app.py#L120-L161)
- Readiness checks: `wait_for_file_ready` checks file stability up to the timeout. `_is_dir_stable` and `is_folder_stable` require a quiet period for folders. — [processor.py L514-L604](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L514-L604); [raw_processor.py L25-L45](https://github.com/jarynclouatre/bindery/blob/730dd98/raw_processor.py#L25-L45)
- Overlapping-folder safety: book processing pauses if `Books_in`/`Books_out` overlap and the extension is `.epub` or `.kepub.epub`. This includes detecting the same mount by reading mount info. — [processor.py L380-L477](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L380-L477)
- Disk accounting: lifetime bytes saved (source minus output) are kept in `stats.json` and shown via `/api/stats`. There is no quota or cache eviction. — [processor.py L255-L287](https://github.com/jarynclouatre/bindery/blob/730dd98/processor.py#L255-L287); [README "Dashboard integration"](https://github.com/jarynclouatre/bindery/blob/730dd98/README.md)

### Inferences

- The default is destructive: originals are deleted. That suits a drop-box but clashes with a library app. Parallax would want the "keep in place" semantics plus a managed conversion cache keyed by source hash and conversion profile, and should not delete originals.
- Using a size+mtime signature that ignores mtime-only changes is a pragmatic idempotency trick. A content hash would be more robust for Parallax, which owns its library store.

### Gaps

- None material to the brief.
