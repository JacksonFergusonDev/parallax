# Progress

[Engineering index](engineering/README.md) · [Hardware checks](engineering/hardware-checks.md) · [Hardware results](engineering/hardware-results.md)

## Completed

- Implemented Kobo library management, native collections, reading stats, backups, and device reports. The reference Libra 2 results cover the workflows tested on each date.
- The desktop EPUB and comic readers, comic preparation for Kobo, comic metadata lookups, and ZIP, RAR, and 7z bundle import are implemented.
- Development scripts and fixture generators use locked Python tooling. CI checks both Mac architectures, with the WKWebView suite split across two Apple silicon jobs and Linux and Windows builds in the manual Portability workflow.
- Added first-run setup. It walks through connecting a Kobo, importing its books, Kobo preferences, appearance and a short tour. It shows on each launch until skipped or finished, libraries from before it are marked done quietly, and Help › Welcome to Parallax reopens it. Browser mock: `?setup`, plus `&kobo=later` or `&kobo=none`.
- Tuned Renovate to open weekly dependency PRs for manual review, group Tauri updates, and keep the scripts interpreter on Python 3.12 for Core ML tooling.

## Current

- Preparing the first release. The engineering index consolidates contributor references and links to the pre-release research and plans in Git history. `docs/` holds user-facing material for the future Zensical site.

## Unresolved

- The hardware checklist tracks open Kobo verification. Only the Libra 2 has a physical-device record so far.
- Check VoiceOver and reader input, the real desktop cover-upscale review flow, and authenticated Metron lookups.
- Real DRM-free Kindle files were checked against calibre. Combo MOBI and real KFX checks remain open. Remove the [vendored boko patch](vendor/boko/VENDORED.md) after upstream releases all three local fixes.

## Next

- Run the outstanding release hardware checks and collect reports from additional Kobo models.
- Set up the Zensical site from the user-facing pages in `docs/`.
- Kindle device support is deferred. Optional Hardcover metadata, Kobo position mapping, PDF comics, and additional comic processing remain future work.
