# Comic preparation for Kobo

Parallax prepares pages for the target Kobo screen at send time and packages them as a fixed-layout KEPUB through kepubify. The preparation output is temporary. The source comic in the library remains available for reading and future sends. Use the [device screen profiles](../../crates/device-kobo/src/screen.rs) for the target size and colour capabilities.

Fixed-layout KEPUB preserves author metadata on the tested Kobo. Series details are written through the guarded device APIs on reconnect after Nickel imports the book, when that option is enabled. Spread handling is a user setting. The implementation adapts page-processing and layout logic from Kindle Comic Converter v13.0.0, with attribution in [KCC-NOTICES](../../crates/comic/KCC-NOTICES).

## Evidence from November 8, 2026

A Kobo Libra 2 on firmware 4.38 was used to compare three volumes in four variants. The original CBZ and a lossless KEPUB contained the same source images. A second CBZ and KEPUB used processed pages. Processing brought the tested 1810 × 2560 JPEG pages down to the 1264 × 1680 screen and encoded them as 16-level greyscale PNGs.

| Observation | Result |
| --- | --- |
| Open time and page turns | Originals took about 2 s to open and 1.5 s to turn a page. Processed copies took about 0.5 s and 0.2 s respectively in either container, with similar perceived quality. |
| Size and preparation time | The three volumes fell from 1,501 MB to 369 MB in total. Processing took about 8 s per volume on the test Mac. |
| Metadata | Both KEPUBs displayed the author. The processed CBZ's ComicInfo author did not appear. None of the four variants grouped as a series. |
| Right-to-left paging | The initial check could not distinguish embedded direction from the device’s default tap behaviour. |

These figures describe the prototype on one Kobo. The [hardware checklist](../hardware-checks.md#comics) covers the production send and update flow, paging direction, colour screens, and older models. The full comparison, research, and prototype are linked from the [engineering archive index](../README.md#historical-material).
