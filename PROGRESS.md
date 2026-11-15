# Progress

The maintainer handoff: where things stand, what's open and what's next. Update it when those change or at a release, not for routine fixes. [CHANGELOG.md](CHANGELOG.md) records user-visible changes and Git history records the rest.

[Engineering index](engineering/README.md) · [Hardware checks](engineering/hardware-checks.md) · [Hardware results](engineering/hardware-results.md) · [Releasing](engineering/releasing.md)

## State of the project

- Kindle support (branch `feat/kindle`): sending as AZW3 with page numbers and thumbnails, updating, removal, adding books, progress from KRDS sidecars, highlights from My Clippings, Send to Kindle email, over a drive or MTP through mtp-rs. Not yet run on a Kindle; see the [Kindle decision](engineering/decisions/kindle.md) and [Kindle hardware checks](engineering/kindle-hardware-checks.md). Browser mock flags: `?kindle`, `?kindle=drive`, `?kindle=busy`, `&kindle_email=…`.
- Kobo library management, native collections, reading stats, backups with revert, and device reports. The reference Libra 2 results cover the workflows tested on each date. Untested or read-only Kobos are explained in setup or with a main-window banner, and `TESTED_MODELS` in `crates/device-kobo/src/support.rs` lists models with hardware results.
- Desktop EPUB and comic readers, comic preparation for Kobo, comic metadata lookups, and ZIP, RAR and 7z bundle import.
- First-run setup covers connecting a Kobo, importing its books, settings, appearance and a short tour. Browser mock flags: `?setup` (with `&kobo=later` or `&kobo=none`), `?untested`, `?readonly`.
- Releases: a tag-driven workflow builds the universal Mac app and per-architecture CLI archives, publishes a GitHub release from CHANGELOG.md, redeploys the site in `site/` and updates the Homebrew formula. The app checks for signed updates and can download them in the background.
- Verification: prek runs quick checks on commit and builds, lints, type checks and tests on push. CI checks both Mac architectures, with the WKWebView suite split across two Apple silicon jobs, and Linux and Windows builds in the manual Portability workflow. Renovate opens weekly dependency PRs for manual review.

## Current

- 0.1.1 is released. Before the next release, add a `v*` tag rule to the `github-pages` environment, or the release's site redeploy is refused and has to be rerun from `main` ([releasing](engineering/releasing.md)). Releases are signed ad hoc until there's a Developer ID.

## Unresolved

- The hardware checklist tracks open Kobo verification. Only the Libra 2 has a physical-device record so far.
- Every Kindle hardware check is open. A friend's Kindle is available for a first run. The riskiest parts are MTP on macOS (photo import service, other apps holding the device), thumbnails and APNX on the device, and KRDS fields on current firmware.
- Check VoiceOver and reader input, the real desktop cover-upscale review flow, and authenticated Metron lookups.
- Real DRM-free Kindle files were checked against calibre. Combo MOBI and real KFX checks remain open. Remove the [vendored boko patch](vendor/boko/VENDORED.md) after upstream releases all four local fixes (boko#37, #39, #40 and the MOBI contents fix).

## Next

- Run the outstanding release hardware checks and collect reports from additional Kobo models.
- Bring the user-facing pages in `docs/` onto the site. Add app screenshots to the site's landing page.
- Run the Kindle hardware checks, then add tested models to `TESTED_MODELS` in `crates/device-kindle/src/models.rs`. Comics for Kindles, calibre's accurate APNX method and highlight text from KRDS positions are future Kindle work. Optional Hardcover metadata, Kobo position mapping, PDF comics, and additional comic processing remain future work.

## Postponed

- [Kobo compatibility check](engineering/plans/kobo-compatibility-check.md): lets users check and report untested Kobo models. Postponed until users ask for support for models we haven't tested.
