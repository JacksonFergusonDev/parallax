# Kobo hardware results

These are dated observations from the reference Kobo Libra 2. Each row covers the listed workflows on the app or prototype available at the time. See the [hardware checklist](hardware-checks.md) for repeatable procedures and checks still open.

| Date | Firmware and schema | Observed result |
| --- | --- | --- |
| 2026-10-30 | 4.38.23697, schema 174 | A pre-import collection row put a newly sent book in its collection immediately after import. The trigger variant needed a restart to appear. Reconnecting after import allowed collection and series changes to appear after ejecting. Parallax uses pre-import membership and reconciles series on reconnect. |
| 2026-11-01 | 4.38.23697, schema 174 | Title and series edits, cover replacement, and sending directly into a collection worked. A separate read-only import into a scratch library imported 93 sideloaded books with the device database checksum unchanged. |
| 2026-11-04 | 4.38, schema 174 | The owner confirmed the Kobo MVP and follow-up checks available that day. These included sending and updating EPUBs, managing collections, reconciling library copies, reading status and stats, and highlights and notes. The [original checklist and progress log](README.md#historical-material) identify the individual checks. |
| 2026-11-04 | 4.38.23852, schema 174 | App-made collections retained their books through Kobo cloud sync. Thirty synced collections soft-deleted by Parallax stayed deleted after another sync. |
| 2026-11-08 | 4.38 | The four-way [comics prototype](decisions/kobo-comics.md) confirmed faster opening and page turns for processed images. It also checked author and series display. The direction test was inconclusive. |
| 2026-11-09 | 4.38 | A Libra 2 comparison informed enabling dithering by default for greyscale comic pages. The final colour-to-greyscale treatment still needs checking through the production send path. |
