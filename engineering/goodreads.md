# Goodreads import

Goodreads has no public API, so the CSV from My Books › Import and export is the only supported source. The parser and merge live in `crates/core/src/goodreads.rs`, storage in `crates/library/src/goodreads.rs`, and the dialog in `app/src/components/GoodreadsImport.tsx`.

## Storage and re-import

Each saved book is one row in `goodreads_books`, keyed by the Goodreads Book Id. A row holds `data` (what you see), `base` (the last export's copy) and `local_only` (kept after a later export dropped it). Importing a newer export merges three ways per editable field (status, rating, date read, times read, your shelves, review):

- Only the export changed it: take the export's value.
- Only you changed it: keep yours.
- Both changed it differently: ask, per book, to keep yours or use Goodreads'. A date read that was filled in when you marked the book read here isn't a choice of yours, so the export's date replaces it without asking.

Other metadata always follows the export. After a merge `base` is the new export and `data` is the merged result, which keeps any changes of yours, so importing the same file again changes nothing. The whole import is one SQLite transaction. An export with no books is refused so a wrong file can't flag everything as missing. Books missing from a newer export are kept as yours or removed, by the user's choice. `Resolutions::default()` is the recommended choice, keeping yours and keeping missing books.

## Linking to library books

`library_matches` links saved books to library files without touching the EPUBs: by ISBN first, then by the duplicate finder's normalised title and author surname. The match is computed on demand by `goodreads_matches`, never stored, so it follows library edits. The library shows a Read or Reading badge from it. A Kobo that has opened the book takes priority with its own progress; a Kobo's "unread" doesn't, since it only means that device hasn't opened the book.

## Editing

`goodreads_edit` changes status, rating, date read and shelves for a saved book. Marking a book read sets the read count to at least 1 and dates it today if it had no date, flagged as guessed (`date_read_guessed`) so a later export's real date wins. Goodreads' own statuses can't be added as your shelves; change the status instead. These edits are what the next import asks about when Goodreads changed the same field.

## Insights

`crates/core/src/goodreads_insights.rs` works out the Reading › Goodreads page from the saved books (as you see them, with your edits), the library matches and the last import record, on demand and without writing. `compute` is the pure part; `insights` adds the library and import. The app's `goodreads_insights` command and `parallax-cli goodreads stats` both call it. The app only words and lays out the numbers (`app/src/goodreadsInsights.ts`), and books refer to each other by Goodreads Book Id within `Insights::books`.

Things real exports taught it:

- Goodreads keeps only the latest date read, and many read books have none. Anything over time counts each dated book once and reports the undated ones (`read_undated`) instead of dropping them.
- Exports from 2026 have no Average Rating column. It's parsed when present (`Book::average_rating`), and the comparison with other readers only shows with five or more books that have it.
- Goodreads often leaves the `(Series, #N)` suffix off manga volumes ("Glass Harbor, Vol. 10", "ガラスの港 3 [Glass Harbor 3]"), and one series can be written with different apostrophes. Series progress groups names by `series_key` (case and apostrophe style folded) and reads a volume from such titles. The parser is unchanged, so stored books and merging aren't affected. A lone unmarked "Vol. 2" isn't treated as a series.
- "Missing" volumes are whole numbers below the highest one read that aren't on any shelf; another edition or box set of books already read isn't counted as more to read.

Each import is recorded in `goodreads_imports` with the file's name, its modified time and the latest date added or read in it, since Goodreads doesn't date exports. Migration 18 gives books saved before it one record from what they hold. `goodreads stats --file` reads an export without saving it and only opens a library when `--library` is given, so trying it never migrates the app's library.

## Not built

A separate works, shelves and read-log model for read-through history, tracking books that aren't on Goodreads, and writing shelves to Kobo collections.

Fixtures come from `scripts/make_goodreads_fixture.py`, which anonymises a real export; see `tests/fixtures/goodreads/README.md`.
