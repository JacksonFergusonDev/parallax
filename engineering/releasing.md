# Releasing Parallax

A release is one pushed tag. [`.github/workflows/release.yml`](../.github/workflows/release.yml) reruns CI on the tagged commit, builds the app and the CLI, publishes a GitHub release, redeploys [the site](../site/) and updates the Homebrew formula. Nothing about a version is edited by hand outside `CHANGELOG.md`.

## Cutting a release

1. Add what changed for users under **Unreleased** in [`CHANGELOG.md`](../CHANGELOG.md), as changes land. These notes become the GitHub release notes, the update banner's **What's New** and the site's version list, so write them for users, following house-style's writing rules.
2. On `main`, run `just release 1.2.3`. It first checks, changing nothing, that the checkout is clean, matches `origin/main`, and that neither the changelog nor origin has the version yet, then runs `just ci`. Then it sets the version in `Cargo.toml`, `Cargo.lock`, `app/package.json` and `tauri.conf.json`, dates the changelog section, commits `chore(release): v1.2.3`, tags `v1.2.3`, and pushes `main` and the tag in one atomic push, which starts the release workflow. If any step fails or you interrupt it, it deletes the tag and resets to where it started. Add `--dry-run` to run only the checks, or `--no-push` to stop after tagging.

To check a release builds without publishing anything, run the Release workflow by hand from the Actions tab, or `gh workflow run release.yml --ref BRANCH`. That dry run checks the manifests agree, reruns CI, builds and signs the app and both CLIs, and keeps `latest.json` and the Homebrew formula as workflow artifacts. It creates no release, deploys no site and pushes nothing to the tap. Pull requests that change the release workflow or its scripts get a dry run automatically. GitHub only offers the manual run for workflows on `main`.

A version with a hyphen, such as `1.3.0-beta.1`, is published as a prerelease. It appears on the site's version list, but the download button, the in-app updater and Homebrew keep offering the newest stable version.

The workflow stops before publishing if the tag and manifests disagree, the changelog has no section for the version, or any CI job fails. The release stays a draft until every file is uploaded.

## What a release contains

| File | Used by |
| --- | --- |
| `Parallax_<version>_universal.dmg` | The site's download button. One app for Apple silicon and Intel, since a browser can't reliably tell them apart. |
| `Parallax_<version>_universal.app.tar.gz` and `.sig` | The in-app updater, which checks the signature against the public key in `tauri.conf.json`. |
| `latest.json` | The updater's manifest, read from `releases/latest/download/latest.json`, so prereleases never reach it. |
| `parallax-cli-<version>-<target>.tar.gz` and `.sha256` | Homebrew and `cargo binstall`. Each holds `parallax-cli`, the matching kepubify, and `LICENSE`. |

## Secrets

Set these under the repository's **Settings › Secrets and variables › Actions**.

| Secret | Needed for | Without it |
| --- | --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | Signing updates | The app build fails. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | The key's password | Leave unset; the current key has none. |
| `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY` | Developer ID signing | The app is signed ad hoc. macOS blocks it on first open until the user chooses **Open Anyway** in **System Settings › Privacy & Security**. |
| `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID` | Notarization (`APPLE_PASSWORD` is an app-specific password) | The app isn't notarized. |
| `TAP_GITHUB_TOKEN` | Pushing `Formula/parallax-cli.rb` to `JacksonFergusonDev/homebrew-tap` | The formula update is skipped with a warning. A fine-grained token with **Contents: read and write** on the tap only. |

### The update signing key

The key pair was generated with `pnpm --dir app tauri signer generate -w ~/.tauri/parallax-updater.key`. Its public key is in `app/src-tauri/tauri.conf.json` under `plugins.updater.pubkey`; the private key is stored as a secret with `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/parallax-updater.key`.

Keep a copy of the private key in a password manager. Every installed copy of Parallax trusts only this key, so losing it means existing users can't receive updates and must download the next version by hand. Never commit it.

### Developer ID signing

Signing needs an Apple Developer Program membership. Create a **Developer ID Application** certificate, export it with its private key from Keychain Access as a `.p12`, then store `base64 -i cert.p12 | gh secret set APPLE_CERTIFICATE`, its export password as `APPLE_CERTIFICATE_PASSWORD`, and the identity name (`Developer ID Application: Name (TEAMID)`) as `APPLE_SIGNING_IDENTITY`. Tauri imports it into a temporary keychain, signs the app, the kepubify sidecar and the disk image, and notarizes when the `APPLE_ID` secrets are set. Until then, releases are signed ad hoc and the site's download page shows how to open an app Apple hasn't notarized. Once releases are notarized, set `notarized` to `true` in `site/src/lib/site.ts` to remove those steps.

## The site

[`.github/workflows/site.yml`](../.github/workflows/site.yml) deploys `site/` to GitHub Pages when the site changes on `main`, when run by hand, and after each release. One-time setup:

1. **Settings › Pages › Source:** GitHub Actions.
2. A DNS `CNAME` record from `parallax.jacksonferguson.me` to `jacksonfergusondev.github.io`. `site/public/CNAME` names the domain.
3. After the first deploy, tick **Enforce HTTPS**.

## Installing the CLI

- Homebrew: `brew install jacksonfergusondev/tap/parallax-cli`. The formula installs the prebuilt archive and points `PARALLAX_KEPUBIFY` at the kepubify beside it. `brew install --HEAD` builds `main` from source against Homebrew's kepubify instead, and is the only option until the first release: the tap carries the `--head` formula from `scripts/homebrew_formula.py` until the release workflow first replaces it.
- Cargo: `cargo install --locked --git https://github.com/JacksonFergusonDev/parallax parallax-cli` builds from source. It needs kepubify on `PATH` (`brew install kepubify`) or `PARALLAX_KEPUBIFY` set to send books. `cargo binstall --git … parallax-cli` downloads the release archive instead.

The CLI isn't on crates.io. Publishing it would mean publishing all ten workspace crates, and crates.io ignores the `[patch]` that substitutes the [vendored boko](../vendor/boko/VENDORED.md), so a published build would lose its Kindle fixes. Revisit once boko releases them.
