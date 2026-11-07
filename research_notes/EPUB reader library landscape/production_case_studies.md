# Production case studies: how open source ebook readers are actually built

State as of 2026-11-07. Evidence labels used throughout: **[source]** means I read it in a shallow clone or the GitHub API on 2026-11-07. **[tracker]** means an issue, PR or commit message. **[secondary]** means a third-party or forum account. Anything older than 2026 is dated inline. Line counts are raw `wc -l` (blanks and comments included, tests excluded unless stated), so treat them as rough scale, not effort. Pinned revisions: Readest `5a105f4` (v0.12.20), readest/foliate-js `b34a0aa`, johnfactotum/foliate-js `78914ae`, Foliate `67b6676`, Thorium Reader `b00cee5`, Calibre `2abf07c`, KOReader `9040116`, crengine `4ce1aed`.

## Which engine does each app use, and did it fork, vendor or patch it?

### Takeaway

Nobody who ships a serious reader uses an engine unmodified. Every web-based production reader either owns its engine (Foliate, Calibre, Kavita), has effectively absorbed it (Thorium moved Readium's Electron code in-tree in February 2026), or carries a heavy fork (Readest's foliate-js fork has more changed lines than upstream has lines in total).

### Cited Findings

| App | Stack | Engine | Relationship to upstream | Measured diff or size | Licence | Status |
|---|---|---|---|---|---|---|
| Readest | Tauri 2 + Next/React | foliate-js | Fork as git submodule, plus a forked Tauri | +9,338/-840 lines vs upstream, 273 commits ahead, 14 behind | AGPL-3.0 (fork is MIT) | Very active |
| Foliate | GJS + GTK4 + WebKitGTK | foliate-js | Same author, plain submodule | None; pinned to a 2026-03-05 commit | GPL-3.0 | Last push 2026-04-08; latest release 3.3.0, April 2025 |
| Thorium Reader | Electron 44 + React | Readium r2-*-js | Moved in-tree February 2026 | About 68k lines in `src/r2-xxx-js` | BSD-3-Clause | Active, v3.5.1 |
| Thorium Web | Next 16 + React 19 | Readium ts-toolkit | npm dependency | 30.7k lines over the toolkit | BSD-3-Clause | Active, v1.7.0 |
| Koodo Reader | Electron 35 + React | `kookit` (closed, minified) | Reported to derive from foliate-js | 1.1 MB minified blob | AGPL-3.0 repo | Active, v2.4.7 |
| Anx Reader | Flutter + forked inappwebview | foliate-js | Vendored into `assets/` | paginator 1,403 lines vs 1,130; 2,583-line `book.js` bridge | MIT | Active |
| BookLore / Grimmory | Angular web | foliate-js | Vendored into `src/assets/foliate` | view.js +79, epub.js +135 lines, new `streaming-loader.js` | AGPL-3.0 | Both active |
| Calibre viewer | Python + Qt WebEngine + RapydScript | In-house | Not applicable | 20.4k lines of `.pyj` | GPL-3.0 | Active |
| Kavita | Angular + .NET | In-house CSS columns; VersOne.Epub server-side | Not applicable | 7.5k UI + 2.1k `BookService.cs` | GPL-3.0 | Active |
| Komga | Vue | R2D2BC (`@d-i-t-a/reader`) | Own stale fork | 41 ahead, 397 behind | MIT | Active |
| Calibre-Web | Flask | epub.js + reader.min.js | Vendored minified | 190-line glue file | GPL-3.0 | Active |
| Alexandria | Tauri 1 + React | epub.js | Own fork `@btpf/epubjs` | 12 ahead, 5 behind | GPL-3.0 | Last push 2025-12 |
| Lector | PyQt5 | Hand-written parser, Qt widgets | Not applicable | 8.5k lines of Python | GPL-3.0 | Last commit in default history 2021-12 |
| KOReader | LuaJIT | crengine (CoolReader fork) | Long-lived fork | 174k C++ lines | AGPL-3.0 | Very active |

