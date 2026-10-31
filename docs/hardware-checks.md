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

Batched so you can do several in one sitting when Claude asks:

- M1: `erc kobo ls` output matches the library on the device.
- M2: a book sent by the app opens and shows KEPUB features (pages per chapter, reading stats).
- M4: a replaced cover shows on the device.
- M5: a book removed by the app leaves no ghost entry; `._*` files are tidied.
- M6: bulk collection assignment and "folders → collections" on your real library.
