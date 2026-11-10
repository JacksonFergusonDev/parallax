# EPUB reader architecture

Parallax uses the [Readest fork of foliate-js](../../vendor/foliate-js/VENDORED.md) for EPUB layout, navigation, and locations. The MIT-licensed fork carries macOS WebKit fixes and runs in the Tauri webview without a separate publication server or native rendering engine. Parallax owns the reader interface and keeps the desktop library and file access in Rust.

Each reader window accesses only its own book through scoped Tauri commands. The reader fetches individual archive entries and renders publisher content in isolated frames with scripts and remote loads blocked. The webview content policy, navigation restrictions, and Tauri command permissions enforce separate boundaries. The release-mode prototype confirmed that the content policy blocked the tested script and network paths and that reader windows lacked access to device commands.

## Evidence from November 7, 2026

- The prototype opened all 96 books in its local sample and rendered 6,563 sections. Median page turns took 105 ms, with 169 ms at the 95th percentile. The 288 tested CFI round trips returned to the same position.
- A 94.7 MB test book opened in 19–34 ms with per-entry access versus about 100 ms when passing the whole file. Page turns took about the same time.
- CFI positions are stored with section and text fallbacks. In local KEPUB conversions, ignoring Kobo’s wrapper markup let 1,794 of 1,804 test passages resolve. Only 22 of 31 stored device positions could be mapped through a fresh conversion. Future Kobo position sync should resolve against the exact KEPUB on the device because the original path or span IDs can differ from a fresh conversion.

The prototype did not establish highlight offset mapping, VoiceOver behaviour, or wheel and trackpad feel. The [desktop integration suite](../../scripts/reader-e2e.mjs) now exercises the production reader in WKWebView. The original survey, timings, and prototype are linked from the [engineering archive index](../README.md#historical-material).
