# Reader spike results

Run on 2026-11-07 against a minimal release build (Tauri 2.12.5, wry 0.57.1, Vite 8.3.5, macOS 27.0.1) with the `readest/foliate-js` fork at `b34a0aa`. It tests the design in [the research report](../reports/EPUB%20reader%20library%20landscape.md). Source and fixtures are in [spikes/reader](../spikes/reader/README.md).

**Verdict: the design holds, so go ahead.** Both gating questions passed in real Tauri, with positive controls. Stub commands stood in for the real ones, and nothing touched a Kobo. The real app has not been changed.

## Results

| Question | Result | Evidence |
| --- | --- | --- |
| 1. Does the manifest enforce labels? | **Pass** | `device_restore` ran from `main` and was refused from `reader-1`. Control without a manifest: it ran from a window with no capabilities. Not covered: the real 61 commands in `main`. |
| 2. Does CSP compose? | **Pass** | Strict: 0 scripts ran, 0 server hits, cover host and `asset:` images blocked, the book's own image loaded. Control with the CSP loosened: 16 script vectors and 40 network endpoints fired. Tauri's header alone let the cover host and `asset:` through, so the `reader.html` meta policy is needed. |
| 3. Does the fork build and render? | **Pass** | Builds on Vite 8.3.5 with `pdf.js` aliased to a stub and `construct-style-sheets-polyfill` added. Engine styles, book CSS, images and fonts all work under the strict CSP. |
| 4. Do engine events work? | **Pass, wheel untested** | Native click, double-click, drag selection, keyboard, highlights, click on a highlight, footnotes, internal and external links all work with book scripts blocked. |
| 5. Which byte path is fast enough? | **Per-entry command** | 94.7 MB book: opens in 19–34 ms by entry, about 100 ms whole file; page turns about 108 ms either way. No `book://` transport needed. |
| 6. Do the native guards work? | **Pass; drop the rule list** | `on_navigation` fires for subframes and stopped a frame leaving for a remote page, which reached the server without it. Popups were refused by `on_new_window`. |
| 7. Does the engine survive awkward books? | **Pass, one open race** | All 96 library books opened; 6,563 sections rendered; 0 of 4,042 images broke. Details below. |
| 8. Do locators hold? | **Pass for positions; highlights unknown** | Filtered CFIs resolved 1,794 of 1,804 passages inside KEPUBs, against 0 unfiltered. 22 of 31 real Kobo positions mapped exactly. Details below. |

## The ACL is a real safety net

With the CSP deliberately loosened, book scripts reached `__TAURI_INTERNALS__` and called `device_restore` 14 times. Every call was refused by the ACL and the command never ran. A CSP mistake therefore doesn't reach Kobo writes, provided the manifest is in place.

The manifest switches on when any permission file exists in `permissions/`, not only when `build.rs` declares it. A leftover `permissions/` folder enforces the ACL with no grants, which blocks every app command.

## Library survey

Every section of all 96 books was walked under the strict CSP, read-only.

- **Page turns:** median 105 ms, 95th percentile 169 ms, maximum 485 ms. The roughly 100 ms floor looks like the engine's animation rather than loading.
- **CFI round trips:** 288 of 288 returned to the same place.
- **Obfuscated fonts:** all 6 such books loaded fonts, so `crypto.subtle` deobfuscation works on `tauri://`.
- **Blocked fonts:** 1,856 `font-src` reports in 3 books, all dead references in the books' own CSS to a scrubbed `XXXXXXXXXXXXXXXX` file. They resolve against `tauri://localhost`, so blocking them is correct.
- **Stress fixtures:** a 19 MB single-file book opened in 313 ms with 110 ms turns, thanks to the fork's section chunking. Self-closed tags and malformed XHTML kept their text. Vertical Japanese, Arabic RTL and fixed-layout spreads were checked by eye and render correctly.

## Kobo mapping

`ChapterIDBookmarked` is `<path inside the zip>#kobo.N.M`, which closes that gap in the research.

Of the 31 sideloaded books with a position, 22 mapped through a fresh kepubify 4.0.4 conversion. For all 22, the 40 characters at the mapped offset matched the plain EPUB, and the computed percentage was within 1.6 points of the Kobo's. The other 9 have span ids or paths the fresh conversion doesn't contain. All 9 reached the device before this project began, which suggests an older converter, but some equally old books did map. **So mapping must read the KEPUB actually on the device** rather than reconverting.

Filtered CFIs work because foliate-js can skip `koboSpan`, `book-columns` and `book-inner`. Sections must be matched by href, as kepubify can shift spine positions. The test used locally converted KEPUBs and checked that the same 30 characters came back.

## Changes to the report

- **Frames are `about:srcdoc`, not blob.** The fork loads sections that way. This strengthens the case against per-chapter headers, because srcdoc inherits the parent's CSP. `frame-src blob:` stays in the tested policy.
- **CFI is more than a cache.** Store it alongside the text quote. The quote remains the fallback for the 0.6% that failed and for books whose text changes.
- **The fork needs one more dependency:** `construct-style-sheets-polyfill` (MIT).
- **The bundle is about 99 KB gzip** with every format and the test code included, not 57.5 KB. Dropping MOBI, FB2 and comics would reduce it.
- **Keep `ipc:` and `http://ipc.localhost` in `connect-src`.** Without them the reader fails on boot, because the fallback IPC path can't carry these binary responses.
- **Drop WKContentRuleList for v1.** It worked (no remote loads, even with scripts running), but it doesn't cover `asset:`, needs `unsafe` in a `forbid` crate, and duplicates the CSP.
- **Page-turn and wheel input are app code.** The fork has no wheel handling.

## New issues to design for

- **Covered windows can stall.** On the first survey, turns jumped to 1,000 ms and the window eventually stopped entirely. Setting `background_throttling` to `Disabled` and keeping the machine quiet removed both. The run changed both at once, so throttling is the likely cause, not a proven one. Test that a covered reader resumes.
- **Progress can exceed 1.** Very small books reported `fraction: 2`. Clamp it before storing or displaying.
- **Rare `goTo` hang.** Two of 6,563 sections timed out at 8 seconds during the walk, then rendered in about 640 ms alone. Use a timeout with a retry.

## Not tested

- The real app's 61 commands under a manifest. The manifest is the next step, and it needs a full regression check of `main`.
- Wheel and trackpad. Synthetic wheel events never reached the page, so this needs a hand check.
- Highlight offsets (`StartContainerPath`, `StartOffset`). The fixture and the latest backup have no `Bookmark` rows because this Kobo has no highlights.
- KEPUBs from the device. Only local conversions were read.
- VoiceOver, media overlays, the epubtest.org and W3C suites, and whether `tauri dev` applies the CSP. Only `epub-test.epub` from the wider corpus was run.
- A wrong invoke key from a subframe, end to end.
- Book 14 reported 3 font errors, not reproduced in its first nine sections. Its CSS has a dead reference, which is the likely cause.
