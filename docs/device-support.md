# Device support

The recorded hardware tests are for the Kobo Libra 2 running firmware 4.38 with database schema 174. They cover EPUB transfers, collections, metadata changes, and other workflows available in early November 2026. Kindle support is new and hasn't been tested on a Kindle yet; see [Kindle](#kindle).

## Read-only devices

When you connect a Kobo, open **On Kobo › About this Kobo…** and look for **Changes**. If the app shows a read-only reason, you can still browse books and collections and import books into Parallax when the database is readable. The app disables actions such as sending, updating, removing, and changing collections on that device. Eject is still available.

Parallax allows writes on Kobo firmware 4.x or earlier with database schema 53–222 and the required database columns and value types. Tolino models are read-only. Older schemas may lack individual features such as series or reading time. These checks apply to every connected device, including models without recorded hardware tests.

Parallax tells you when a model has no recorded hardware checks yet, once per model, during setup or in a banner under the toolbar. An untested Kobo that passes the checks above can still be changed, with a backup before every change. **About this Kobo…** shows whether hardware checks have been run on the model.

**About this Kobo…** also shows the screen size used when preparing comics and covers. Parallax looks up the model, measures the device’s full-screen cover cache when the model is unknown, or falls back to 1072 × 1448. The screen entry identifies whether that size was looked up, measured, or assumed.

## Kindle

Connect your Kindle by USB and unlock it. Most Kindles before 2024 appear in Finder as a drive. The Scribe (firmware 5.16.3 and later), 2024 and later Paperwhites and the Colorsoft don't; they use MTP, which Parallax opens itself. Either way, an **On Kindle** tab appears.

What works:

- **Send to Kindle** converts library books to AZW3, with estimated page numbers and a cover on the Kindle's home screen. Comics aren't sent yet.
- **Update on Kindle** replaces a book you edited and keeps your place and highlights.
- **Remove from Kindle** deletes a book with its page numbers, cover and the Kindle's reading data for it.
- **Add to library** copies DRM-free books from the Kindle into your library. Books bought from Amazon are skipped.
- When your Kindle connects, its reading progress and the highlights, notes and bookmarks in its `My Clippings.txt` are copied into your library. Progress is worked out from where you are in the book, so it can differ a little from the Kindle's own percentage, and a book counts as finished once you reach the end.
- When a Kindle goes online, Amazon can replace the covers of books you sent with generic ones. Parallax puts them back each time the Kindle connects, as calibre does.

What doesn't: a Kindle keeps its collections and its read status where Parallax can't reach them, so they aren't shown or changed. Older Kindles that list collections in a file show them, read-only.

### If Parallax can't open your Kindle

On a Mac, other apps can hold a Kindle that uses MTP, and then Parallax shows why it can't open it.

- macOS's photo import service grabs these Kindles as they connect. With **Settings › Kindle › Free your Kindle from photo import** on (the default), Parallax stops it while it opens your Kindle, and macOS starts it again when it's needed.
- Amazon's Send to Kindle app, Android File Transfer, OpenMTP and MacDroid also hold the Kindle. Quit them, then unplug your Kindle and plug it in again.

### Send to Kindle by email

**Email to Kindle** in the library sends the selected books to your Kindle's email address, and Amazon delivers them to your Kindles and Kindle apps. It works with any Kindle and needs no cable.

1. On Amazon, open **Manage Your Content and Devices › Preferences › Personal Document Settings**. Note your Kindle's email address, which ends in `@kindle.com`, and add the address you'll send from to the **Approved Personal Document E-mail List**.
2. In Parallax, open **Settings › Kindle** and enter your Kindle's email, your email provider's outgoing mail server, your user name and password. Many providers need an app password made for Parallax. It's kept in your Mac's keychain.
3. Select books in the library and choose **Email to Kindle**. Each book goes as an EPUB in its own email. Books over about 37 MB can't be emailed, as Amazon takes up to 50 MB per email.

Parallax can tell you an email was sent, but not whether Amazon accepted it. If a book doesn't arrive, check that your sending address is on the approved list.

## Share a device report

1. Connect your Kobo and tap **Connect** on its screen, or connect and unlock your Kindle.
2. In Parallax, open **On Kobo › About this Kobo… › Save device report…**, or **On Kindle › About this Kindle… › Save device report…**. For a Kobo, you can also use `parallax-cli kobo report --out report.json`.
3. To share the JSON, attach it to a [GitHub issue](https://github.com/JacksonFergusonDev/parallax/issues/new) with a description of what happened and what you expected. Saving the report only reads the Kobo. Sharing it is optional.

A Kobo report includes the model, firmware, database version and schema, counts and storage types, cover-cache sizes, top-level system dot folders, and the read-only verdict. A Kindle report includes the model, firmware, how it connects, how many books of each file type it has, and which of the folders and files Parallax uses are there. Neither includes the serial number, book titles and authors, book paths, reading progress, highlights, or account details.
