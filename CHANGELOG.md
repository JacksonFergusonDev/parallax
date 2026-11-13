# Changelog

What changed in each release of Parallax, for people using it. The release workflow publishes each version's section as its GitHub release notes, which the app's update banner and the site's download page show. Add entries under Unreleased as changes land; `just release` dates them.

## Unreleased

- Book details keep the cover and Read button aligned, group cover actions under Change cover, and show resolution advice below the cover.
- First-run setup is larger and easier to read, and you can move through it from the keyboard: Return continues, ⌘← goes back and ⌘. skips setup. Each button shows its shortcut.
- Reading opens straight away, without a loading screen, and keeps its place when you switch tabs or between Kobo and Goodreads. With a Kobo connected, it shows your last saved stats at once and updates them from the Kobo while you look.
- **Goodreads insights** in Reading › Goodreads, or View › Goodreads Insights, show what your Goodreads shelves say about your reading: books and pages read each year, this year so far against the same point last year, how you rate, book lengths, your to-read pile and how long it would take at your pace, the authors you read most, series with more on your shelves, when your books were first published and the editions on your shelves. To-read books already in your library are listed so you can start one. A table of every book can be searched, sorted and filtered, and each row opens to its dates, shelves, edition and review. The page says which export the books came from and when you imported it, and read books without a date on Goodreads are counted apart rather than left out. `parallax-cli goodreads stats` shows the same from your saved books, or straight from an export with `--file`, and `--json` gives every number.
- **Import from Goodreads** (File menu, and the Library tools menu) saves the CSV from Goodreads' My Books › Import and export. Import a newer export whenever you like and your books update in place without being added twice. If you changed a book here and Goodreads changed it too, or Goodreads no longer lists a book you saved, Parallax shows both sides in plain words and asks what to do. **Use recommended** settles everything in one click by keeping your changes and keeping those books. Nothing is deleted unless you choose Remove. The dialog opens straight to the file picker, shows what the export holds, including new shelves and how many of its books are in your library, and closes when the import is done. First-run setup offers it as an optional step of its own, with how to download your export from Goodreads. Library books on your Read and Currently reading shelves now show a Read or Reading badge and count in the Read filter, and the book panel lets you change a book's Goodreads status, rating and shelves. `parallax-cli goodreads show`, `import` and `saved` do the same from the terminal, with `--json` for scripts.
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
