# Changelog

What changed in each release of Parallax, for people using it. The release workflow publishes each version's section as its GitHub release notes, which the app's update banner and the site's download page show. Add entries under Unreleased as changes land; `just release` dates them.

## Unreleased

- **Check for new Kobo software** in Settings › Kobo asks Kobo, when your Kobo connects, whether newer software is available for it, as Kobo Utilities does, and tells you how to install it. Kobo is told your model and software version, not your serial number. It's off until you turn it on. `parallax-cli kobo firmware` checks from the terminal.
- **Save to Disk…** (File menu, ⇧⌘S, and Library tools) saves the selected books, or every book shown, with your edits and covers, as calibre's save to disk does. Settings › Library sets how files are named, such as a folder for each author with series books numbered, and can save everything into one ZIP archive instead, with a choice of compression. `parallax-cli library save` does the same.
- **Add automatically** in Settings › Library watches a folder you choose, such as Downloads, and adds books saved there to your library while Parallax is open, as calibre's auto-add folder does. It waits for downloads to finish and adds each file once. You can also have added files moved to the Trash. It's off until you choose a folder.
- **Reading layout** in Settings › Kobo sets a font, size, line spacing, margins and alignment on books, as Kobo Utilities does, so they open the way you like without adjusting each one on your Kobo. Select books in On Kobo and choose **Set layout**, or turn on **Set this layout on new books** to have books without settings of their own get it when your Kobo connects. It's off until you set it up.
- When you send a book again, or send it to another Kobo, it opens where you last were rather than at the start, as Kobo Utilities' restore bookmark does. Parallax saves your place each time a Kobo connects and puts it back once the Kobo has imported the new copy. A book you've opened or marked unread since is left alone. Turn it off in Settings › Kobo.
- Settings › Kobo › Sending books changes books as they go to your Kobo: hyphenate always or never, smarten quotes and dashes, add fixes for reading in full screen, and add your own CSS to every book, as calibre's kobo_extra.css does. Each is off until you turn it on.
- Covers sent to a black-and-white Kobo stay in colour again, still sized to its screen. The grey treatment from 0.1.2 is off until it's been compared on a Kobo.

## 0.1.2 (2026-11-13)

- Book and comic covers sent to your Kobo are sized to its screen, so a large cover is smaller to copy and quicker for your Kobo to process. On a black-and-white Kobo, colour covers get the same grey treatment as colour comic pages, which keeps shapes and contrast that plain greyscale washes out. Your library keeps the original cover, and On Kobo shows it in full colour.
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
