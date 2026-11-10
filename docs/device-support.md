# Device support

The recorded hardware tests are for the Kobo Libra 2 running firmware 4.38 with database schema 174. They cover EPUB transfers, collections, metadata changes, and other workflows available in early November 2026. Parallax can import DRM-free Kindle-format files. Kindle device support is deferred.

## Read-only devices

When you connect a Kobo, open **On Kobo › About this Kobo…** and look for **Changes**. If the app shows a read-only reason, you can still browse books and collections and import books into Parallax when the database is readable. The app disables actions such as sending, updating, removing, and changing collections on that device. Eject is still available.

Parallax allows writes on Kobo firmware 4.x or earlier with database schema 53–222 and the required database columns and value types. Tolino models are read-only. Older schemas may lack individual features such as series or reading time. These checks apply to every connected device, including models without recorded hardware tests.

Parallax tells you when a model has no recorded hardware checks yet, once per model, during setup or in a banner under the toolbar. An untested Kobo that passes the checks above can still be changed, with a backup before every change. **About this Kobo…** shows whether hardware checks have been run on the model.

**About this Kobo…** also shows the screen size used when preparing comics. Parallax looks up the model, measures the device’s full-screen cover cache when the model is unknown, or falls back to 1072 × 1448. The screen entry identifies whether that size was looked up, measured, or assumed.

## Share a device report

1. Connect your Kobo and tap **Connect** on its screen.
2. In Parallax, open **On Kobo › About this Kobo… › Save device report…**. From the command line, you can use `parallax-cli kobo report --out report.json`.
3. To share the JSON, attach it to a [GitHub issue](https://github.com/JacksonFergusonDev/parallax/issues/new) with a description of what happened and what you expected. Saving the report only reads the Kobo. Sharing it is optional.

The report includes the model, firmware, database version and schema, counts and storage types, cover-cache sizes, top-level system dot folders, and the read-only verdict. It omits the serial number, book titles and authors, book paths, reading progress, highlights, and account details.