- **[source]** Readest pins forks of foliate-js and Tauri, plus simplecc-wasm, js-mdict, tauri-plugin-turso and tauri-plugin-webview-upgrade, as submodules — [.gitmodules](https://github.com/readest/readest/blob/5a105f4/.gitmodules)
- **[source]** Fork diff against upstream `main`, excluding `vendor/`: 19 files, +9,338/-840 lines. Merge base is dated 2025-11-28; the fork is 273 commits ahead and 14 behind — [compare](https://github.com/johnfactotum/foliate-js/compare/main...readest:foliate-js:b34a0aa)
- **[source]** Upstream foliate-js has 7,104 lines of top-level JS. The fork's paginator alone is 4,447 lines against upstream's 1,130, fixed-layout is 2,242 against 319, and epub.js is 1,477 against 1,083 — [fork paginator](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js); [upstream paginator](https://github.com/johnfactotum/foliate-js/blob/78914ae/paginator.js)
- **[source]** Activity in 2026: 232 fork-only commits against 14 upstream. Huang Xin wrote 247 of the 273 commits ahead of the merge base. Upstream's last commit is 2026-05-01 — [upstream commits](https://github.com/johnfactotum/foliate-js/commits/main)
- **[source]** Readest's Tauri fork is small: 5 commits ahead of `tauri-apps/tauri` dev and 235 behind (home indicator, NSIS language, listener guard, CEF stub, mobile main-thread fix) — [readest/tauri](https://github.com/readest/tauri/tree/readest)
- **[tracker]** The upstreaming attempt failed. Readest's PR #43 was declined item by item because John Factotum does not support older browsers and the pdf.js changes conflicted with native ES modules. It was closed unmerged on 2025-03-18 — [foliate-js #43](https://github.com/johnfactotum/foliate-js/issues/43)
- **[source]** Upstream says of itself that the library is not stable, may change at any time, has no release, and should be used as a submodule — [foliate-js README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md)
- **[tracker]** Thorium's `r2-xxx-js` packages moved into its own source tree on 2026-02-09 — [PR #3383](https://github.com/edrlab/thorium-reader/pull/3383). The PR body gives no reason.
- **[source]** That tree is about 68k lines: r2-navigator-js 38.6k, r2-shared-js 13.2k, r2-streamer-js 6.5k, r2-lcp-js 4.3k, r2-utils-js 4.3k, r2-opds-js 2.3k. The navigator lives under `electron/`, with a 6,034-line `webview/preload.ts` — [src/r2-xxx-js](https://github.com/edrlab/thorium-reader/tree/b00cee5/src/r2-xxx-js)
- **[source]** Older Readium code is archived: `readium/readium-js` was archived, with last push in 2022 — [readium-js](https://github.com/readium/readium-js)
- **[source]** Thorium Web depends on `@readium/navigator` ^2.11.1 with React 19 and Next 16.2 — [package.json](https://github.com/edrlab/thorium-web/blob/main/package.json). The navigator is 19.4k lines and the injectables 9.6k lines — [ts-toolkit](https://github.com/readium/ts-toolkit)
- **[source]** Koodo ships `src/assets/lib/kookit.min.js` (1.1 MB) and two `kookit-extra` builds with no source in the repo — [src/assets/lib](https://github.com/koodo-reader/koodo-reader/tree/main/src/assets/lib)
- **[tracker]** A user asked for kookit to be opened and said it is based on foliate-js. That is a user's claim, not the maintainer's — [Koodo #1113](https://github.com/koodo-reader/koodo-reader/issues/1113). My string check of the blob found `makeZipLoader` twice and `foliate` once, and no `ePub(` or `epubjs`. This is consistent with the claim but does not prove it.
- **[source]** Komga pins `github:gotson/R2D2BC#fork` — [package.json](https://github.com/gotson/komga/blob/master/komga-webui/package.json). The fork is 41 ahead and 397 behind `d-i-t-a:production`, last pushed 2024-02 — [gotson/R2D2BC](https://github.com/gotson/R2D2BC)
- **[source]** Komga turns off R2D2BC's bookmarks, annotations, TTS, search and timeline in the `rights` block, and ships copies of ReadiumCSS — [EpubReader.vue](https://github.com/gotson/komga/blob/master/komga-webui/src/views/EpubReader.vue)
- **[source]** Alexandria depends on `@btpf/epubjs` and is still on Tauri 1 (`@tauri-apps/api ^1.4.0`) — [package.json](https://github.com/btpf/Alexandria/blob/main/package.json). The fork is 12 ahead and 5 behind futurepress, last pushed 2023-11 — [btpf/epub.js](https://github.com/btpf/epub.js)
- **[source]** Kavita uses VersOne.Epub 3.3.6 and HtmlAgilityPack server-side, with no client ebook engine — [Kavita.Services.csproj](https://github.com/Kareadita/Kavita/blob/develop/Kavita.Services/Kavita.Services.csproj)
- **[source]** Anx vendors foliate-js and forks `flutter_inappwebview` — [pubspec.yaml](https://github.com/Anxcye/anx-reader/blob/main/pubspec.yaml)
- **[source]** `koreader/crengine` describes itself as a fork that cross-pollinates with CoolReader, and is not a GitHub fork — [koreader/crengine](https://github.com/koreader/crengine)
- **[secondary]** BookLore's repository reportedly vanished in March 2026 amid a dispute over AI-generated code, and Grimmory is a community fork. These are forum accounts with no official statement — [Lemmy thread](https://lemmy.klein.ruhr/post/218663); [AUR comments](https://aur.archlinux.org/packages/booklore). **[source]** `booklore-app/booklore` exists again now, with 1.3k stars against Grimmory's 4.7k — [Grimmory](https://github.com/grimmory-tools/grimmory)

### Inferences

- A foliate-js user should plan to hold a fork. Upstream's stated scope (latest browsers, no stability promise) plus its 2026 commit rate means the integrator owns WebKit fixes. Readest shows the outcome: it is the de facto maintained line of foliate-js.
- Readest's fork is MIT and carries the macOS and iOS WebKit fixes. For this project it is a better base than upstream, subject to the licensing section below.
- Thorium's in-tree move suggests r2-navigator-js now has one meaningful consumer. Its Electron `webview` and preload design looks non-portable to WKWebView.

### Gaps

- crengine's diff against CoolReader could not be measured because the repos are not related on GitHub.
- The version of Calibre-Web's `epub.min.js` could not be read from the minified file.
- Lector's rendering widget was only partly confirmed (`QGraphicsView` for comics); the text widget was not confirmed.

## How much reader code sits on top of the engine?

### Takeaway

Pagination is small and everything around it is large. By these counts the engine runs from about a tenth of a finished reader (Calibre, Readest) to under half (Foliate, the leanest). A minimal reader needs about 8k lines over foliate-js (Foliate), a complete in-house viewer about 30k (Calibre), and a maximal one about 60k in the reader module alone (Readest).

### Cited Findings

| Project | Engine or core | Reader layer | Notes |
|---|---|---|---|
| Foliate | foliate-js 7.1k | 8.5k GJS total | Includes library and OPDS |
| Calibre | `paged_mode` 988 + `flow_mode` 703 + `cfi` 984 | 20.4k `.pyj` + 8.9k Qt Python + 1.1k `render_book.py` | Full in-house stack, about 30k |
| Readest | Fork about 9.7k in its main 7 files | 61.4k in `src/app/reader` | Whole non-test `src` is about 286k |
| Thorium | r2-xxx-js 68k | 26k reader renderer, 14.7k sagas | Whole `src` 203k |
| Koodo | kookit (closed) | 86.9k in `src` | Excludes the blob |
| BookLore | Vendored foliate 6.5k | 10.3k Angular | Web only |
| Kavita | None | 7.5k UI + 2.1k server | `book-reader.component.ts` alone is 2,715 |
| Komga | R2D2BC | 1,071 | Deliberately thin, features disabled |
| Calibre-Web | epub.js | 190 | Glue only |
| Alexandria | epub.js fork | 11.8k | Includes Rust |
| KOReader | crengine 174k C++ | 31.5k Lua in reader modules + `credocument.lua` 2,077 | Native |

Module breakdown, in lines:

| Concern | Readest | Calibre `.pyj` | Foliate | KOReader Lua |
|---|---|---|---|---|
| Annotation | 8,044 annotator + 1,343 service + 1,439 notebook | `highlights` 1,130 + `annotations` 313 + `selection_bar` 1,225 | `annotations.js` 722 | `readerhighlight` 3,073 + `readerannotation` 577 |
| TOC, search, sidebar | 4,747 sidebar | `toc` 375 + `search` 490 + `search_worker` 298 | `toc.js` 110 + `search.js` 145 | `readertoc` 1,534 + `readersearch` 1,169 |
| Settings | 20,121 `components/settings` (app-wide) | `read_book/prefs/*` (colors 316, keyboard 216, head_foot 287, selection 264) | Inside `book-viewer.js` 1,083 | `readertypeset` 579 + `readertypography` 881 |
| Theming, publisher CSS | `utils/style.ts` 2,114 + transformers 1,221 | Part of `iframe` 1,146 | `themes.js` 113 | `readerstyletweak` 1,262 + `css_tweaks` 1,190 |
| Progress, footer | `footerbar` 1,520 + `ProgressBar` 532 + `useProgressSync` 549 | `overlay` 1,372 | `navbar.js` 237 | `readerfooter` 2,776 + `readerrolling` 1,990 |
| Gestures, input | `iframeEventHandlers` 940 + `useIframeEvents` 646 + `usePagination` 767 | `touch` 402 + `shortcuts` 467 | In `reader.js` 557 | Framework-level |
| Footnotes | `FootnotePopup` 1,009 + `transformers/footnote` | `footnotes` 316 + `content_popup` 190 | Engine-level | `readerlink` 1,679 |
| Engine host | `FoliateViewer.tsx` 1,568 | `view` 1,668 | `reader.js` 557 + `webview.js` 207 | `credocument` 2,077 |
| Sync | 14,272 `services/sync` | `db` 578 (local only) | None | Plugin |
| TTS | 15,088 service + 2,977 UI | `tts` 343 + `read_aloud` 284 | `tts.js` 198 + `speech.js` 161 | Not counted |

All figures above are **[source]**, counted from these trees: [Readest reader](https://github.com/readest/readest/tree/5a105f4/apps/readest-app/src/app/reader), [Calibre read_book](https://github.com/kovidgoyal/calibre/tree/2abf07c/src/pyj/read_book), [Foliate src](https://github.com/johnfactotum/foliate/tree/67b6676/src), [KOReader modules](https://github.com/koreader/koreader/tree/9040116/frontend/apps/reader/modules), [crengine](https://github.com/koreader/crengine/tree/4ce1aed/crengine), [Thorium src](https://github.com/edrlab/thorium-reader/tree/b00cee5/src).

- **[source]** Readest's test tree is larger than its product code: about 301k lines across 1,504 files in `src/__tests__`, against about 286k non-test lines in `src` — [__tests__](https://github.com/readest/readest/tree/5a105f4/apps/readest-app/src/__tests__)
- **[source]** Readest's own notes say agents write, review and merge most of its code, so its line counts overstate human effort — [LESSONS.md](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/LESSONS.md)

### Inferences

- A realistic first target for this project is the Foliate tier: about 5k to 8k lines of TypeScript over the engine, covering host, TOC, search, annotations, settings and progress. A library and Kobo logic already exist, which removes a large part of Foliate's total.
- Readest's numbers are not a budget. Much of its size is sync, TTS, dictionaries and six platforms. Its annotator (8k) is a useful warning, though: selection UX costs more than page layout.
- Calibre's core layout code at about 1.7k lines shows that CSS-column pagination is not where the mass is. The bulk is edge cases and surrounding UI.

### Gaps

- Modules were assigned by directory and file name, not by reading every file, so boundaries are approximate. Thorium and Koodo were not split by feature.

## What did they have to write themselves or work around?

### Takeaway

The same few problems recur everywhere: CSS multi-column is slow and quirky, publisher CSS defeats user settings, whole-book single-file EPUBs freeze layout, position drifts on reflow, and WebKit needs its own scroll, zoom and memory fixes. Each one was found through user reports on real books.

### Cited Findings

#### Pagination and layout

- **[source]** foliate-js uses CSS multi-column like epub.js and says it shares the limits: slow, some CSS misbehaves, other bugs — [README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md)
- **[source]** Calibre uses columns too and carries the same scars: a `ScrollResizeBugWatcher` class for a Chrome bug where `scrollWidth` grows after layout, plus workarounds for `getBoundingClientRect()` in column mode and SVG sizing — [paged_mode.pyj](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/pyj/read_book/paged_mode.pyj)
- **[tracker]** Kavita posted a $200 bounty for text bleeding between pages in column layout — [Kavita #4040](https://github.com/Kareadita/Kavita/issues/4040). An older column bug cropped the left edge of text — [#2013](https://github.com/Kareadita/Kavita/issues/2013)
- **[tracker]** Readest fork fixes in the same class: "demote over-tall inline-block boxes so columns don't clip content" (#27), "reach the last page when the book ends partway into a spread" (#147), and turning pages horizontally for `vertical-rl` (#45) — [fork commits](https://github.com/readest/foliate-js/commits/b34a0aa)
- **[tracker]** `vh` units inside books produced blank second pages until Readest made them resolve against the right viewport — [Readest #1027](https://github.com/readest/readest/issues/1027)
- **[tracker]** A small-screen case needed two flips to see text that filled half a page, 37 comments — [Readest #1470](https://github.com/readest/readest/issues/1470)
- **[tracker]** Readium hit the same class of problem: a Firefox virtual-column workaround and a WebKit scroll fix — [ts-toolkit #118](https://github.com/readium/ts-toolkit/issues/118); [#138](https://github.com/readium/ts-toolkit/issues/138)

#### Large books and performance

- **[tracker]** An EPUB with nearly all text in one HTML file froze the whole app in paged mode — [Readest #6155](https://github.com/readest/readest/issues/6155). The answer, built October 2026, is a new 235-line module that renders huge sections in chunks while keeping CFIs valid. It cites a 20 MB concordance with about 85k top-level elements, and multi-column pre-paint walking every sibling once per column — [section-chunks.js](https://github.com/readest/foliate-js/blob/b34a0aa/section-chunks.js)
- **[tracker]** More fixes: link-heavy single-file sections hanging (#6678), chapter loading split to reduce swipe stalls (#6266), and bounded-concurrency resource replacement — [Readest commits](https://github.com/readest/readest/commits/5a105f4)
- **[tracker]** Long TOCs slowed book opening — [Readest #386](https://github.com/readest/readest/issues/386)
- **[tracker]** A 2,815-book `library.json` of about 12 MB made even page turns laggy, and the maintainer plans a SQLite migration — [Readest #3714](https://github.com/readest/readest/issues/3714)
- **[tracker]** Foliate's rewrite cited epub.js loading whole files and needing every section processed for the slider — [Foliate #962](https://github.com/johnfactotum/foliate/issues/962)
- **[tracker]** Thorium crashed on a Bookshare book, 72 comments, and on large LCP PDFs — [Thorium #2814](https://github.com/edrlab/thorium-reader/issues/2814); [#3390](https://github.com/edrlab/thorium-reader/issues/3390)

#### Publisher CSS

- **[source]** KOReader has a 1,190-line catalogue of style tweaks and a 1,262-line tweak module; crengine's CSS engine alone is 8,551 lines — [css_tweaks.lua](https://github.com/koreader/koreader/blob/9040116/frontend/ui/data/css_tweaks.lua); [lvstsheet.cpp](https://github.com/koreader/crengine/blob/4ce1aed/crengine/src/lvstsheet.cpp)
- **[tracker]** Line spacing did nothing where books used `<div>` paragraphs, so Readest added a selector for divs that contain only inline children — [Readest #1130](https://github.com/readest/readest/issues/1130). Margins, line length and font size drew 23 comments — [#5301](https://github.com/readest/readest/issues/5301)
- **[tracker]** Authored `text-wrap: pretty` produced wide word gaps (#5582), and Readest tracks regressions where fixes skipped an "Override Book Layout" mode — [LESSONS.md](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/LESSONS.md)
- **[tracker]** Kavita images vanished on dark, black or white themes — [Kavita #2723](https://github.com/Kareadita/Kavita/issues/2723)
- **[source]** Readest runs a transformer pipeline over content before display, including footnote, sanitizer, style, punctuation, nbsp, language and whitespace passes — [transformers](https://github.com/readest/readest/tree/5a105f4/apps/readest-app/src/services/transformers)
- **[source]** Calibre does this server-side before display: `render_book.py` transforms HTML, stylesheets and SVG and virtualises resources — [render_book.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/srv/render_book.py)

#### Footnotes

- **[source]** Upstream's footnote module is 103 lines; the fork's is 253, and Readest adds a 1,009-line popup — [fork footnotes.js](https://github.com/readest/foliate-js/blob/b34a0aa/footnotes.js); [FootnotePopup.tsx](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/components/FootnotePopup.tsx)
- **[tracker]** Footnote bugs keep coming: hidden notes spoken by TTS (#4135), popup themes (#5267), and selections jumping next to markers (#6566) — [Readest #4135](https://github.com/readest/readest/issues/4135). Thorium's endnotes popup took 30 comments — [Thorium #2478](https://github.com/edrlab/thorium-reader/issues/2478)

#### Fixed layout

- **[source]** Fixed layout grew from 319 to 2,242 lines in the fork, about 7 times. Much of that is PDF, zoom and scroll-strip work rather than EPUB alone — [fixed-layout.js](https://github.com/readest/foliate-js/blob/b34a0aa/fixed-layout.js)
- **[tracker]** Komga users complained of poor UX for EPUB comics, and Komga routed compatible EPUBs to its Divina reader — [Komga #1324](https://github.com/gotson/komga/issues/1324)

#### Position and pagination numbers

- **[tracker]** Page numbers are a persistent complaint because reflowable pages are estimates. Readest's maintainer rejected one sampling approach as unreliable — [Readest #3271](https://github.com/readest/readest/issues/3271). A later commit calibrates page readouts with real screen counts — [#7060](https://github.com/readest/readest/commits/5a105f4)
- **[tracker]** Kavita resumes on the wrong page, still open since 2024 — [Kavita #3022](https://github.com/Kareadita/Kavita/issues/3022)
- **[tracker]** Most relevant to a Kobo manager: a web reader's progress update made the Kobo open at the wrong section — [Grimmory #36](https://github.com/grimmory-tools/grimmory/issues/36). Readest keeps both CFI and KOReader XPointers, and needed work to resolve element-offset XPointers (#5111) — [book-config.md](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/docs/book-config.md)

#### Security

- **[source]** Because of [WebKit bug 218086](https://bugs.webkit.org/show_bug.cgi?id=218086), iframes need `allow-scripts` for events to work, which foliate-js says makes the sandbox useless, so CSP must block scripts — [README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md)
- **[tracker]** Grimmory removed `allow-scripts` on 2026-04-22, then three days later switched to "use CSP for protecting foliate instead of iframe sandbox" — [Grimmory commit](https://github.com/grimmory-tools/grimmory/commit/30434e4dc)
- **[tracker]** Alexandria, also Tauri, received an "Arbitrary file read vulnerability" report in 2024 (title read only) — [Alexandria #46](https://github.com/btpf/Alexandria/issues/46)

### Inferences

- Budget for a corpus of awkward real books rather than a feature checklist. Most fixes above were triggered by a specific file.
- Keep CFI as the primary locator but design for a second one. A Kobo-integrated reader needs a mapping between EPUB CFI and Kobo's position format, and Grimmory #36 shows how that fails visibly.
- Apply the sanitizer and CSP from the first commit; do not count on the iframe sandbox. The Grimmory reversal is probably that WebKit bug in practice, though its commit text does not say so.

### Gaps

- I read commit titles rather than every patch, so some attributions of cause come from the commit or issue text alone.

## Did any app migrate between engines, and why?

### Takeaway

The confirmed direction of travel is away from epub.js, and towards owning the code. The most instructive migration is at the webview layer: Readest dropped WebKitGTK for CEF on Linux in September 2026 but stayed on WKWebView on macOS.

### Cited Findings

- **[tracker]** Foliate moved from epub.js to its own renderer (announced 2022-10-19, shipped in 3.0, 2023). Stated reasons: whole-file loading, TOC items needing conversion to CFIs, Mobi extraction to disk, a two-column cap, no 1:1 swipe, flashing on resize, position loss, and word counting that likely broke Japanese jumps — [Foliate #962](https://github.com/johnfactotum/foliate/issues/962)
- **[tracker]** The same thread records that continuous scrolling was first left out for complexity and flashing "especially on WebKit", then users said it was essential — [Foliate #962](https://github.com/johnfactotum/foliate/issues/962). It remains an open request as of 2025 — [Foliate #1545](https://github.com/johnfactotum/foliate/issues/1545)
- **[tracker]** Readest shipped Linux on Tauri's CEF runtime in September 2026, and macOS, Windows and mobile stayed on the Tauri fork — [PR #6074](https://github.com/readest/readest/pull/6074). Nothing installable runs on WebKitGTK any more — [PR #6218](https://github.com/readest/readest/pull/6218)
- **[tracker]** The WebKitGTK pain behind that shows up in crashes and rendering reports, including a SEGV opening EPUBs — [Readest #4115](https://github.com/readest/readest/issues/4115) — and the maintainer telling a user to wait for the CEF build — [#2772](https://github.com/readest/readest/issues/2772)
- **[source]** The cost: a separate lockfile, a Linux-only `[patch]` set, and a warning that plugin bumps can silently drop patches — [cef.toml](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/.cargo/cef.toml)
- **[tracker]** Jellyfin, July to August 2026: a contributor called epub.js abandoned after a last-page bug on Android. A maintainer warned foliate-js is hardly more active, wanted typings, rejected Readium because it expects a server-generated manifest, and rejected depending on unofficial epub.js forks — [jellyfin-web #8278](https://github.com/jellyfin/jellyfin-web/discussions/8278). Note futurepress/epub.js did see a push in 2026-03 — [epub.js](https://github.com/futurepress/epub.js)
- **[secondary]** Calibre 4.0 (2019) required rewriting the viewer because of the move from Qt WebKit to Qt WebEngine — [New in calibre 4.0](https://calibre-ebook.com/new-in/thirteen)
- **[tracker]** Thorium's in-tree move of Readium packages, February 2026 — [PR #3383](https://github.com/edrlab/thorium-reader/pull/3383)
- **[source]** No epub.js commits exist in Koodo's searchable history, so I could not confirm a Koodo migration by that route — [Koodo](https://github.com/koodo-reader/koodo-reader)

### Inferences

- epub.js is a poor starting point in 2026: Calibre-Web and Alexandria remain on it but neither shows much investment, and Foliate's reasons still apply.
- A CEF path on macOS exists in principle, but Readest did not need it there. That is evidence that WKWebView is workable with fixes, while WebKitGTK was not.

### Gaps

- I found no Readest statement about ever using epub.js; the evidence suggests it started on foliate-js. Search results found no primary statement either way.
- BookLore's history could not be checked because the available clone was shallow and commit search found nothing.

## Readest specifically: how does it work on Tauri and macOS?

### Takeaway

Readest does its parsing in the webview with zip.js and foliate-js, reads files by ranged fetches through the asset protocol on macOS, stores per-book JSON, and uses a permissive CSP. Code cannot be copied into a GPLv3 project without AGPL terms following it, but the foliate-js fork is MIT and is the part that matters most.

### Cited Findings

#### Loading books

- **[source]** On macOS desktop, ordinary local paths become a `RemoteFile` that does ranged reads over the Tauri asset protocol. The comment says it is about 2 times faster than `NativeFile` because of [tauri#9190](https://github.com/tauri-apps/tauri/issues/9190). It falls back to `NativeFile` on failure — [nativeAppService.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/services/nativeAppService.ts)
- **[source]** `NativeFile` extends `File`, reads through the Tauri fs handle, and keeps an LRU of up to 50 chunks of 1 MB — [file.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/utils/file.ts)
- **[source]** Android and Linux use a `rangefile` custom scheme instead because Chromium re-applies `Range` offsets to intercepted bodies. iOS uses `NativeFile` — [range_file.rs](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/src/range_file.rs)
- **[source]** `DocumentLoader` opens the file with zip.js `BlobReader` inside the webview. On Tauri, the Rust `parse_epub_full` command prefetches container.xml, OPF, nav and NCX so foliate-js skips zip.js for those files — [document.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/libs/document.ts)
- **[source]** Rust deliberately does not parse OPF metadata, because reimplementing it would diverge from foliate-js; it does hashing and cover resizing — [epub_parser.rs](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/src/epub_parser.rs)
- **[source]** Sections and resources reach iframes as `blob:` URLs through `URL.createObjectURL`, and the iframe has `sandbox="allow-same-origin allow-scripts"` — [epub.js](https://github.com/readest/foliate-js/blob/b34a0aa/epub.js); [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js)
- **[tracker]** blob-URL iframes break browser extensions on the web build — [Readest #889](https://github.com/readest/readest/issues/889)

#### CSP

- **[source]** `tauri.conf.json` CSP: `default-src 'self' 'unsafe-inline' blob: data: customprotocol: asset: http://asset.localhost http://rangefile.localhost ipc: http://ipc.localhost`; `script-src` includes `'unsafe-inline' 'unsafe-eval' data: blob: asset:` plus Sentry, PostHog and Stripe; `frame-src 'self' blob: asset:`; `connect-src` includes `http://*:* https://*:*`. The asset scope allows `$APPDATA/**`, `$TEMP/**` and `**/Readest/**/*` — [tauri.conf.json](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/tauri.conf.json)
- **[source]** That policy is far looser than foliate-js's advice. Foliate's own reader uses `script-src 'self'`, `frame-src blob: data:`, `form-action 'none'` — [reader.html](https://github.com/johnfactotum/foliate/blob/67b6676/src/reader/reader.html)
- **[source]** Readest's protection instead is a DOMPurify transformer on book content unless the user sets `allowScript`, which also unlocks evaluation of inline scripts on Tauri — [sanitizer.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/services/transformers/sanitizer.ts); [FoliateViewer.tsx](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/components/FoliateViewer.tsx)
- **[source]** Foliate instead loads through a registered `foliate:` URI scheme — [webview.js](https://github.com/johnfactotum/foliate/blob/67b6676/src/webview.js)

#### macOS specifics

- **[source]** macOS runs Wry on WKWebView with minimum macOS 12.0. Custom code covers traffic lights, window close, the system dictionary popover, menus, and Safari and Apple auth — [src-tauri/src/macos](https://github.com/readest/readest/tree/5a105f4/apps/readest-app/src-tauri/src/macos)
- **[source]** Close-to-hide needs an AppKit workaround because macOS 26 Tahoe can leave a focused black phantom window (#4875), and an earlier fix made the close button minimise (#5240) — [window.rs](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/src/macos/window.rs)
- **[source]** `gpu-composite` restores persistent compositor layers on Apple WebKit so page turns stay smooth on ProMotion — [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js)

#### Persistence

- **[source]** Each book has `<bookHash>/config.json` under `Books`, with `schemaVersion`, `progress`, `location` (CFI), `xpointer`, `booknotes`, `viewSettings` and `updatedAt`. Notes carry `cfi` plus `xpointer0` and `xpointer1` — [book-config.md](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/docs/book-config.md); [book.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/types/book.ts)
- **[source]** Progress saves are debounced about 1.5 seconds after the last turn, with a flush when the app is hidden so the last turns survive being killed — [useProgressAutoSave.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/hooks/useProgressAutoSave.ts)
- **[source]** Shelf metadata sits in `library.json`, and a Turso or libSQL layer with migrations exists for the database service — [architecture.md](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/docs/architecture.md)
- **[tracker]** The maintainer called moving reading data to SQLite sensitive and wants more field testing first — [Readest #3714](https://github.com/readest/readest/issues/3714)

#### Recurring WKWebView and Safari issues

| Symptom | Evidence |
|---|---|
| Scroll offset clamped after transforms, so the last page snaps back (iOS 18) | [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js) comment, readest#5663; fork #73 |
| No reliable cross-iframe scroll anchoring, so reopening drifts to chapter start | [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js) comment |
| `document.fonts.ready` crashes WebContent before Safari 17 | [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js); [commit #5654](https://github.com/readest/readest/commits/5a105f4) |
| Client rects do not reflect zoom, unlike Chrome, Edge and Firefox | [overlayer.js](https://github.com/readest/foliate-js/blob/b34a0aa/overlayer.js) |
| Glyph clipping, fixed by `-webkit-line-box-contain` | [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa/paginator.js) |
| Text jitter in scroll mode on macOS with always-visible scrollbars (fixed 0.9.22, March 2025) | [#600](https://github.com/readest/readest/issues/600) |
| Text missing after an update on macOS 15.4 (April 2025) | [#903](https://github.com/readest/readest/issues/903) |
| Library covers blank until resize (October 2026) | [#6806](https://github.com/readest/readest/issues/6806) |
| Weak hyphenation: WebKit on Apple platforms asks CoreFoundation rather than using patterns; left as is | [#5749](https://github.com/readest/readest/issues/5749) |
| Safari font line-height differences in EPUBs | [commit #4120](https://github.com/readest/readest/commits/5a105f4) |
| Local M4B silent on macOS, fixed with a media proxy | [#6848](https://github.com/readest/readest/issues/6848) |
| `CanvasRenderingContext2D.filter` ignored; WKWebView is the last holdout | [PR #6218](https://github.com/readest/readest/pull/6218) |
| Old WebKit lacks lookbehind regex, top-level await, constructable stylesheets | [foliate-js #43](https://github.com/johnfactotum/foliate-js/issues/43) |
| Open upstream proposals for WebKit zoom and layout | [#165](https://github.com/johnfactotum/foliate-js/issues/165); [#166](https://github.com/johnfactotum/foliate-js/issues/166) |

Search counts for the tracker: "webkit" 34 issues, "safari" 43, "WKWebView" 7; commit search "webkit" 66, "safari" 32, "wkwebview" 21 — [issues](https://github.com/readest/readest/issues). Note that much of the severe list above is iOS. macOS desktop is the milder half.

#### Licensing

- **[source]** Readest is AGPL-3.0, and its `readest/foliate-js` fork keeps the MIT licence — [readest](https://github.com/readest/readest); [fork](https://github.com/readest/foliate-js)
- **[source]** GPLv3 section 13 permits combining with AGPLv3 code into a single work. The GPL still applies to the GPL part, but AGPL's network clause applies "to the combination as such" — [GPLv3](https://www.gnu.org/licenses/gpl-3.0.txt)
- **[source]** AGPL section 13 requires a modified version to offer source to users interacting with it remotely over a network, if it supports such interaction — [AGPLv3](https://www.gnu.org/licenses/agpl-3.0.txt)

### Inferences

- Copying Readest app code cannot relicense it to GPLv3. Those files stay AGPL inside a combined work. For a desktop app with no remote users the practical burden is small, but the project would stop being purely GPLv3, and any later hosted or remote mode would trigger section 13. This is my reading, not legal advice.
- Safe to take without AGPL strings: the foliate-js fork (MIT), Foliate and Calibre (GPLv3), Thorium and Readium (BSD-3), Komga and Anx (MIT). KOReader and Koodo are AGPL. Ideas and bug knowledge from Readest are free to reuse, so reimplement from its comments and issues rather than copying its code.
- Copy the asset-protocol ranged-read pattern first, and benchmark against tauri#9190 rather than trusting the 2 times figure.
- Tighten CSP compared with Readest. Its loose policy serves Sentry, Stripe, translators and remote catalogues that this app may not need. Foliate's reader policy is the better template.
- Avoid a single growing JSON file for library-wide state, which is Readest's `library.json` problem. Per-book data is less exposed to it.

### Gaps

- The WKWebView behaviours were not reproduced here. They come from code comments and issues. The iOS share of them may not apply on macOS.
- I did not verify current status of tauri#9190 itself.

## What do users complain about most?

### Takeaway

Users rarely complain about engines. Complaints concentrate on progress and sync that loses position, page numbers, input feel, formatting they cannot override, and platform packaging. That is where polishing cost goes.

### Cited Findings

Method: GitHub issues sorted by comments and thumbs-up. This favours old issues and active reporters, so it shows themes rather than frequencies.

| App | Top themes | Examples |
|---|---|---|
| Readest | Sync and backup, TTS, packaging, page-turn feel, page numbers | [#356 WebDAV](https://github.com/readest/readest/issues/356) (+35), [#627 F-Droid](https://github.com/readest/readest/issues/627) (+33), [#258 TTS](https://github.com/readest/readest/issues/258) (70 comments), [#2252 page turn sensitivity](https://github.com/readest/readest/issues/2252) (42), [#1356 progress and notes sync](https://github.com/readest/readest/issues/1356), [#2795 E2EE](https://github.com/readest/readest/issues/2795) |
| Foliate | Windows build, PDF, blank pages from GPU and WebKit | [#115](https://github.com/johnfactotum/foliate/issues/115), [#139](https://github.com/johnfactotum/foliate/issues/139) (+131), [#1107](https://github.com/johnfactotum/foliate/issues/1107), [#1271](https://github.com/johnfactotum/foliate/issues/1271) |
| Thorium | Scroll wheel paging, default settings, persistent page number, tables, MathML | [#1436](https://github.com/edrlab/thorium-reader/issues/1436), [#3283](https://github.com/edrlab/thorium-reader/issues/3283), [#3718](https://github.com/edrlab/thorium-reader/issues/3718), [#3300](https://github.com/edrlab/thorium-reader/issues/3300), [#3362](https://github.com/edrlab/thorium-reader/issues/3362) |
| Koodo | Sync, resume position, wheel paging, total pages | [#1408](https://github.com/koodo-reader/koodo-reader/issues/1408), [#827](https://github.com/koodo-reader/koodo-reader/issues/827), [#487](https://github.com/koodo-reader/koodo-reader/issues/487), [#1510](https://github.com/koodo-reader/koodo-reader/issues/1510) |
| Kavita | Resume page, column bleeding, stretched covers | [#3022](https://github.com/Kareadita/Kavita/issues/3022), [#4040](https://github.com/Kareadita/Kavita/issues/4040), [#3938](https://github.com/Kareadita/Kavita/issues/3938) |
| Calibre-Web | Progress sync, Kobo sync | [#2298](https://github.com/janeczku/calibre-web/issues/2298) (+57), [#1470](https://github.com/janeczku/calibre-web/issues/1470) |
| Grimmory | Kobo sync, cut-off text | [#93](https://github.com/grimmory-tools/grimmory/issues/93), [#36](https://github.com/grimmory-tools/grimmory/issues/36), [#233](https://github.com/grimmory-tools/grimmory/issues/233) |

- **[secondary]** Calibre viewer: touchpad wheel events turned pages too fast after 4.20, and users long asked for better annotations (MobileRead, about 2019 to 2020, so dated). These come from a search summary and I did not open the threads, so the links are candidates, not confirmed locations of each statement — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=4012305); [MobileRead](https://www.mobileread.com/forums/showthread.php?p=3909889)
- **[secondary]** It's FOSS found Readest sometimes forgets window size, while Foliate remembers it — [It's FOSS](https://itsfoss.com/news/readest-app/)
- **[secondary]** A new-reader thread from October to November 2026 shows expert users asking for chapter and book page counts, custom fonts, font weight and less wasted space — [MobileRead CalibReader](https://www.mobileread.com/forums/showthread.php?p=4618880)

### Inferences

- Scroll and trackpad handling needs early attention on a Mac app. Wheel paging recurs in Calibre, Thorium and Koodo.
- Trustworthy resume is the cheapest big win. Debounce and flush on hide, and test it against reflow.
- Foliate's blank-page and Readest's WebKitGTK reports suggest users blame the app for webview faults, so version logging in bug reports pays off.

### Gaps

- Reddit and Hacker News content could not be retrieved: the Redlib mirrors returned 403 or TLS errors and search found no usable HN thread. User sentiment here therefore leans on trackers and MobileRead, and is skewed towards people who file issues.

## What are the quality bars and test corpora, and are engines scored?

### Takeaway

No foliate-js-based reader appears on any public scoreboard I could find. Thorium is the benchmark with near-perfect published results, while W3C results describe whole reading systems and the 3.3 snapshot dates from 2023. A project must build its own corpus and can use these suites as free test books.

### Cited Findings

- **[source]** W3C's EPUB 3.3 implementation report is dated 22 March 2023. My count from its consolidated tables, all feature levels (pass/fail): Colibrio 140/17, Thorium 2.2.0 130/33, Kobo 113/51, Apple Books 109/48, PUBLUS 94/61, tolino 92/42, VitalSource 90/35. Google Play has 124 untested. Not listed: Readium web, foliate-js, Foliate, Readest, Calibre, KOReader — [EPUB 3.3 results](https://w3c.github.io/epub-tests/epub33/results)
- **[source]** The live results page now targets EPUB 3.4 and has a publish date of 2026-09-29. Its `reports` directory holds only two reports, PUBLUS Android and Voyager BinB — [results](https://w3c.github.io/epub-tests/results); [reports](https://github.com/w3c/epub-tests/tree/main/reports)
- **[source]** epubtest.org shows Thorium at 100% basic and non-visual with JAWS and NVDA, and 100% visual adjustments and read aloud on Windows and macOS. Thorium with Narrator scores 81.25% non-visual. Thorium Mobile with VoiceOver scores 93.33% basic. Foliate, Readest, Calibre and Readium are not listed. The page is undated, with a footer of 2019 to 2024 — [epubtest.org results](https://epubtest.org/results)
- **[source]** Same page: Apple Books with VoiceOver on macOS scores 100% basic and 56.25% non-visual. Kobo scores 60% basic with JAWS and VoiceOver, 53.33% with TalkBack — [epubtest.org results](https://epubtest.org/results)
- **[source]** Test books are published there and users already cite them: Readest #600 reproduced with "Basic Functionality (v2.0.0)" — [Readest #600](https://github.com/readest/readest/issues/600); [test book](https://www.epubtest.org/test-books/basic-functionality/2.0.0)
- **[source]** The DAISY-led TIES group runs the reading system evaluation, hosted at EPUBTest.org — [DAISY](https://daisy.org/activities/projects/ties/reading-systems/)
- **[source]** A scripted-content safety book exists at [johnfactotum/epub-test](https://github.com/johnfactotum/epub-test), referenced by the [foliate-js README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md)
- **[source]** W3C i18n also has a reader results page. The search tool reported a Thorium 3.5.1 pass from it, but in the page text that string sits in a sample record format, so do not treat it as a result — [i18n readers](https://w3c.github.io/i18n-tests/results/readers.html)
- **[source]** Readest's own tests include EPUB fixtures, generated crengine XPointer fixtures for KOReader compatibility, Playwright and WebdriverIO tiers — [testing.md](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/docs/testing.md)
- **[source]** KOReader keeps a separate test-data submodule — [.gitmodules](https://github.com/koreader/koreader/blob/9040116/.gitmodules)

### Inferences

- A sensible corpus: epubtest.org fundamentals, W3C epub-tests, the epub-test security book, plus a local stress set of a single-file 20 MB book, a very long TOC, a heavily styled book, vertical Japanese, RTL, fixed layout, and Kobo KEPUBs. The stress set maps directly to the failures above.
- If the app ever claims accessibility, Thorium is the real comparison. Publishing epubtest results would itself be unusual among foliate-js apps.

### Gaps

- epubtest.org results are undated, so I cannot tell how recent they are. W3C has no current data for desktop open source readers.
- I found no test of foliate-js as an engine, so its conformance is unknown. A local run would be needed.

## Accessibility: screen readers with paginated iframes, and keyboard navigation

### Takeaway

The leading accessible reader avoids the problem rather than solving it: when screen reader support is on, Thorium forces scroll mode and stops estimating position. Paginated iframes with CSS columns are not known to be workable for screen readers, and Readest's alternative is a set of injected skip links with side effects.

### Cited Findings

- **[source]** Thorium forces EPUB content to scroll mode instead of paginated when screen reader support is on, to avoid CSS column problems — [Thorium accessibility docs](https://thorium.edrlab.org/en/th3/500_accessibility/_print/)
- **[source]** Screen readers use a separate virtual buffer, so the app may not know the reading position. Thorium disables its automatic location estimate in this mode — [Thorium accessibility docs](https://thorium.edrlab.org/en/th3/500_accessibility/_print/)
- **[source]** Since 3.3 the mode is an explicit setting, because third-party apps requesting assistive privileges caused false detection — [Thorium accessibility docs](https://thorium.edrlab.org/en/th3/500_accessibility/_print/)
- **[source]** Thorium uses a visually hidden, debounced announcement component for navigation — [NavigationAnnouncement.tsx](https://github.com/edrlab/thorium-reader/blob/b00cee5/src/renderer/reader/components/NavigationAnnouncement.tsx)
- **[tracker]** Thorium needed long work to get screen readers to follow links into the content webview, 41 comments, using the epubtest fundamentals book — [Thorium #1594](https://github.com/edrlab/thorium-reader/issues/1594). Focus after TOC and footnote jumps was a separate issue — [#1379](https://github.com/edrlab/thorium-reader/issues/1379)
- **[source]** Readest sets `tabindex="-1"` on all book links and injects a hidden "skip to reading position" link first in the iframe body, because NVDA's landmark navigation fires no DOM events. It also injects a next-section skip link — [a11y.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/utils/a11y.ts)
- **[tracker]** Those injections cost correctness elsewhere: a trailing link created blank pages (#4126), one changed last-paragraph line spacing (#4642), and the CFI code needs to tolerate the injected step — [a11y.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/utils/a11y.ts); [cfi.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/utils/cfi.ts)
- **[source]** Readest has keyboard hooks including `useBookShortcuts` and `useSpatialNavigation` — [hooks](https://github.com/readest/readest/tree/5a105f4/apps/readest-app/src/app/reader/hooks). Thorium's shortcuts are configurable, after a 38-comment design issue from 2018 — [Thorium #155](https://github.com/edrlab/thorium-reader/issues/155)
- **[tracker]** Foliate has an open issue saying its main application areas are inaccessible to assistive technology users, from 2023 — [Foliate #1150](https://github.com/johnfactotum/foliate/issues/1150)
- **[secondary]** Readium CSS paginates with CSS multicolumn on the root inside an iframe — [Readium CSS docs](https://app.unpkg.com/@evidentpoint/readium-css@1.0.0-alpha.2/files/docs/CSS03-injection_and_pagination.md). A 2019 study found validators passed content that screen reader testing showed to have barriers — [Leporini and Meattini review](https://a11y-paradise.onrender.com/reviews/69bae31c7ca019dbf27342f8)

### Inferences

- Copy Thorium's approach: an explicit screen reader setting that switches to a scrolled flow and uses DOM focus for location. It is simpler than Readest's and has published scores behind it.
- macOS VoiceOver is the relevant reader here, and it is the weakest documented case: Apple Books itself scores 56.25% non-visual on macOS. Thorium's documented workarounds are mostly NVDA and JAWS specific, so they may not transfer to VoiceOver; this is untested.
- Keep the shell UI accessible from the start (React with proper roles and focus return), since that is cheap compared with a retrofit.

### Gaps

- I found no authoritative test of VoiceOver with paginated iframes in WKWebView, and no epubtest result for any Tauri app. Needs hands-on testing on real hardware.
- Details of Thorium Web's accessibility in the ts-toolkit navigator were not found.
