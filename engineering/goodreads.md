# Goodreads import

Goodreads has no public API, so the CSV from My Books › Import and export is the only supported source. The parser and merge live in `crates/core/src/goodreads.rs`, storage in `crates/library/src/goodreads.rs`, and the dialog in `app/src/components/GoodreadsImport.tsx`.

## Storage and re-import

Each saved book is one row in `goodreads_books`, keyed by the Goodreads Book Id. A row holds `data` (what you see), `base` (the last export's copy) and `local_only` (kept after a later export dropped it). Importing a newer export merges three ways per editable field (status, rating, date read, times read, your shelves, review):

- Only the export changed it: take the export's value.
- Only you changed it: keep yours.
- Both changed it differently: ask, per book, to keep yours or use Goodreads'.

Other metadata always follows the export. After a merge `data` and `base` match, so importing the same file again changes nothing. The whole import is one SQLite transaction. An export with no books is refused so a wrong file can't flag everything as missing. Books missing from a newer export are kept as yours or removed, by the user's choice. `Resolutions::default()` is the recommended choice, keeping yours and keeping missing books.

## Linking to library books

`library_matches` links saved books to library files without touching the EPUBs: by ISBN first, then by the duplicate finder's normalised title and author surname. The match is computed on demand by `goodreads_matches`, never stored, so it follows library edits. The library shows a Read or Reading badge from it, and a Kobo's own progress takes priority.

## Editing

`goodreads_edit` changes status, rating, date read and shelves for a saved book. Marking a book read sets the read count to at least 1 and dates it today if it had no date. These edits are what the next import asks about when Goodreads changed the same field.

## Not built

A separate works, shelves and read-log model for read-through history, tracking books that aren't on Goodreads, and writing shelves to Kobo collections.

Fixtures come from `scripts/make_goodreads_fixture.py`, which anonymises a real export; see `tests/fixtures/goodreads/README.md`.
