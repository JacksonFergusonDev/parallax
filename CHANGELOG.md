# Changelog

What changed in each release of Parallax, for people using it. The release workflow publishes each version's section as its GitHub release notes, which the app's update banner and the site's download page show. Add entries under Unreleased as changes land; `just release` dates them.

## Unreleased

- The first release: a library for books and comics, a reader for each, and sending to a Kobo as KEPUB with collections, series and reading progress.
- Every change to the Kobo's database is backed up first, and **Revert changes…** goes back to any saved point.
- Parallax checks for new versions and asks before installing one. **Check for Updates…** in the Parallax menu checks now.
- `parallax-cli` installs with Homebrew or Cargo.
