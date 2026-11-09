# Device support

Which Kobos Parallax changes, how that is enforced, and how a Kobo it doesn't know yet gets added.

## What decides support

Reading (listing books, collections, covers, highlights, adding books to the library) works on any Kobo whose database is schema 53 or newer.

Changing a Kobo (sending, updating or removing books, collections, series, reading status, deleting cached covers) needs every one of these, checked by `KoboDevice::check_writable` and inside `open_writer` before any write (`crates/device-kobo/src/support.rs`):

| Check | Why |
|---|---|
| Database schema 53–222 | The range calibre's KoboTouch driver has validated writes against. |
| Not a Tolino reader (model ids 690, 691, 693) | Tolino readers run Kobo's software but store yes/no values as numbers and name cached covers differently (calibre `needs_real_bools`, `imageid_from_contentid`). |
| Firmware 4.x or older | Kobo's own firmware has been 4.x since 2016; 5.x is Tolino's. |
| `content`, `Shelf` and `ShelfContent` have the columns ERC writes | A schema missing one is something ERC hasn't seen. |
| Yes/no columns hold `'true'`/`'false'` text | Every write uses text; mixing in 0/1 could confuse Nickel. |

A Kobo that fails one is read-only: the app shows why, disables the buttons that change it, and the backend refuses those changes anyway. The hardware model alone doesn't block changes, because ERC only edits the database and copies files, which don't depend on the screen or chip; the model matters only when it identifies a Tolino reader.

Optional columns turn off a feature rather than the device: series (schema 65+; `SeriesID`/`SeriesNumberFloat` 136+), reading time (168+), shelf `Id`/`Type`, bookmark `Type`. Writes skip the columns a schema lacks.

## Screens

Each Kobo has a screen profile (size in portrait pixels, colour or not) that comics are prepared for, shown in About this Kobo. Known model ids, 310 to 393 plus the Tolino readers, are listed in `crates/device-kobo/src/screen.rs`, with sizes from Kobo's specifications. An unknown model is measured from its own full-screen covers (`N3_FULL` in `.kobo-images`, drawn at screen height), and otherwise assumed to be 1072 × 1448, the most common size. About this Kobo says when a size was measured or assumed.

Not guarded because they are safe on any Kobo: deleting macOS `._` files (macOS created them) and ejecting.

## Device reports

Someone with a Kobo ERC doesn't know can save a device report:

- App: On Kobo → About this Kobo… → Save device report…
- CLI: `erc kobo report --out report.json`

The report is JSON: model id, firmware, database version, the database's `CREATE` statements (no rows), how yes/no values are stored, row counts per table, content and bookmark type counts, cached-cover kinds with their pixel sizes, the device's dot folders, and ERC's verdict. It has no serial number, titles, authors, file or folder names, reading data, highlights or account details. Saving it only reads the Kobo. They send the file however they like (email, a message); nothing is uploaded.

## Adding a Kobo from a report

1. Put the report in `tests/fixtures/kobo/reports/` (name it like `libra2-fw4.38-db174.json`).
2. Run `just test`. `reported_devices_replay` (`crates/device-kobo/tests/support.rs`) rebuilds that database from its schema, checks ERC's verdict matches the report's, and, when the Kobo is writable, makes every kind of change ERC makes and checks the database's integrity afterwards.
3. Read the report: a new `model_id` gets a name and screen in `screen::MODELS`; `value_types` shows integer yes/no values; `cover_files` gives the screen's height (`N3_FULL`).
4. If it's read-only for a reason that turns out to be safe, change the check in `support.rs` with a test, and update the report's `read_only` (or ask for a new report).
5. Before calling it supported, someone with that Kobo runs the relevant `docs/hardware-checks.md` items. A passing replay shows ERC's SQL fits the schema; it doesn't show how Nickel reacts.

Reports so far:

| Report | Device | Verdict |
|---|---|---|
| `libra2-fw4.38-db174.json` | Kobo Libra 2, firmware 4.38.23697, schema 174 (reference device) | writable |
