# Kobo hardware checks

Run the relevant checks on a physical Kobo with test books and disposable collections. Record the app version or commit, model, firmware, database schema, steps exercised, and results in [hardware results](hardware-results.md). Save a [device report](../docs/device-support.md#share-a-device-report) for an unfamiliar model or unexpected behaviour. Fixture replay and FAT32 image tests cover the desktop side of the workflow. Use the device to observe how Nickel imports and presents changes.

## Connection and reading

- Connect by USB and confirm **On Kobo** lists the expected books, collections, covers, and progress. Check **About this Kobo…** for its write verdict and screen profile. On a read-only device, confirm that change controls are unavailable and eject still works.
- Save a device report and confirm that the reported model, firmware, schema, and verdict match the app. Confirm that Nickel shows no import or library change after ejecting.
- Import a sideloaded book into Parallax and open it on the desktop. Confirm its metadata and cover, and that Parallax recognises the device copy. Check that available reading stats, highlights, and notes match the device.

## Send, update, and organise

- Send a test EPUB to a new collection and eject. Confirm that the book opens as a KEPUB, the cover and metadata appear, and the book is already in the collection. Reconnect with series reconciliation enabled, then eject and verify the series.
- Read a few pages, then update the book from Parallax with a changed cover and title. After ejecting, confirm that the changes appear and the reading position is preserved.
- Create, rename, and delete disposable collections. Apply an automatic grouping rule and verify its membership. With cloud sync enabled, sync and check that the remaining collections keep their local books and deleted synced collections stay gone.
- Check reading-status changes and remove a disposable book. Eject and verify that the status changes persist and the removed book leaves no blank entry. If using the Library removal dialog, verify the selected library and device scopes.
- On a batch send or update, check progress and the result for each book. Check pending sends, replacements, and removals before ejecting, then reconnect and verify that imported or removed books have settled into the expected state.

## Restore a device change

Create a test collection and send a disposable book to it. Eject, read a few pages, and reconnect. Remove the book in Parallax, eject, and confirm it is gone. Reconnect and choose the point before removal in **Revert changes…**. Eject and check that the book, cover, reading position, and collection membership are back. Reconnect and undo the revert, then eject and verify the book is gone again.

## Amazon formats

- Send a DRM-free KindleGen AZW3 or combo MOBI, an older MOBI, and a DRM-free KFX where available. Check cover, images, styling, contents, and a footnote link in the converted copy.
- Check that a DRM-protected input is refused with a DRM message and no book is added.

## Comics

1. Send or update a comic with a named series, an ordinary spread, and a very wide page. Try **Split**, **Keep whole**, and **Whole then halves**. After ejecting, check page order and fit. Ordinary whole spreads should stay unrotated. Verify that the very wide page rotates in each mode.
2. Compare left- and right-edge taps for a raw CBZ and a prepared KEPUB. Repeat for a left-to-right comic. Record the direction each tap actually turns.
3. Check chapter navigation and metadata on the Kobo. Reconnect with **Add series to newly imported books** enabled, then verify the series and volume order. Update a comic already in progress and check that it reopens at the same place.
4. On a colour Kobo, check colour pages and greyscale pages and record any rainbow artefacts. On an older 600 × 800 or 758 × 1024 model, confirm that the fixed-layout KEPUB opens and record page-turn speed.
5. If testing landscape two-up with spread pinning enabled in the model profile, record the model and firmware and whether the split halves face each other.

## Marking a model as tested

After running the relevant checks on a physical device of a new model and recording them in [hardware results](hardware-results.md), add its short model id to `TESTED_MODELS` in `crates/device-kobo/src/support.rs`. Untested models still get changes when the database checks pass. The flag only controls the "not tested yet" notice and **About this Kobo…**.

## Checks still open

- Repeat the production comics send and update checks on the Libra 2. Verify direction, chapter presentation, series, reading position, and the final page-processing settings. Colour and older models still need their own runs.
- Verify batch progress, pending-eject states, Library removal scopes, Library filters with a connected device, and the physical-device restore flow.
- Collect reports and run the relevant checks on other Kobo models. Real KindleGen combo files and KFX samples are still needed for the import checks.
