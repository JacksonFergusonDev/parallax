# Hardware checks

Manual checks only a person holding the Kobo can do. Record results in the tables, or just tell Claude what you saw.

## Spike S1 — collections for newly sent books (**done 2026-10-30**: A immediate, B after restart, C immediate)

**Question:** when the app sends a book, can it also put the book in a collection without you replugging the Kobo? Three approaches are tested at once with three tiny test books:

| Book | Approach | What it proves |
|---|---|---|
| A | Collection row written *before* the Kobo imports the book | Simplest option: no triggers needed at all |
| B | Database trigger adds the row *as* the Kobo imports the book | The seriesmeta-style option |
| C | Row written on the *next* connection, after import (plus series) | The "unplug and replug" baseline that calibre uses |

What the script changes on the device: it adds a folder `ERC-Spike/` with 3 small EPUBs, 3 collections named `ERC Spike A/B/C`, and one trigger with a side table. It backs up `KoboReader.sqlite` into `.spike-backups/` in this repo before every write. `cleanup` removes everything it added.

### Before you start

1. On the Kobo, **turn Wi-Fi off** (so the test collections never sync to your Kobo account).
2. Connect the Kobo and tap **Connect** so it shows up as `KOBOeReader`.

### Round 1 — A and B

```bash
scripts/spike-s1.sh prepare
```

```bash
diskutil eject /Volumes/KOBOeReader
```

3. Unplug, then wait for the Kobo to finish "Importing content".
4. On the Kobo, open **My Books → Collections** and check the table below. If a collection is empty, **restart the Kobo** (Settings → Device information → Restart, or hold the power button → Power off, then on) and check again.

| Check | Before restart | After restart (only if needed) |
|---|---|---|
| ERC Spike A exists and contains "ERC Spike A" | | |
| ERC Spike B exists and contains "ERC Spike B" | | |
| All 3 test books appear in My Books and open | | |

### Round 2 — C (replug flow)

5. Plug the Kobo back in and tap **Connect**.

```bash
scripts/spike-s1.sh check
```

```bash
scripts/spike-s1.sh replug-test
```

```bash
diskutil eject /Volumes/KOBOeReader
```

6. On the Kobo, check:

| Check | Result |
|---|---|
| ERC Spike C exists and contains "ERC Spike C" (without a restart) | |
| "ERC Spike C" shows series "ERC Spike Series", book 1 | |
| A and B are still in their collections | |

### Round 3 — clean up

7. Plug in, tap **Connect**, then run:

```bash
scripts/spike-s1.sh check
```

```bash
scripts/spike-s1.sh cleanup
```

```bash
diskutil eject /Volumes/KOBOeReader
```

8. Confirm the 3 test books and 3 collections are gone, then turn Wi-Fi back on.

If anything looks wrong at any point, plug in and run `scripts/spike-s1.sh restore` to put back the most recent backup.

## Later checkpoints (need app code first)

**All done 2026-11-04**: you confirmed every app feature below works on the Libra 2 (firmware 4.38, schema 174). Spike S2 (cloud sync) was skipped.

Batched so you can do several in one sitting when Claude asks:

- M1: `erc kobo ls` output matches the library on the device.
- M2: a book sent by the app opens and shows KEPUB features (pages per chapter, reading stats).
- M3 (**done 2026-11-01**): edit a library book's title and series, send it, and confirm the Kobo shows the new title and series.
- M4 (**done 2026-11-01**): a replaced cover shows on the device. Re-check: "Find cover online…" and "Find series and genres online…" for We Are Legion (We Are Bob) now offer only that book's covers and find Bobiverse #1.
- M5, in the On Kobo tab:
  - Remove a book you don't need: after ejecting, it's gone from the Kobo's library with no blank entry.
  - Click "Remove macOS clutter": it should report about 68 files, and the Kobo still works normally afterwards.
  - Send a library book, read a few pages on the Kobo, reconnect, edit its title and cover in the app, click "Update 1 on Kobo", eject: the new title and cover show, and your reading position is kept.
