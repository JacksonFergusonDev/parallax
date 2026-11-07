# Device notes — reference Kobo (inspected 2026-10-30, read-only)

## Device

- Kobo Libra 2, `.kobo/version` model id `...0388`, firmware **4.38.23697**.
- Mounts at `/Volumes/KOBOeReader`, FAT32 (`msdos`, via macOS fskit), USB, ~29 GiB.
- Signed in to a Kobo account (`user` has 1 row); cloud sync active (`LastSyncTime` set in `Kobo eReader.conf`).
- Previously managed by calibre 9.16 (`driveinfo.calibre`, `metadata.calibre` at the volume root).

## KoboReader.sqlite

- `DbVersion` = **174**. 38 MB. `PRAGMA integrity_check` = ok.
- **`journal_mode=wal`**. No `-wal`/`-shm` files exist while in USB mode (Nickel checkpoints before exposing the volume). Any desktop write creates `-wal`/`-shm` on the device, so a write session must end with `PRAGMA wal_checkpoint(TRUNCATE)` and a clean close so nothing is left beside the DB. `sqlite3 -readonly` on a bare copy fails (SQLITE_CANTOPEN), so reads should use a copy opened read-write or `immutable=1` URI.
- `content`: 101 columns. Books are `ContentType=6`.
- calibre's book filter (`BookID IS NULL AND ((Accessibility=-1 AND IsDownloaded IN ('true',1)) OR (Accessibility IN (1,2,8,9) AND IsDownloaded IN ('true',1)))`) yields the 93 sideloaded books; the 18 store books are `IsDownloaded='false'` (in the cloud, not on the device).
- Dates are UTC without a zone suffix (`2024-10-10T06:36:38.327`). For sideloads `DateAdded` is empty; `DateCreated` is when the file arrived ("on device since"). `TimeSpentReading` is in seconds.
- Existing triggers: only Kobo's `kobo_plus_asset_cleanup`. No seriesmeta/calibre triggers installed.
- `Shelf(CreationDate, Id PK, InternalName, LastModified, Name, Type, _IsDeleted, _IsVisible, _IsSynced, _SyncTime, LastAccessed)`.
- `ShelfContent(ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced, PK(ShelfName, ContentId))`. Booleans are stored as the strings `'true'`/`'false'`.

## Library state

- 111 books total: 93 sideloaded KEPUBs (`application/x-kobo-epub+zip`, all under `KEpub/`), 18 store books.
- Sideloaded books live in genre subfolders (`KEpub/Sci-Fi`, `KEpub/Horror`, `KEpub/Fantasy`, `KEpub/Fiction`, …). Nickel ignores folders, so this organization isn't visible on the device. "Folder → collection" is an obvious first grouping rule and a migration path.
- **Zero collections** (`Shelf` and `ShelfContent` are empty). This makes the device a low-risk target for collection tests.
- Series is set on 1 of 93 sideloaded books (`SeriesID` = series name, as expected for sideloads).
- Reading data: ReadStatus 0/1/2 = 73/34/4. `___PercentRead`, `DateLastRead` and `TimeSpentReading` are populated for books that have been opened. Enough for MVP status badges.

## ERC records (`/.erc/`, M10)

Written only during ERC changes, never on connect. All writes are temp-file + rename, and `._` companions are removed.

- `device.json`: `device_id` (UUID ERC assigns on its first write), `serial`, `first_write` and `last_write` stamps (`at`, `erc_version`, `firmware`, `db_version`).
- `books.json`: every book file ERC wrote: `content_id`, `path`, `library_book` (desktop library id, when sent from the library), `sha256` and `size` at write time, `first_written`, `last_written`, `erc_version`, `writes`. Removed books are dropped. A size mismatch means something else changed the file.
- `log.jsonl`: one JSON object per operation (`at`, `erc_version`, `op` = send/replace/remove/collection/series/tidy/restore, `content_ids`, `detail`, `backup` = the database backup file taken first, `error` when the write session failed). Newest last, capped at 2000 entries.
- An unreadable file is renamed to `<name>.unreadable-<timestamp>` and rebuilt; recording never fails the change itself.

## Backups (on the computer, not the Kobo)

- `<app data>/device-backups/<serial>/KoboReader-<stamp>.sqlite.zst`: the database before a change, each compressed on its own, with `KoboReader-<stamp>.json` beside it (firmware, database version, what changed). To open one by hand: `zstd -d <file>`.
- `removed/<stamp>/`: book files that change removed, for 30 days. See `crates/device-kobo/src/backups.rs`.

## Confirmed hazards

- **AppleDouble files**: 68 `._*` files in `KEpub/`, but **none imported into `content`** (fw 4.38 skips dotfiles; an earlier count of 2 was a `LIKE` false positive, since `_` is a wildcard: use `instr()` or escape the `_`). The app should still never create them (copy without xattrs) and can offer to tidy existing ones as a low-priority cleanup.
- **Every new file or folder written from macOS gets a `._<name>` companion** on this volume: macOS attaches `com.apple.provenance`, which cannot be removed, and the msdos/fskit driver stores xattrs as AppleDouble files. Deleting the `._` file after writing works and it is not recreated (probed 2026-10-30). This is where the existing 68 `._*` files came from.
- `.Spotlight-V100`, `.fseventsd` and `.Trashes` exist on the volume, so macOS indexing has been active. Add `.metadata_never_index`.
