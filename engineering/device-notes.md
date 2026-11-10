# Kobo engineering notes

Kobo exposes its storage over USB with the database at `.kobo/KoboReader.sqlite` and the model and firmware in `.kobo/version`. The [Libra 2 report fixture](../tests/fixtures/kobo/reports/libra2-fw4.38-db174.json) records firmware 4.38.23697 and schema 174 without library rows. The optional real-database regression test uses a local copy at `tests/fixtures/private/KoboReader-libra2-fw4.38-db174.sqlite`, which is ignored by Git. See [device support](../docs/device-support.md) for the user-facing compatibility policy.

## Database and collections

- Read from the mounted database through the immutable connection in [db.rs](../crates/device-kobo/src/db.rs). On the reference Libra 2, Nickel checkpointed its WAL before exposing the volume over USB. Opening a bare copy with `sqlite3 -readonly` failed when the side files were absent.
- Book rows use `ContentType=6`. Sideloaded IDs are usually `file:///mnt/onboard/<path>`. Preserve literal path matching. SQLite's `LIKE` treats underscores as wildcards.
- Dates are UTC without a zone suffix. Sideloaded rows can have an empty `DateAdded`, so `DateCreated` provides the date the file arrived. `TimeSpentReading` is in seconds.
- Nickel imports a newly transferred book after ejecting. Write its `Shelf` and `ShelfContent` membership before import, using the known content ID. Series lives on the imported `content` row and is reconciled on the next connection when enabled.
- Preserve Nickel's cloud sync semantics. Soft-delete synced collections through the existing collection operations. The [dated hardware results](hardware-results.md) record why Parallax uses pre-import membership and soft deletion.

## Write safety and backups

Use the guarded device APIs and [DeviceWriter](../crates/device-kobo/src/writer.rs) for database changes. The writer checks support and database health, takes a backup, and applies transactional changes. It verifies integrity, checkpoints the WAL, and closes the database. A failed integrity check restores the pre-change backup.

macOS may create FAT32 AppleDouble companions (`._<name>`) for new files even when their contents were copied without extended attributes. Remove companions for files Parallax writes through the existing transfer helper. The reference device did not import existing dotfiles as books. The earlier apparent matches came from a SQL wildcard.

Database backups live under `<app data>/device-backups/<serial>/` as `KoboReader-<stamp>.sqlite.zst`, with a JSON sidecar for the operation and database version. Copies of removed book files are kept under `removed/<stamp>/` and retained according to the backup policy. See [backups.rs](../crates/device-kobo/src/backups.rs).

## Device records

Parallax writes its on-device records under `/.parallax/` during changes. Reads have no side effects. See [state.rs](../crates/device-kobo/src/state.rs) for the format.

- `device.json` stores device identity and first and last write stamps.
- `books.json` tracks Parallax-managed paths, library links, hashes, sizes, and write stamps.
- `log.jsonl` records operations, associated backups, and errors. The log holds up to 2,000 entries.

## Add a device report

1. Place the JSON under `tests/fixtures/kobo/reports/` with a name that includes the model, firmware, and database version.
2. Run `cargo test --locked -p parallax-device-kobo --test support reported_devices_replay`. The test rebuilds the schema, checks the write verdict, and exercises writes against the fixture when the device is writable.
3. Inspect the model ID, `value_types`, and `cover_files`. Update [screen.rs](../crates/device-kobo/src/screen.rs) for a new profile and [support.rs](../crates/device-kobo/src/support.rs) when evidence warrants changing a guard. Add coverage for the difference and update the expected verdict or collect a new report.
4. Ask someone with that Kobo to run the relevant [hardware checks](hardware-checks.md). Record fixture replay and observed Nickel behaviour separately.
