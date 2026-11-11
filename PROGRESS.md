# Progress

[Engineering index](engineering/README.md) · [Hardware checks](engineering/hardware-checks.md) · [Hardware results](engineering/hardware-results.md)

## Completed

- Implemented Kobo library management, native collections, reading stats, backups, and device reports. The reference Libra 2 results cover the workflows tested on each date.
- The desktop EPUB and comic readers, comic preparation for Kobo, comic metadata lookups, and ZIP, RAR, and 7z bundle import are implemented.
- Development scripts and fixture generators use locked Python tooling. CI checks both Mac architectures, with the WKWebView suite split across two Apple silicon jobs and Linux and Windows builds in the manual Portability workflow.
- Added first-run setup. It walks through connecting a Kobo, importing its books, settings (recommended, or customised), appearance and a short tour. It shows on each launch until skipped or finished, libraries from before it are marked done quietly, and Help › Welcome to Parallax reopens it. Browser mock: `?setup`, plus `&kobo=later` or `&kobo=none`.
- An untested or read-only Kobo is explained once per model, in setup's Connect step or as a banner in the main window with **Don't Show Again**. `TESTED_MODELS` in `crates/device-kobo/src/support.rs` lists models with hardware results, and **About This Kobo…** shows whether hardware checks have been run on that model. Browser mock: `?untested` or `?readonly`.
- Tuned Renovate to open weekly dependency PRs for manual review, group Tauri updates, and keep the scripts interpreter on Python 3.12 for Core ML tooling.
- Adopted TypeScript 7 for builds and type-checking with the TypeScript 6 API for ESLint. Registered GitHub's `ubuntu-26.04-arm` label with actionlint until the tool supports it directly.
- Added a theme-aware README wordmark, with editable branding SVGs and an Inkscape export script.
- The interface uses DM Sans only (house-style's), and copy follows house-style's writing rules.
- Added the site in `site/` (house-style, like Star Ground's), a tag-driven release workflow (universal Mac app, per-architecture CLI archives, GitHub release from CHANGELOG.md, site redeploy, Homebrew formula), and in-app update checks with signed updates. See [releasing](engineering/releasing.md).
- Release packaging runs alongside CI, with publishing gated on both. The macOS CI cache can reuse compiled dependencies across lockfile changes on the same runner image and Rust toolchain.
- The WKWebView search scenarios wait for the reader to open and for a search result before navigating, so a slow launch or search can't skip those steps.
- Moved slower checks to prek's pre-push stage, added app build and site checks to the hooks, and limited release preflight to release eligibility. Changelog-only releases still trigger the project-wide lint, type and test checks.
- Compact controls in the Library toolbar and filter row free up vertical space for books. The site's library snapshot reflects the new layout.
- The Reading tab has more space below the toolbar, with a smaller inset at narrow window widths.
- Library and On Kobo request thumbnails when cards and rows mount so covers are ready sooner during fast scrolling.

## Current

- Released 0.1.0: the DMG, update archive and CLI archives are on GitHub, the site lists it, and `brew install jacksonfergusondev/tap/parallax-cli` installs it. Next release: add a `v*` tag rule to the `github-pages` environment first, or the release's site redeploy is refused and has to be rerun from `main` ([releasing](engineering/releasing.md)). Releases are signed ad hoc until there's a Developer ID. The engineering index consolidates contributor references and links to the pre-release research and plans in Git history. `docs/` holds user-facing material to bring onto the site.

## Unresolved

- The hardware checklist tracks open Kobo verification. Only the Libra 2 has a physical-device record so far.
- Check VoiceOver and reader input, the real desktop cover-upscale review flow, and authenticated Metron lookups.
- Real DRM-free Kindle files were checked against calibre. Combo MOBI and real KFX checks remain open. Remove the [vendored boko patch](vendor/boko/VENDORED.md) after upstream releases all three local fixes.

## Next

- Run the outstanding release hardware checks and collect reports from additional Kobo models.
- Bring the user-facing pages in `docs/` onto the site. Add app screenshots to the site's landing page.
- Kindle device support is deferred. Optional Hardcover metadata, Kobo position mapping, PDF comics, and additional comic processing remain future work.

## Postponed: Kobo compatibility check

Postponed until users ask for support for Kobo models we haven't tested. Until then, the untested-model notice and the existing device report (**About This Kobo… › Save device report…**) cover it. The goal is to add models without owning them: Parallax gathers evidence from the user's Kobo, mostly without writing to it, and the user reviews and sends a report.

### Where it appears

- Setup's Connect step, as a secondary "This Kobo hasn't been tested yet — check it (2 min)" next to the existing untested notice. It never blocks setup.
- The main-window untested banner gains **Check Compatibility…**. Also add **Kobo › Check Compatibility…** and a button in **About This Kobo…**.
- Remember the outcome per model and firmware, so a firmware update can prompt again.

### Tier 1: read-only comparison (automatic, no writes)

- Extend `DeviceReport` (`crates/device-kobo/src/report.rs`) with a schema fingerprint: the tables and columns writes use, their declared and stored types, and the boolean storage (text or integer).
- Ship the Libra 2 fingerprint (schema 174, firmware 4.38) as the reference and compare against it. Show the result in plain words: "stores books the same way as tested models" or "differs in these places".
- Record how sideloaded books are stored, the ContentType and ContentID patterns, and whether the collection and series columns are present.

### Tier 2: rehearsal on a copy (no risk to the device)

- Copy `.kobo/KoboReader.sqlite` with its WAL into a temporary directory on the Mac. Never open the device database read-write.
- Run the real `DeviceWriter` paths against the copy: send a bundled public-domain test book's database rows, add it to a collection, set series, mark it read, remove it, and restore from a backup. Then run `PRAGMA integrity_check` and Parallax's own consistency checks (a collection row for every member, no orphaned ContentIDs).
- Record each step's outcome and error. Delete the copy afterwards.
- This needs `DeviceWriter` to accept a database path separate from the mount for file operations, or a mode that writes rows without copying files. Design it so the guarded device APIs stay the only route to a real device.

### Tier 3: guided test on the device (optional, explicit consent)

- Explain the risk honestly: one disposable test book in a temporary collection, the usual pre-change database backup, and Revert available. Bounded risk, not no risk.
- Steps: send the bundled test book to a "Parallax test" collection, then ask the user to eject. Ask them with buttons whether the book shows on the Kobo, in the collection, with its cover and series (optionally attaching a photo). Then ask them to reconnect, check the Kobo imported it (ContentID rows present and no pending state), remove the book and the collection, eject, and confirm both are gone.
- Before release, run it on the reference Libra 2 and record the result in `engineering/hardware-results.md`.

### Report, consent and sharing

- The consent screen summarises the findings ("these tests showed …"), lists every field that would be shared, and has **View full report** for the raw JSON. It contains no serial, titles, authors, paths, reading data or account details, as in the current device report. It adds the fingerprint difference, the rehearsal results and the guided-test answers.
- **Send Report** (primary) posts the JSON to a Cloudflare Worker relay, which needs no account:
  - The Worker validates it against the report schema (format version, field types, size cap of about 256 KB) and rate-limits by IP without storing the IP.
  - It files a GitHub issue from a bot account with a fine-grained token kept as a Worker secret and scoped to issues on this repository. It labels the issue `kobo-report` with the model and firmware in the title, and returns the issue number as a reference for the user.
  - It accepts an optional contact email, blank by default and never published in the issue. If one is given, store it privately (for example, email it to the maintainer instead of putting it in the issue).
  - The Worker source, `wrangler.toml` and a short privacy note (what's received, what's kept, no IP logging) live in the repository. The app's request goes to one fixed HTTPS endpoint, and its failures fall back to the options below.
- Fallback links: **Save Report…**, **Email Instead…** (a `mailto:` with subject and body filled in, with the saved file shown in Finder to attach) and **Share on GitHub** (a pre-filled issue form, `.github/ISSUE_TEMPLATE/kobo-report.yml`).

### Turning reports into support

- Maintain a list of known-good fingerprints (model, firmware range, schema) shipped with the app. A Kobo whose fingerprint matches a confirmed one is shown as "community-verified" rather than untested. Keep it separate from `TESTED_MODELS`, which stays reserved for hardware the maintainer has checked.
- List community-verified models in `docs/device-support.md` separately from the Libra 2 hardware results.
- A rehearsal failure points to the exact write that needs adapting. Fixes still go through code review, fixtures built from the shared schema, and the hardware checklist where possible.
