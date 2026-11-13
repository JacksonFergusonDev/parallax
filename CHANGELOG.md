# Changelog

What changed in each release of Parallax, for people using it. The release workflow publishes each version's section as its GitHub release notes, which the app's update banner and the site's download page show. Add entries under Unreleased as changes land; `just release` dates them.

## Unreleased

- **Import from Goodreads** (File menu, and the Library tools menu) saves the CSV from Goodreads' My Books › Import and export. Import a newer export whenever you like and your books update in place without being added twice. If you changed a book here and Goodreads changed it too, or Goodreads no longer lists a book you saved, Parallax shows both sides in plain words and asks what to do. **Use recommended** settles everything in one click by keeping your changes and keeping those books. Nothing is deleted unless you choose Remove. The dialog opens straight to the file picker, shows what the export holds, including new shelves and how many of its books are in your library, and closes when the import is done. First-run setup offers it as an optional step when you add books. Library books on your Read and Currently reading shelves now show a Read or Reading badge and count in the Read filter, and the book panel lets you change a book's Goodreads status, rating and shelves. `parallax-cli goodreads show`, `import` and `saved` do the same from the terminal, with `--json` for scripts.
- **Update on Kobo** now works for a comic that was on your Kobo as its CBZ file before you added it to your library. It changes the title, author and series there and keeps your place. Parallax tells you first that the pages and cover stay as they are, and that removing the comic and sending it again gets you a copy prepared for your Kobo but starts it from the beginning.

## 0.1.1 (2026-11-11)

- Settings › General can download new versions in the background. Parallax then tells you the update is ready, and installs it when you choose **Restart to Update**.
- The app icon keeps its full gold outline on macOS 26 and later, where it was trimmed at the edges and corners.
- The disk image opens in an arranged window, with Parallax beside a shortcut to Applications on its background.

## 0.1.0 (2026-11-11)

- The first release: a library for books and comics, a reader for each, and sending to a Kobo as KEPUB with collections, series and reading progress.
- Every change to the Kobo's database is backed up first, and **Revert changes…** goes back to any saved point.
- Parallax checks for new versions and asks before installing one. **Check for Updates…** in the Parallax menu checks now.
- `parallax-cli` installs with Homebrew or Cargo.
