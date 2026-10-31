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
- Existing triggers: only Kobo's `kobo_plus_asset_cleanup`. No seriesmeta/calibre triggers installed.
- `Shelf(CreationDate, Id PK, InternalName, LastModified, Name, Type, _IsDeleted, _IsVisible, _IsSynced, _SyncTime, LastAccessed)`.
- `ShelfContent(ShelfName, ContentId, DateModified, _IsDeleted, _IsSynced, PK(ShelfName, ContentId))`. Booleans are stored as the strings `'true'`/`'false'`.

## Library state
- 111 books total: 93 sideloaded KEPUBs (`application/x-kobo-epub+zip`, all under `KEpub/`), 18 store books.
- Sideloaded books live in genre subfolders (`KEpub/Sci-Fi`, `KEpub/Horror`, `KEpub/Fantasy`, `KEpub/Fiction`, …). Nickel ignores folders, so this organization isn't visible on the device. "Folder → collection" is an obvious first grouping rule and a migration path.
- **Zero collections** (`Shelf` and `ShelfContent` are empty). This makes the device a low-risk target for collection tests.
- Series is set on 1 of 93 sideloaded books (`SeriesID` = series name, as expected for sideloads).
- Reading data: ReadStatus 0/1/2 = 73/34/4. `___PercentRead`, `DateLastRead` and `TimeSpentReading` are populated for books that have been opened. Enough for MVP status badges.

## Confirmed hazards
- **AppleDouble ghosts are real here**: 68 `._*` files in `KEpub/` and **2 `._*` rows already imported into `content`** as bogus books. The app should (a) never create them (copy without xattrs), and (b) offer a cleanup of existing ones (delete the file plus its `content` row through the safe-write layer).
- `.Spotlight-V100`, `.fseventsd` and `.Trashes` exist on the volume, so macOS indexing has been active. Add `.metadata_never_index`.
