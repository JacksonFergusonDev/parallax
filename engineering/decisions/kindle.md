# Kindle support

Parallax supports Kindles by cable, as a drive or over MTP, and by Send to Kindle email. It was built without a Kindle, so this records where each behaviour comes from and what still needs a device. The procedures are in [Kindle hardware checks](../kindle-hardware-checks.md).

## What a Kindle allows

A Kindle keeps its library, collections and read status in `cc.db` on its system partition, which USB doesn't expose. Over USB, Parallax can only add and remove files in `documents/` and read the files the Kindle writes beside books. So, unlike a Kobo:

- There are no collection changes. Firmware before 5 kept a read-only cache in `system/collections.json`, which Parallax shows if it's there.
- Read status and progress are estimates from sidecar files, not the Kindle's own values.
- Nothing needs a backup, because Parallax never writes the Kindle's own data. A wrong write leaves a book that doesn't appear or has a generic cover.

## Code layout

- `crates/device-kindle` is the backend: storage, detection, models, headers, sidecars, sending and removal. It follows calibre's Kindle and MTP drivers (`devices/kindle/driver.py`, `devices/kindle/apnx.py`, `devices/mtp/driver.py`), which are cited where ported.
- `crates/core/src/kindle.rs` holds library workflows: send, update, remove, import, progress and highlight sync. `kindle_email.rs` holds email.
- The app routes the existing device commands to these when the mount is a Kindle (`app/src-tauri/src/kindle.rs`). The UI reuses the device view and names the device by brand, hiding Kobo-only tools.
- `parallax-cli kindle` covers the same workflows.

## Storage: drive and MTP behind one interface

Kindles before 2024 mount as a FAT drive. The Scribe since firmware 5.16.3, 2024 and later Paperwhites and the Colorsoft use MTP instead, which has no mount point and addresses files by handle. `storage::Storage` is a small file interface (list, read, read a prefix, write, remove, make folders, free space) over paths relative to the storage root. `DriveStorage` and `MtpStorage` implement it, so everything else is written once. An MTP Kindle's mount in the UI is the key `mtp:<serial>`.

Writes never leave a partial file under the final name. On a drive, a write goes to a temporary file that's renamed. MTP can't replace in place, so `MtpStorage` uploads beside the old file and swaps them by rename when the device supports rename, and otherwise deletes first, as calibre does.

## MTP library: mtp-rs

