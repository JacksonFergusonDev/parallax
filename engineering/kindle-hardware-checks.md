# Kindle hardware checks

Kindle support was built without a Kindle. Fixtures, calibre's driver logic and mtp-rs's virtual MTP device cover the desktop side (see [the Kindle decision](decisions/kindle.md#how-it-was-verified)), but nothing yet confirms how a real Kindle reacts. Run these checks on a physical Kindle and record the results in [hardware results](hardware-results.md). Until then, every Kindle shows as untested and the app calls Kindle support new.

Use only test books you add for the session. Don't open, copy or share the owner's own books, highlights or reading data. Each check is tagged with the connection it applies to:

- **Drive**: Kindles before 2024 (except the Scribe), which appear in Finder as a volume named Kindle.
- **MTP**: the Scribe on firmware 5.16.3 or later, 2024 and later Paperwhites, the Colorsoft and later models. They don't appear in Finder.
- **Either**: both.

Do the MTP checks if the Kindle is MTP, the Drive checks if it's a drive, and every Either check.

## Before you start

1. Build the branch or release under test and note its version or commit.
2. On the Kindle, open **Settings › Device options › Device info** and note the model, firmware version and serial number. Keep the serial out of anything you share.
3. Prepare test books in the library: an EPUB with a cover, one with a table of contents, footnote links and an image, and one with a very long title. Add a DRM-free AZW3 file on the Mac for check 14. Add a CBZ comic for check 9.
4. Quit Amazon's Send to Kindle, Android File Transfer, OpenMTP and MacDroid if they're running. They hold MTP devices open. Keep Activity Monitor ready to check for them and for `ptpcamerad`.
5. Note whether the Kindle's Wi-Fi is on. Check 8 depends on it.

## Connection

1. **Either.** Unlock the Kindle and connect it by USB. Within about 5 seconds, Parallax should show an **On Kindle** tab and the Kindle's model in the top bar. Open **About this Kindle…** and record the model, firmware, connection and screen shown. Compare them with the Kindle's Device info.
   - If the model shows only as "Kindle", record characters 4 to 6 of the serial (or characters 3 and 4 if it starts with B or 9). That code is missing from `crates/device-kindle/src/models.rs`.
2. **Drive.** Confirm the serial in About this Kindle is the Kindle's real serial and not `kindle-Kindle`. A real serial means Parallax matched the drive to the Kindle's USB details.
3. **MTP.** With **Settings › Kindle › Free your Kindle from photo import** on, unplug and plug in the Kindle 3 times. Record whether it opens each time and how long it takes. Then turn the setting off and plug it in again. Record the message shown, and whether `ptpcamerad` appears in Activity Monitor.
4. **MTP.** Start Amazon's Send to Kindle app, then plug in the Kindle. Parallax should show "Another app is using your Kindle" instead of the books. Quit the app, unplug and plug in again. The books should appear.
5. **Either.** Choose **Eject Kindle**. On a drive, the volume should leave Finder. On MTP, the Kindle should return to its home screen or say it's safe to unplug, and Parallax shouldn't reopen it until you unplug and plug it in again. Record what the Kindle's screen shows.
6. **Either.** From Terminal, run `parallax-cli devices --json` and `parallax-cli kindle ls`. Both should list the Kindle and its books.

## Books on the Kindle

7. **Either.** Open **On Kindle**. Books already on the Kindle should be listed with titles and authors. Books bought from Amazon should not offer **Add to library**. For 3 books you've opened on the Kindle, record the percentage Parallax shows and the Kindle's own percentage or location. Positions are byte offsets into the book's text, so they should be within a few percent.

## Sending

8. **Either.** Select the test EPUBs in the library and choose **Send to Kindle**. Eject and unplug. On the Kindle, record for each book:
   - It appears in the Library within a minute, as a downloaded book.
   - Its cover shows on the home screen and in the Library, not a generic cover.
   - Title and author are right, and the long title isn't cut off in a way that breaks it.
   - The book opens, and the table of contents, footnote link and image work.
   - **Go to › Page** offers page numbers (from the APNX file). With **Add page numbers** off in Settings › Kindle, send another book and confirm it shows locations only.
9. **Either.** Select the CBZ comic, set as manga (right to left), and send it. On the Kindle, record whether it appears under the Library's comics, opens full screen with no margins, turns pages right to left with a tap on the left, shows its cover on the home screen, and whether pages look sharp and evenly toned. Send the same comic as left to right and confirm the page direction follows. On a Colorsoft, record whether colour pages show in colour. Note any page that's blank, cropped or the wrong way round.
10. **Either.** With Wi-Fi on, let the Kindle sync for a few minutes, or restart it. Record whether sent books' covers turn generic. Connect to Parallax again, eject, and check whether the covers are back. Parallax copies them back on every connect, as calibre does.
11. **Either.** Send a book that's already on the Kindle. Parallax should refuse it as already on the Kindle.

## Updating, progress and highlights

12. **Either.** On the Kindle, read about a third of a sent book. Highlight a sentence, add a note to another, and add a bookmark. Connect to Parallax. In the library, the book's progress should be close to the Kindle's percentage. Open its highlights: the highlight, the note and the bookmark should be there with their text and dates. Record any that are missing or wrong. Then delete `documents/My Clippings.txt` from the Kindle with Parallax closed (on MTP, use OpenMTP or Android File Transfer), then connect to Parallax. The highlight from check 12 should still be there, now read from the book: compare its text word for word with what you highlighted, and note any missing or extra words at either end. The note's own text should be there too.
13. **Either.** Change the book's title and cover in Parallax and choose **Update on Kindle**. Eject. The book should open where you left it, with its highlights, and show the new title and cover.
14. **Either.** Finish a book on the Kindle, or mark it as read there. Connect and record what Parallax shows for it. Parallax estimates "finished" from the position, so a book marked read without reaching the end will likely still show as reading.

## Removing and adding

15. **Either.** Remove a sent book in **On Kindle**. Eject. It should be gone from the Kindle with no blank entry left. On a drive, confirm in Finder that its `.sdr` folder and its file in `system/thumbnails` are gone.
16. **Drive.** Copy the DRM-free AZW3 into the Kindle's `documents` folder in Finder. In Parallax, select it in **On Kindle** and choose **Add to library**. It should appear in the library with its cover and details.
17. **Either.** Try **Add to library** on a book bought from Amazon. It should be skipped as a Kindle Store book.

## Send to Kindle by email

This needs the Amazon account the Kindle is registered to, or your own account with the Kindle app on a phone, which receives emailed books too.

18. In **Settings › Kindle**, enter the account's `@kindle.com` address, your mail server, user name and app password. Add your sending address to the account's Approved Personal Document E-mail List.
19. Select 2 test EPUBs and choose **Email to Kindle**. Both should arrive on the Kindle or app within about 10 minutes, with covers and correct titles. Record how long they took and anything that bounced.
20. Remove the address from the approved list and email a book again. Record whether you get a bounce message. Parallax can only tell you whether the email was sent, not whether Amazon accepted it.

## Collect

- Save a device report from **About this Kindle… › Save device report…**. It has no titles, serial or reading data.
- For one test book you read and highlighted in check 12, copy its `.sdr` folder from `documents/` and the matching entries from `documents/My Clippings.txt`. On MTP, quit Parallax and copy them with OpenMTP or Android File Transfer. These become fixtures for the sidecar and clippings parsers in `crates/device-kindle`. Only collect test books.

## Marking a model as tested

After running these checks on a model and recording them in [hardware results](hardware-results.md), add its name, as About this Kindle shows it, to `TESTED_MODELS` in `crates/device-kindle/src/models.rs`. The app then stops calling Kindle support new for that model.