- M6, in the app:
  - (**done 2026-11-01**) Library tab: select two books (⌘-click), choose "New collection…" next to Send, name it, send and eject. Books already on the Kobo instead show "Add N to collection ▾" in the same place. Both books appear in that collection on the Kobo straight away. Reconnect: if they have a series, the app says it added series details; eject again and the series shows on the Kobo.
  - On Kobo tab: "Folders → collections…" (dry run on your device: Fantasy 20, Fiction 8, Horror 1, Nonfiction 40, Sci-Fi 24). Create them, eject, and check My Books → Collections.
  - Rename one collection and delete another; eject and confirm.
- M8, On Kobo tab → "Automatic collections…": add "A collection for each author" with at least 3 books. The preview should list collections such as Brandon Sanderson (7) and Pierce Brown (5). Apply, eject, and check them on the Kobo. Sending a new book by one of those authors should file it automatically. Connecting never creates collections by itself: a rule that would make new ones waits for Apply.
- M9: Library and On Kobo each switch between Covers and List; clicking a list header sorts descending, then ascending, then back to the default. Scrolling past the end of a list doesn't move the rest of the window.
- M10: after any change from the app (send, update, remove, collections), On Kobo → "About this Kobo…" lists it under Activity. After ejecting, the Kobo's library shows no new or blank books (the records live in a hidden `.erc` folder, which Nickel should ignore). `erc kobo history` shows the same.
- M11: On Kobo → "Add all to library": every book you sideloaded appears in the Library tab with its cover, marked On Kobo, and none needs updating. Open one in another reader (Apple Books) to confirm it reads normally.
- M12: On Kobo → "Find duplicates…" should say no duplicates on your Kobo today. To try the review, duplicate an unread KEPUB in Finder on the Kobo (⌘D), eject and reconnect so Nickel lists both, then remove the copy from the review, click "Remove macOS clutter" and eject: only the original remains, with its reading progress.
- M13: On Kobo → "Enhance and organise…": books are added to the library, then series and genres are proposed (genres use your folder names, e.g. Sci-Fi). Save, click "Update N on Kobo", then "Make collections from series and genres…", add series and tag rules, Apply, eject: series show on book details and the genre collections appear, with reading progress intact.
- M14: On Kobo → "Check this Kobo…" (expect few or no problems; leftover covers are likely). Delete leftover covers and eject: covers still show for every book. Mark a book you've finished as Finished, and one as "Unread (start over)", eject: the Kobo shows both. Reconnect: the Library tab's Progress column matches the Kobo.
- M15: no device needed. Library → "Check library…" lists low-resolution covers and broken links; click one to open the book. In the edit panel, add a translator to a book, send it, and check in calibre or Apple Books that the translator is listed.
- M16: highlight a passage and add a note in a sideloaded book on the Kobo, then reconnect. In the Library tab, select that book: the edit panel shows "Highlights and notes (2)…" with the chapter named as in the Kobo's contents; export Markdown and open it.
- Device support: with the Kobo connected, About this Kobo shows "Changes: Allowed, with a backup first"; Save device report… writes a JSON file, and the Kobo shows no change afterwards (no new books, no import screen).

## Spike S2 — collections and Kobo cloud sync (with M6) (**skipped 2026-11-04**: the user doesn't sync with a Kobo account; run it if someone who does reports collections coming back or vanishing)

**Question:** do collections the app creates survive a Kobo sync, and does deleting one stick?

1. After the M6 checks, keep one app-made collection with books in it (say "ERC Sync Test").
2. On the Kobo, turn Wi-Fi on and sync (Settings → Sync, or the sync icon). Wait for it to finish.
3. Check: the collection still exists and still holds its books. (Expected: the name syncs to your account; sideloaded books in it stay because the membership is local.)
4. Connect, delete "ERC Sync Test" in the app, eject, sync again.
5. Check: it doesn't come back after the sync. If it does, tell Claude; deletions of synced collections may need a different approach.

| Check | Result |
|---|---|
| Collection survives sync with its books | |
| Deleted collection stays deleted after sync | |