[mtp-rs](https://github.com/vdavid/mtp-rs) is a pure-Rust MTP client over `nusb`, licensed MIT or Apache-2.0. It was chosen because:

- It lists the Kindle Paperwhite 12th generation (2024) as fully supported, and handles a Kindle quirk: the Kindle rejects OpenSession with a non-zero transaction ID.
- It needs no C library. calibre uses libmtp through its own C extension, and libmtp's device table only lists Fire tablets under Amazon's vendor ID. libmtp also has [reported conflicts with macOS's USB ownership](https://dev.to/hiyoyok/mtp-file-transfer-in-rust-on-macos-why-i-wrote-my-own-stack-3obg).
- Its `virtual-device` feature serves a folder over the real MTP code path, which is how the MTP path is tested without hardware.

The alternatives were `libmtp-sys` (unmaintained for six years, needs libmtp and libusb) and `mtpkit` (version 0.1, LGPL). mtp-rs is async and runtime-agnostic; `MtpStorage` blocks on it with `futures-lite` so the backend stays synchronous like the Kobo one. MTP allows one connection per device, so the app keeps open connections in `detect::Connections` and reuses them across polls. An ejected Kindle isn't reopened until it's unplugged.

### macOS's photo import service

macOS's `ptpcamerad` claims MTP and PTP devices as they connect, which locks other apps out. Even calibre has [unresolved reports](https://www.mobileread.com/forums/showthread.php?p=4470957) of 2024 Kindles failing on Macs. When opening a Kindle fails with exclusive access, Parallax stops `ptpcamerad` with `pkill` and retries for about 1.5 seconds. It runs as the user, so no password is needed, and launchd starts it again when something needs it. **Settings › Kindle › Free your Kindle from photo import** turns this off. Apps that hold the device themselves, such as Amazon's Send to Kindle, Android File Transfer and OpenMTP, aren't stopped; the app tells the user to quit them.

## Detection and models

A drive counts as a Kindle when it has `documents` and `system` folders and is named Kindle, or an Amazon USB device (vendor 0x1949) that isn't offering MTP is connected. The serial comes from USB through `nusb`. MTP Kindles are found by vendor ID without opening them. Models come from the serial number using KOReader's device codes (`frontend/device/kindle/device.lua`), with published screen sizes. Unknown serials fall back to the USB product name.

## Sending

Library books are exported as EPUB with their current details, converted to AZW3 by boko in a child process (like the Amazon importer), and written to `documents/Title - Author.azw3`, calibre's default naming. AZW3 rather than KFX: sideloaded KFX normally comes from Amazon's Kindle Previewer, and boko's KFX output hasn't been checked on a device. Comics aren't sent; a Kindle doesn't read CBZ, and preparing comics for Kindle screens is future work.

Two files go beside each book, as calibre writes them:

- **Cover thumbnail.** A Kindle shows `system/thumbnails/thumbnail_{ASIN}_{cdetype}_portrait.jpg` for a sideloaded book, from EXTH 113 and 501. boko wrote the same placeholder ASIN into every book, so all books shared one thumbnail; the vendored copy now uses the book's UUID or a derived one ([boko#40](https://github.com/zacharydenton/boko/pull/40)). Amazon replaces these thumbnails with generic ones when the Kindle goes online, so each is also kept in `amazon-cover-bug/` and restored on every connect, with an index file per book so removal can find it.
- **Page numbers.** An APNX file in `documents/<name>.sdr/`. Page starts follow calibre's "accurate" method: `text.rs` decompresses the book's PalmDOC text records, a line starts at each paragraph and after every 70 characters of paragraph text, and every 32nd line starts a page. This tracks print pages more closely than calibre's default "fast" method, a page every 2,300 characters, which is still used when the text can't be read (HUFF/CDIC compression, or a book with no `<p>` tags). calibre's "pagebreak" method isn't ported.

Updating a book writes the same file name, so the Kindle keeps its `.sdr` folder and with it the reading position and highlights.

## Reading progress and highlights

The Kindle writes reader data store (KRDS) files in each book's `.sdr` folder. The parser is ported from John Howell's `krds.py` (GPL v3), with the tolerance for newer firmware fields from zevisvei/kindle-reading-dashboard, which documents firmware 5.18 and 5.19. Progress is the last position read divided by the text length in the book's header, since positions in MOBI and KF8 books are byte offsets into the text. When that isn't available, the reading timer's share read is used, capped at 100%. A book counts as finished at 99%. Reading time comes from the timer.

KRDS annotations hold positions but not text, so highlight and note text comes from `documents/My Clippings.txt`, matched to library books by title. The clippings format is the Kindle's English one; entries in other languages keep their text but are treated as highlights.

## Send to Kindle by email

Each book is exported as EPUB, which Send to Kindle accepts, and sent in its own email to stay under Amazon's 50 MB limit. Sending uses SMTP through `lettre` with rustls, STARTTLS by default. The password is stored in the keychain through `keyring` under "Parallax SMTP", never in the library. The sending address must be on the account's approved list; Parallax can only tell whether the email was sent.

## How it was verified

- **Fixtures and temporary folders.** Header parsing, thumbnails, APNX bytes (compared with calibre's layout), clippings, KRDS, collections and the send, update, remove and import workflows run against `tests/fixtures/amazon` books in temporary drive-mode Kindles (`crates/device-kindle`, `crates/core/tests/kindle.rs`).
- **Virtual MTP device.** `crates/device-kindle/tests/mtp.rs` runs storage operations, sending, scanning, thumbnail restore and removal through mtp-rs's virtual device, with and without rename support.
- **The browser mock.** `?kindle`, `?kindle=drive` and `?kindle=busy` show the UI for an MTP, drive or held Kindle; `&kindle_email=name@kindle.com` sets up email.

None of this shows how a real Kindle indexes files, displays thumbnails and APNX page numbers, behaves over MTP on macOS, or writes KRDS files on current firmware. The [hardware checks](../kindle-hardware-checks.md) cover those.
