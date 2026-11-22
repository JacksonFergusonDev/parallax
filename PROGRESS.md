# Progress

The maintainer handoff: where things stand, what's open and what's next. Update it when those change or at a release, not for routine fixes. [CHANGELOG.md](CHANGELOG.md) records user-visible changes and Git history records the rest.

[Engineering index](engineering/README.md) · [Hardware checks](engineering/hardware-checks.md) · [Hardware results](engineering/hardware-results.md) · [Releasing](engineering/releasing.md)

## State of the project

- Kindle support: sending as AZW3 with page numbers (calibre's accurate method) and thumbnails, comics as fixed-layout AZW3, updating, removal, adding books, progress from KRDS sidecars, highlights from My Clippings and KRDS sidecars, Send to Kindle email, over a drive or MTP through mtp-rs. Not yet run on a Kindle; see the [Kindle decision](engineering/decisions/kindle.md) and [Kindle hardware checks](engineering/kindle-hardware-checks.md). Browser mock flags: `?kindle`, `?kindle=drive`, `?kindle=busy`, `&kindle_email=…`.
- Kobo library management, native collections, reading stats, backups with revert, and device reports. The reference Libra 2 results cover the workflows tested on each date. Untested or read-only Kobos are explained in setup or with a main-window banner, and `TESTED_MODELS` in `crates/device-kobo/src/support.rs` lists models with hardware results.
- Desktop EPUB and comic readers, reading places kept in step between a Kobo and the desktop reader, comic preparation for Kobo and Kindle, comic metadata lookups, PDF comics imported as CBZ, and ZIP, RAR and 7z bundle import.
- First-run setup covers connecting a Kobo, importing its books, settings, appearance and a short tour. Browser mock flags: `?setup` (with `&kobo=later` or `&kobo=none`), `?untested`, `?readonly`.
- Releases: a tag-driven workflow builds the universal Mac app and per-architecture CLI archives, publishes a GitHub release from CHANGELOG.md, redeploys the site in `site/` and updates the Homebrew formula. The app checks for signed updates and can download them in the background.
- Verification: prek runs quick checks on commit and builds, lints, type checks and tests on push. CI checks both Mac architectures, with the WKWebView suite split across two Apple silicon jobs, and Linux and Windows builds in the manual Portability workflow. Renovate opens weekly dependency PRs for manual review.

## Current

- 0.1.3 is released. Unreleased: Kindle support, Export Library, PDF comics and Kobo place sync. Releases are signed ad hoc until there's a Developer ID.

## Unresolved

- The hardware checklist tracks open Kobo verification. Only the Libra 2 has a physical-device record so far.
- Every Kindle hardware check is open. A friend's Kindle is available for a first run. The riskiest parts are MTP on macOS (photo import service, other apps holding the device), thumbnails and APNX on the device, KRDS fields and positions on current firmware (highlight text assumes KF8 positions are byte offsets), and fixed-layout comics, which lack the RESC record KindleGen writes.
- Kobo place sync with the desktop reader is untested on hardware: check that Nickel opens at a `ChapterIDBookmarked` Parallax writes.
- Check VoiceOver and reader input, the real desktop cover-upscale review flow, and authenticated Metron lookups.
- Real DRM-free Kindle files were checked against calibre. Combo MOBI and real KFX checks remain open. Remove the [vendored boko patch](vendor/boko/VENDORED.md) after upstream releases all four local fixes (boko#37, #39, #40 and the MOBI contents fix).

## Next

- Run the outstanding release hardware checks and collect reports from additional Kobo models.
- Bring the user-facing pages in `docs/` onto the site. Add app screenshots to the site's landing page.
- Run the Kindle hardware checks, then add tested models to `TESTED_MODELS` in `crates/device-kindle/src/models.rs`. Additional comic processing remains future work.
- Look up missing descriptions from Apple Books and Google Books, which already return them. Hardcover metadata was dropped in November 2026: on a 99-book library it filled about 6 real series that the current sources missed, with noisy genres, and it needs an account and OAuth.

## Postponed

- [Kobo compatibility check](engineering/plans/kobo-compatibility-check.md): lets users check and report untested Kobo models. Postponed until users ask for support for models we haven't tested.
