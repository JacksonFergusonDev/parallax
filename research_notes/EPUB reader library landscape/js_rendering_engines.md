# JavaScript/TypeScript EPUB rendering engines for a Tauri 2 + React 19 + WKWebView reader

Data snapshot: 2026-11-07, about 18:00 UTC. "12 months" means since 2025-11-07. Evidence tags: **[repo]** GitHub API or npm registry data; **[src]** read in a local clone of the source; **[build]** measured by a local Vite build; **[doc]** project README, docs or issue text. Anything unverified is under Inferences or Gaps. No engine was run inside WKWebView or Tauri in this research, so build results show only that a library compiles.

## Project health: licence, releases, activity, maintainers, production users

### Takeaway

Four engine lines are actively maintained: the Readest fork of foliate-js, Readium ts-toolkit, `@likecoin/epub-ts` (an epub.js rewrite), and prose-reader. Upstream foliate-js is alive but slow (15 commits in 12 months), and upstream epub.js is close to dormant (last npm release February 2022). All the leading candidates have GPLv3-compatible permissive licences. The strongest production evidence for this exact stack is Readest, a Tauri 2 + React 19 app that ships foliate-js on macOS.

### Cited Findings

Health table **[repo]** (commit counts are on the default branch):

| Engine | Licence | Latest npm release | Last commit | Commits 12m | Open issues / PRs | Maintainers |
| --- | --- | --- | --- | --- | --- | --- |
| [foliate-js upstream](https://github.com/johnfactotum/foliate-js) | MIT | None official (see Packaging) | 2026-05-01 | 15 | 39 / 32 | 14 contributors; johnfactotum 268 commits, next is chrox with 12 |
| [readest/foliate-js fork](https://github.com/readest/foliate-js) | MIT | None; used as a submodule | 2026-11-05 | 274 | Not fetched | Huang Xin (chrox) wrote 248 of 274 |
| [epub.js](https://github.com/futurepress/epub.js) | BSD-2-Clause per [npm](https://www.npmjs.com/package/epubjs); GitHub reports NOASSERTION | 0.3.93, 2022-02-16 (**old**); alpha tag 0.5.0-alpha.3 | 2026-03-23 | 3 | 485 / 31 | 87 contributors; fchasen 1,002 commits |
| [react-reader](https://github.com/gerhardsletten/react-reader) | Conflict: repo LICENSE is Apache-2.0, package.json says ISC | 2.0.15, 2025-09-26 | 2026-06-01 (typo fix) | 2 | 5 / 0 | 1 on npm |
| [Readium ts-toolkit](https://github.com/readium/ts-toolkit) | BSD-3-Clause | navigator 2.15.0, shared 2.9.0, 2026-11-06 | 2026-11-06 | 94 | 60 / 5 | 3 on npm; 19 contributors; JayPanoz wrote 81 of the 12-month commits |
| [r2-navigator-js](https://github.com/edrlab/r2-navigator-js) (moved from readium org) | BSD-3-Clause | 1.25.7, 2025-11-24 | 2025-11-24 | 3 | 26 combined | danielweck only in 12m |
| [Thorium Reader](https://github.com/edrlab/thorium-reader) (app) | BSD-3-Clause | v3.5.1, 2026-09-02 | 2026-11-04 | 983 | 154 / 16 | danielweck 488, panaC 324 |
| [Thorium Web](https://github.com/edrlab/thorium-web) (app and components) | BSD-3-Clause | 1.7.0, 2026-10-07 | 2026-10-07 | 624 | Not fetched | 28 authors in 12m |
| [readium/web](https://github.com/readium/web) | BSD-3-Clause | None | 2025-05-29 (**dormant**) | 0 | 6 / 0 | n/a |
| [@likecoin/epub-ts](https://github.com/likecoin/epub.ts) | BSD-2-Clause per npm | 0.7.3, 2026-10-26 | 2026-10-26 | 222 | Not fetched | William Chong wrote 212 |
| [@intity/epub-js](https://github.com/intity/epub-js) | BSD-2-Clause | 0.3.103, 2026-09-26 | 2026-10-25 | 174 | 4 / 0 | intity only |
| [Flow](https://github.com/pacexy/flow) (app with vendored epub.js) | AGPL-3.0 | Not published as a library | 2026-03-28 | 3 | 88 / 8 | 3 authors in 12m, 1 commit each |
| [prose-reader](https://github.com/mbret/prose-reader) | MIT | 40.0.0, 2026-11-05 | 2026-11-05 | 1,317 | 64 combined | mbret only on npm; 504 of the 12-month commits are authored as "Claude" |
| [Vivliostyle](https://github.com/vivliostyle/vivliostyle.js) | AGPL-3.0 | 2.46.0, 2026-11-03 | 2026-11-07 | 1,164 | Not fetched | 8 on npm; 15 authors in 12m |
| [R2D2BC](https://github.com/d-i-t-a/R2D2BC) | Apache-2.0 | 2.5.3, around 2026-06-19 | 2026-06-19 | Not fetched | Not fetched | 68 stars |

Production users:

- Readest v0.12.20 (2026-11-05, 26k stars, AGPL-3.0) uses `readest/foliate-js` as a [git submodule](https://github.com/readest/readest/blob/main/.gitmodules), with React 19.2.8, Next 16.4.2 and `@tauri-apps/api` 2.11.1 **[src]**. Its fork is 273 commits ahead and 14 behind upstream **[repo]**.
- Foliate itself uses foliate-js, but its [last release is 3.3.0 from 2025-04-01](https://github.com/johnfactotum/foliate/releases), which is over 12 months old **[repo]**.
- `@readium/navigator` powers [Thorium Web](https://www.npmjs.com/package/@edrlab/thorium-web), [Stump](https://github.com/stumpapp/stump/blob/main/packages/browser/package.json) (pinned at 2.8.2, on React 19), and [Storyteller](https://github.com/smoores-dev/storyteller) (`^2.10.0`) **[src]**. GitHub code search also listed NatLibFi's `ekirjasto-webreader-frontend` and `readium/playground` **[repo]**.
- [Thorium Reader](https://github.com/edrlab/thorium-reader/blob/develop/package.json) ships the Electron-based r2 stack on Electron 44 and React 18 **[src]**.
- epub.js: Flow vendors it as [`packages/epubjs`](https://github.com/pacexy/flow/tree/main/packages/epubjs) (`@flow/epubjs` 0.3.93, no commits to that directory since 2022-11-24) **[src]**. [Alexandria](https://github.com/btpf/Alexandria) (GPL-3.0, last commit 2025-11-29) uses `@btpf/epubjs` 0.3.95 (last npm change 2023) on `@tauri-apps/api` ^1.4, so it is still Tauri v1 **[repo]**.
- `@likecoin/epub-ts` states it was [built at 3ook.com](https://github.com/likecoin/epub.ts) **[doc]**.
- Monthly npm downloads **[repo]**: epubjs 495,730; react-reader 79,434; @vivliostyle/core 64,938; @readium/navigator 45,134; foliate-js (unofficial package) 34,070; @likecoin/epub-ts 8,108; @prose-reader/core 6,567; r2-navigator-js 3,619; @intity/epub-js 797.
- Recent adopters among Tauri 2 readers: [LightRead](https://github.com/yzfly/LightRead) depends on `foliate-js ^1.0.1`, and [papery-epub-reader](https://github.com/Mu-Wan/papery-epub-reader) on `foliate-js 1.0.1` plus `epubjs` **[src]**.

### Inferences

- Licence fit: MIT, BSD and Apache-2.0 code can be combined into a GPLv3 app. AGPL-3.0 code (Vivliostyle, Flow, Readest's app code) can be combined through GPLv3 section 13, but the AGPL terms then continue to apply to that part. So Readest's React glue should be read for ideas and not copied, while its MIT foliate-js fork is free to use. This is a licence reading and not legal advice.
- Bus factor is low everywhere. Readest's fork depends on one person, ts-toolkit mostly on one (with EDRLab organisational backing), and prose-reader on one, with a large AI-authored share. Readest has the largest user base, so its fork gets the most real-world bug exposure.
- epub.js maintenance is merge-only. Its 2026 fixes (memory leaks, scroll jitter) are on master but in no npm release, because npm's latest is still 0.3.93.

### Gaps

- Open issue and PR counts were not collected for the Readest fork, likecoin, Thorium Web or Vivliostyle.
- Whether Anx Reader (MIT, 9k stars) uses foliate-js could not be confirmed; its `.gitmodules` lookup returned nothing.
- Koodo Reader ships a minified `kookit-extra.min.mjs`, and I did not establish what engine that is.

## Packaging: npm, API stability, vendoring, TypeScript, bundle size, Vite and React 19

### Takeaway

Readium, likecoin, epub.js and prose-reader install normally from npm with TypeScript types, and all built under Vite 7 with React 19. foliate-js must be vendored, has no types, declares itself unstable, and upstream failed a stock Vite build until `pdf.js` was stubbed. The `foliate-js` npm package is not published by the author and should be avoided.

### Cited Findings

Bundle measurements **[build]** used Vite 7.3.7, React 19.3.1 and esbuild minification, installed with `--ignore-scripts`. Each entry imported only the main API, so tree-shaking applies. A baseline entry importing `react` and `react-dom/client` was 216.1 KB min / 67.2 KB gzip; the rows below do not include it unless noted.

| Engine (entry) | Vite result | Total min | Total gzip | Notes |
| --- | --- | --- | --- | --- |
| foliate-js upstream `view.js` | **Failed as-is**; passed with `pdf.js` stubbed | 148.9 KB | 57.5 KB | All lazy chunks, including MOBI, FB2 and zip.js; no PDF |
| epubjs 0.3.93 | Pass | 343.1 KB | 105.2 KB | |
| @likecoin/epub-ts 0.7.3 | Pass | 265.2 KB | 75.8 KB | Project README claims [57.5 KB gzip](https://github.com/likecoin/epub.ts), measured differently |
| react-reader 2.0.15 | Pass | 364.5 KB | 112.4 KB | Includes epubjs and react |
| @readium/navigator + shared | Pass | 503.2 KB | 126.2 KB | Main chunk 358 KB / 96 KB; the rest is lazily loaded ReadiumCSS variants |
| @prose-reader/core + streamer | Pass | 243.2 KB | 73.2 KB | Includes rxjs; excludes any zip library |
| @vivliostyle/core | Pass | 841.3 KB | 238.9 KB | |

- foliate-js README: "This library itself is, however, *not* stable. Expect it to break and the API to change at any time", and "since there's no release yet, it is recommended that you include the library as a git submodule" — [README](https://github.com/johnfactotum/foliate-js#current-status) **[doc]**. Its `package.json` is version 0.0.0 and it is plain ES modules with no build step **[src]**.
- The Vite failure is `Invalid glob: "vendor/pdfjs/*"`, caused by [`pdf.js` line 1](https://github.com/johnfactotum/foliate-js/blob/main/pdf.js), `new URL(`vendor/pdfjs/${path}`, import.meta.url)`, which Vite rewrites into an invalid `import.meta.glob` **[build]**.
- The [`foliate-js` npm package](https://www.npmjs.com/package/foliate-js) (1.0.1, 2025-04-21) has one maintainer, `shmandadi`, not the author, although its metadata points at the upstream repo **[repo]**. Its tarball omits `pdf.js`, adds a `construct-style-sheets-polyfill` dependency, and differs from current main by 136 changed lines in `paginator.js`, 27 in `epub.js`, 31 in `view.js` and 92 in `overlayer.js` **[src]**. Much of that drift could simply be upstream changes since April 2025; I did not audit it for tampering.
- foliate-js has no TypeScript declarations in the repo **[src]**. Its tests are [`tests/epubcfi-tests.js`, `tests.html` and `tests.js`](https://github.com/johnfactotum/foliate-js/tree/main/tests), and CI in the workflows I read is ESLint **[src]**.
- `@readium/navigator` ships types and ESM, with `sideEffects: false` **[src]**. Its [CHANGELOG](https://github.com/readium/ts-toolkit/blob/develop/navigator/CHANGELOG.MD) states it follows Semantic Versioning **[doc]**, but it had 83 releases in 12 months **[repo]**. I found no explicit "stable API" statement. Stump [pins exact versions](https://github.com/stumpapp/stump/blob/main/packages/browser/package.json) **[src]**.
- epubjs ships hand-maintained types in `types/index.d.ts`, with runtime dependencies on `core-js`, `lodash`, `localforage`, `@xmldom/xmldom ^0.7.5` and others, and a 6.4 MB unpacked package **[repo]**.
- `@likecoin/epub-ts` claims it is a drop-in replacement for epubjs with strict TypeScript, one runtime dependency (`jszip`) and 970+ tests — [README](https://github.com/likecoin/epub.ts) **[doc]**. It remains pre-1.0 at 0.7.3 **[repo]**.
- `react-reader` declares no React peer dependency, and its dev dependencies are `react ^19.0.0`, so it installs on React 19 without conflict **[src]**. It depends on `epubjs ^0.3.93`, so it inherits all epub.js limits **[repo]**.
- prose-reader ships types and has a React package whose peer is [`react: 19.x`](https://github.com/mbret/prose-reader/tree/develop/packages/react-reader) **[src]**, but has reached major version 40 with 447 published versions and 175 in 12 months, and its core changelog has 25 lines marked BREAKING **[repo] [src]**. `rxjs` is a peer dependency **[repo]**.
- `@edrlab/thorium-web` is a Next-dependent component package, with `next` 16.2.12 in its dependencies and peers including Redux Toolkit, react-aria and i18next **[repo]**. Its [Implementers' Guide](https://github.com/edrlab/thorium-web/blob/develop/docs/ImplementersGuide.md) says packaged components ship "with dependencies and restrictions" for now **[doc]**.
- `@intity/epub-js` lists `jsdom` and `marked` as runtime dependencies **[repo]**, which is unusual for a browser renderer.

### Inferences

- Integrating foliate-js into this project means a submodule (preferably `readest/foliate-js` or a project fork of it), hand-written `.d.ts` declarations, and a Vite workaround such as aliasing `pdf.js` to a stub. Since every library book is an EPUB, dropping PDF costs nothing.
- For an app that wants the lowest packaging friction with typed APIs, Readium and likecoin are the cleanest options. Readium's fast release pace means exact version pins, following Stump's practice.
- A React wrapper adds little. All these engines are imperative DOM renderers, so a thin `useEffect`-based component around an engine is enough, and react-reader's main value is demo speed.

### Gaps

- Whether `readest/foliate-js` passes a Vite build was not tested; Readest builds through Next.
- Runtime behaviour under React 19 StrictMode (double-mounted effects) was not tested for any engine.
- The React-only baseline should not be subtracted exactly from the wrapper figure, because tree-shaking differs per entry.

## Rendering quality: pagination, CSS, RTL and vertical writing, FXL, media overlays, footnotes, fonts

### Takeaway

foliate-js has the broadest EPUB rendering coverage in a client-only package: paginated vertical writing, FXL, media overlays, footnote extraction and font deobfuscation. Readium has the most developed publisher-CSS handling and typography (ReadiumCSS) and strong FXL, but vertical writing is forced into scroll mode and it relies on an external parser. epub.js and its forks lack font deobfuscation, media overlays and footnote handling, and carry many old FXL and RTL bugs.

### Cited Findings

Rendering matrix, from source unless noted **[src]**:

| Capability | foliate-js | epub.js / likecoin | Readium ts-toolkit | prose-reader |
| --- | --- | --- | --- | --- |
| Paginated | Yes, CSS multi-column on the document in [`paginator.js`](https://github.com/johnfactotum/foliate-js/blob/main/paginator.js) | Yes, [`contents.js columns()`](https://github.com/futurepress/epub.js/blob/master/src/contents.js) | Yes, through ReadiumCSS columns | Yes, `column-width` in [`layoutEnhancer.ts`](https://github.com/mbret/prose-reader/blob/develop/packages/core/src/enhancers/layout/layoutEnhancer.ts) |
| Scrolled | Yes (`scrolled()` path) | Yes (continuous manager) | Yes (`scroll` preference) | Not confirmed in this pass |
| Spreads / column count | `max-column-count` attribute | Yes | `columnCount` preference | Yes (`spreadMode` tests exist) |
| Publisher CSS | Kept; app adds styles; [`transformTarget`](https://github.com/johnfactotum/foliate-js#readme) hook can rewrite content | Kept; `themes.js` overrides | ReadiumCSS `before`/`after`/`default` layers plus CJK and RTL variants | Enhancers inject overrides |
| RTL | Yes, detected per document | Yes, with open bugs | Yes (`ReadingProgression.rtl`) | Not confirmed |
| Vertical writing | **Paginated** (`vertical` handled throughout paginator) | Partial; `column-axis` path | **Forced to scroll**, [`EpubNavigator.ts`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/EpubNavigator.ts) | Renderer has writing-mode code |
| FXL | Yes, separate [`fixed-layout.js`](https://github.com/johnfactotum/foliate-js/blob/main/fixed-layout.js); no mixed-layout books | Yes (`pre-paginated`), weak | Yes, dedicated [`fxl/`](https://github.com/readium/ts-toolkit/tree/develop/navigator/src/epub/fxl) frame pool and spreader | Yes |
| Media overlays | Yes, `MediaOverlay` class and `view.startMediaOverlay()` | No; only reads the manifest attribute | No SMIL parser found in `navigator` or `guided-navigation`; read-along works from Guided Navigation data | No SMIL code in package sources |
| Footnotes | [`footnotes.js`](https://github.com/johnfactotum/foliate-js/blob/main/footnotes.js) detects `noteref`/`doc-noteref` and extracts content; app draws the popup | None | No `noteref`/`footnote` code in `navigator/src` or injectables | No matches in package sources |
| Obfuscated fonts | Yes: IDPF and Adobe algorithms in [`epub.js`](https://github.com/johnfactotum/foliate-js/blob/main/epub.js) | **No** (no `obfuscat` or `encryption.xml` match in either) | Model only ([`Encryption.ts`](https://github.com/readium/ts-toolkit/tree/develop/shared/src/publication/encryption)); the parser or server must deobfuscate | No matches in package sources |

- foliate-js [README](https://github.com/johnfactotum/foliate-js#readme) says it has two renderers and "there's no support for mixed layout books" **[doc]**.
- foliate-js font deobfuscation needs SHA-1. The [README](https://github.com/johnfactotum/foliate-js#demo) says the default uses Web Crypto, available only in secure contexts, and the [`EPUB` constructor](https://github.com/johnfactotum/foliate-js/blob/main/epub.js) accepts a custom `sha1` **[doc] [src]**.
- The Readest fork grew `paginator.js` from 1,130 to 4,447 lines and `fixed-layout.js` from 319 to 2,242 **[src]**. Recent commits include rendering huge sections in chunks (2026-10) and [footnote popup context fixes](https://github.com/readest/foliate-js/commits/main) (2026-10-26) **[repo]**.
- A Readium maintainer wrote "vertical-rl support in the ts-toolkit is very minimal", and reported it "completely broken" on iOS 27 — [ts-toolkit #237](https://github.com/readium/ts-toolkit/issues/237), still open **[doc]**.
- epub.js has long-open FXL and RTL issues: [#1069](https://github.com/futurepress/epub.js/issues/1069) and [#1070](https://github.com/futurepress/epub.js/issues/1070) on RTL FXL spreads, [#1095](https://github.com/futurepress/epub.js/issues/1095) on cut-off fixed layouts, [#980](https://github.com/futurepress/epub.js/issues/980) on `currentLocation` for pre-paginated books, and [#1271](https://github.com/futurepress/epub.js/issues/1271) and [#1277](https://github.com/futurepress/epub.js/issues/1277) on RTL vertical offset **[repo]**.
- foliate-js ruby handling has an open report: line breaks inside `<ruby>` appear as spaces and search context breaks — [#124](https://github.com/johnfactotum/foliate-js/issues/124) **[doc]**.
- Thorium Reader's stack has first-class [`media-overlays.ts`](https://github.com/edrlab/r2-navigator-js/tree/develop/src/electron/renderer), but under `src/electron` **[src]**.
- Vivliostyle's [`epub.ts`](https://github.com/vivliostyle/vivliostyle.js/blob/master/packages/core/src/vivliostyle/epub.ts) includes font deobfuscation and `pre-paginated` handling **[src]**.

### Inferences

- For a calibre-style library of arbitrary user EPUBs, obfuscated-font support matters because retail EPUBs often embed obfuscated fonts. Missing it in epub.js and its forks would show as silent fallback fonts.
- All the mainstream engines paginate with CSS multi-column in an iframe, so rendering quality on macOS is ultimately bounded by WebKit's column implementation. Differences come from workarounds and not from layout technology. Vivliostyle is the exception as a dedicated typesetter (based on prior knowledge of the project, not checked in source this session), but it is heavy and AGPL.
- Mixed-layout books (reflowable plus FXL in one spine) remain a foliate-js limitation.

### Gaps

- No side-by-side rendering test against a shared EPUB corpus was run. The matrix shows code presence and not visual quality.
- prose-reader's RTL and scrolled-mode support were not confirmed by the checks run here, so "not confirmed" does not mean absent.
- Whether Stump's Rust generator deobfuscates fonts was not checked.
- I did not inspect Bibi, R2D2BC or the Treinetic reader beyond repository metadata.

## Feature coverage: TOC, CFI, search, annotations, progress, theming, typography

### Takeaway

foliate-js and epub.js are CFI-native and do search client-side. Readium has no CFI generation or client-side search, and uses Locators and a positions list instead, but offers the richest preferences API. For a Rust backend that may want to store CFIs (as calibre and Kobo-adjacent tools do), this difference is an architectural decision.

### Cited Findings

Feature matrix **[src]** unless noted:

| Feature | foliate-js | epub.js / likecoin | Readium ts-toolkit | prose-reader |
| --- | --- | --- | --- | --- |
| TOC | Yes (`book.toc`, per [README](https://github.com/johnfactotum/foliate-js#readme)) | Yes (`navigation.js`) | From the manifest | From the manifest |
| CFI | [`epubcfi.js`](https://github.com/johnfactotum/foliate-js/blob/main/epubcfi.js) parse, generate, resolve | [`epubcfi.js`](https://github.com/futurepress/epub.js/blob/master/src/epubcfi.js), 1,048 lines | **Only a [`partialCfi`](https://github.com/readium/ts-toolkit/blob/develop/shared/src/publication/html/Locations.ts) getter** on Locators | Standalone [`@prose-reader/cfi`](https://www.npmjs.com/package/@prose-reader/cfi) |
| In-book search | [`search.js`](https://github.com/johnfactotum/foliate-js/blob/main/search.js), `Intl.Collator` and `Intl.Segmenter` based | `section.find()` and `search()` | No search module found; Stump does it server-side in [`search.rs`](https://github.com/stumpapp/stump/blob/main/core/src/readium/search.rs) | `enhancer-search` |
| Highlights | [`overlayer.js`](https://github.com/johnfactotum/foliate-js/blob/main/overlayer.js) SVG overlay, hit testing by client rects | `marks-pane` SVG annotations | [`@readium/decorator`](https://github.com/readium/ts-toolkit/tree/develop/decorator), CSS Highlight API | `enhancer-annotations` |
| Progress | [`progress.js`](https://github.com/johnfactotum/foliate-js/blob/main/progress.js), byte-size based, no pre-pass needed | `locations.generate()` pre-pass | Positions list via `pub.positionsFromManifest()` | Own pagination enhancer |
| Theming and typography | Bring your own CSS through `setStyles`; no preference model | `themes.js`, `fontSize` | Full [`EpubPreferences`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/preferences/EpubPreferences.ts): font, line height, hyphens, ligatures, alignment, columns and more | Settings and enhancers |
| TTS | [`tts.js`](https://github.com/johnfactotum/foliate-js/blob/main/tts.js) emits SSML; app supplies speech | None | [`ReadAloudNavigator`](https://github.com/readium/ts-toolkit/blob/develop/navigator/docs/ReadAloud.md) with `@readium/speech` | Not confirmed |

- likecoin's [benchmark](https://github.com/likecoin/epub.ts) claims `locations.generate(1000)` on War and Peace took 42.9 s on epubjs 0.3.93 and 159 ms on epub-ts, in headless Chrome on an M4 Pro **[doc]**. This is a vendor benchmark that I did not reproduce, and it was not run on WebKit.
- Readium `EpubNavigator` takes a `positions` array or loads one from the manifest in [`load()`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/EpubNavigator.ts) **[src]**, and Thorium Web requires a server providing the [manifest and Positions List](https://github.com/edrlab/thorium-web/blob/develop/docs/ImplementersGuide.md) **[doc]**.
- ts-toolkit contains **zero** references to `.opf` or `container.xml` across `shared/src` and `navigator/src` **[src]**, so it does not parse EPUB packages. Thorium Web parses with the [Readium Go-Toolkit](https://github.com/readium/go-toolkit), which [produces the manifest and positions](https://github.com/edrlab/thorium-web/blob/develop/docs/ImplementersGuide.md) **[doc]**.
- Stump has an MIT-licensed Rust [`ReadiumManifestGenerator`](https://github.com/stumpapp/stump/blob/main/core/src/readium/generator.rs) built on the `epub` crate, with position models, and a [search module](https://github.com/stumpapp/stump/tree/main/core/src/readium) **[src]**. Stump's [licence is MIT](https://github.com/stumpapp/stump) **[repo]**.
- WebKit caveat for Readium highlights: decorations needed a forced reflow, per [#258](https://github.com/readium/ts-toolkit/issues/258) and [#259](https://github.com/readium/ts-toolkit/issues/259), both closed in September 2026 **[repo]**.
- foliate-js has an open idea to adopt the CSS Custom Highlight API, [#21](https://github.com/johnfactotum/foliate-js/issues/21) **[repo]**.

### Inferences

- With Readium, the Rust backend must own manifest generation, positions, search and font deobfuscation. That fits this project's Rust backend, and Stump shows a route, but it is real extra scope compared with foliate-js, where all of this is client-side.
- With foliate-js, the app must own the settings model and UI, popup rendering, and theming. Readest shows that this is achievable but substantial.
- Byte-size progress in foliate-js is available immediately on open, which suits a large library. Its numbers will not match Readium positions or epub.js locations, so cross-engine position migration would be lossy unless anchored to CFIs.

### Gaps

- No engine's CFI output was tested for interoperability with calibre or Kobo CFIs.
- Whether Readium's `partialCfi` is ever populated outside a streamer was not traced.

## Known bugs and limitations on WebKit, Safari and WKWebView

### Takeaway

The biggest WebKit constraint affects every engine: [WebKit bug 218086](https://bugs.webkit.org/show_bug.cgi?id=218086) means iframes need `allow-scripts` for parent-attached events to fire, which defeats the iframe sandbox for same-origin content. Beyond that, Readest's fork and ts-toolkit are actively fixing WebKit bugs in 2026, while epub.js has years of open Safari and iOS reports. Foliate's own target includes WebKitGTK, which is related to but not the same as WKWebView.

### Cited Findings

Shared:

- Bug 218086 has status NEW, was filed 2020-10-22, and was last modified 2025-02-09: parent-attached listeners inside `sandbox="allow-same-origin"` frames are blocked in Safari — [WebKit Bugzilla](https://bugs.webkit.org/show_bug.cgi?id=218086) **[doc]**. The page was read on 2026-11-07.
- A November 2026 ts-toolkit PR removed a `frameSandbox` option because "WebKit dispatches no event listener in a frame without `allow-scripts`... so the navigator stops working in Safari" — [#286](https://github.com/readium/ts-toolkit/issues/286) **[doc]**. So the problem is current.
- foliate-js, Readium and prose-reader all set `allow-same-origin allow-scripts`: [`paginator.js`](https://github.com/johnfactotum/foliate-js/blob/main/paginator.js), [`FrameManager.ts`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/frame/FrameManager.ts), [`createFrameElement.ts`](https://github.com/mbret/prose-reader/blob/develop/packages/core/src/enhancers/html/renderer/createFrameElement.ts) **[src]**.

foliate-js and Readest:

- foliate-js [targets](https://github.com/johnfactotum/foliate-js#readme) "the latest version of WebKitGTK, Firefox, and Chromium" **[doc]**, which does not name Safari.
- It carries WebKit workarounds: `-webkit-line-box-contain` to fix glyph clipping, and `requestAnimationFrame` needed in WebKit, in [`paginator.js`](https://github.com/johnfactotum/foliate-js/blob/main/paginator.js) **[src]**.
- Open: [#165](https://github.com/johnfactotum/foliate-js/issues/165) WebKitGTK chapter-frame zoom (tied to [WebKit bug 313899](https://bugs.webkit.org/show_bug.cgi?id=313899)), [#166](https://github.com/johnfactotum/foliate-js/issues/166) callbacks reaching unready documents including Safari `ResizeObserver` behaviour, [#97](https://github.com/johnfactotum/foliate-js/issues/97) a null error from `ResizeObserver` on Firefox and Safari, [#86](https://github.com/johnfactotum/foliate-js/issues/86) iOS text selection treated as a swipe **[repo]**.
- Closed: [#44](https://github.com/johnfactotum/foliate-js/issues/44) links not followed after page flips on Safari 17 **[repo]**.
- WebKit fixes that exist only in the [Readest fork](https://github.com/readest/foliate-js/commits/main) as of this snapshot include re-applying scroll offsets when WebKit clamps them (2026-08-13), avoiding `document.fonts.ready` before Safari 17 (2026-08-12), and Safari zoom handling (2025-12-14) **[repo]**. "Only in the fork" is inferred from the 273-commit lead; I did not diff each one.
- Readest on Mac: [#5582](https://github.com/readest/readest/issues/5582) wide word gaps in justified Standard Ebooks text, [#5749](https://github.com/readest/readest/issues/5749) weak hyphenation where the maintainer said they keep the system engine's behaviour, and [#5601](https://github.com/readest/readest/issues/5601) bulk import exhausting WebKit resources **[repo]**. [#903](https://github.com/readest/readest/issues/903) (2025) saw text vanish after an update until a user cleared WebKit caches **[doc]**.

Readium ts-toolkit:

- [#147](https://github.com/readium/ts-toolkit/issues/147) tracks WebKit-specific bugs: iOS text-size adjustment after rotation, iPadOS zoom issues ([WebKit 293617](https://bugs.webkit.org/show_bug.cgi?id=293617)), and scroll mode not scrollable until a forced recalc **[doc]**.
- The source sends `force_webkit_recalc` when it detects WebKit ([`FrameManager.ts`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/frame/FrameManager.ts)) and has Safari focus handling in [`FramePoolManager.ts`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/frame/FramePoolManager.ts) **[src]**.
- Open: [#175](https://github.com/readium/ts-toolkit/issues/175) inconsistent text selection on Safari and Firefox, [#237](https://github.com/readium/ts-toolkit/issues/237) vertical text on iOS **[repo]**.

epub.js and forks:

- epub.js defaults to `sandbox="allow-same-origin"` with `allow-scripts` only when `allowScriptedContent` is set ([`iframe.js`](https://github.com/futurepress/epub.js/blob/master/src/managers/views/iframe.js)) **[src]**.
- Open reports: [#1229](https://github.com/futurepress/epub.js/issues/1229) click and touch events not working on iPhone and Mac, [#1212](https://github.com/futurepress/epub.js/issues/1212) Safari scroll jumps, [#1303](https://github.com/futurepress/epub.js/issues/1303) scroll bounce, [#1218](https://github.com/futurepress/epub.js/issues/1218), [#1343](https://github.com/futurepress/epub.js/issues/1343) and [#1366](https://github.com/futurepress/epub.js/issues/1366) column shifts, [#1163](https://github.com/futurepress/epub.js/issues/1163) iOS WKWebView load failure, [#1411](https://github.com/futurepress/epub.js/issues/1411) RTL book not loading on Mac **[repo]**. A search for "tauri" in its tracker returned 0 results **[repo]**.
- The source has a comment that a Safari column cut-off fix "causes RTL issues" ([`contents.js`](https://github.com/futurepress/epub.js/blob/master/src/contents.js)) **[src]**.
- likecoin's [CHANGELOG](https://github.com/likecoin/epub.ts/blob/master/CHANGELOG.md) records iOS WebKit fixes, including removing `-webkit-line-box-contain` and an `inline-block` column-container fix **[doc]**. Note that foliate-js keeps that property while likecoin removed it for line-height issues.

prose-reader:

- Its Playwright config includes [Desktop Safari and iPhone 12 projects](https://github.com/mbret/prose-reader/blob/develop/apps/tests/playwright.config.ts), and core has a [`webkit.ts`](https://github.com/mbret/prose-reader/blob/develop/packages/core/src/enhancers/webkit.ts) enhancer for flicker **[src]**. I did not verify CI runs them.

Tauri/wry:

- [wry #1775](https://github.com/tauri-apps/wry/issues/1775) (open): custom-protocol responses sent off the main thread block the calling thread on iOS and macOS **[doc]**.

### Inferences

- epub.js's default sandbox matches the failure condition of bug 218086, which plausibly explains #1229. If so, enabling `allowScriptedContent` is the likely fix, and that removes its main script protection. Any WKWebView use of epub.js, likecoin or react-reader needs this checked first.
- Upstream foliate-js does not name Safari as a target, so on macOS the Readest fork is the better base, because it has real macOS and iOS users filing bugs.
- Justification gaps and hyphenation weaknesses appear to be engine-level WebKit traits and not engine-library bugs, so changing library would likely not fix them.

### Gaps

- No Tauri-specific rendering bugs were found for Readium or prose-reader, likely because few Tauri apps use them. Absence of reports here is not evidence of it working.
- The behaviour of macOS WKWebView specifically (as opposed to Safari or iOS) is under-reported in most trackers.
- WebKit bug 313899 was not opened directly; it is cited through foliate-js #165.

## Content loading, strict CSP and custom URI schemes

### Takeaway

Every serious engine renders through `blob:` or `srcdoc` iframes that run same-origin with `allow-scripts`, so a parent-page CSP is the real security boundary, not the sandbox. foliate-js works with a strict `script-src 'self'` and fits a Rust-backed loader through its book interface. Readium needs real URLs for a manifest and resources, which a custom URI scheme could provide, and its frames allow inline script. Service-worker designs do not work on `tauri://` in WKWebView, which rules out prose-reader's recommended path unless a custom scheme replaces it.

### Cited Findings

Loading models:

| Engine | Book access | Section delivery | Sub-resources | Sandbox |
| --- | --- | --- | --- | --- |
| foliate-js | `File`/`Blob` through vendored zip.js, or any object with `loadText`, `loadBlob`, `getSize`, `sha1` ([`EPUB` constructor](https://github.com/johnfactotum/foliate-js/blob/main/epub.js)) | `iframe.src = blob:` | Rewritten to `blob:` URLs | `allow-same-origin allow-scripts` |
| Readest fork | As above, plus a Rust [`range_file.rs`](https://github.com/readest/readest/blob/main/apps/readest-app/src-tauri/src/range_file.rs) and `rangefile.localhost` host in its CSP | Also has `iframe.srcdoc` paths (conditions not traced) | `blob:` | Same |
| epub.js / likecoin | JSZip archive in memory, or a URL | `srcdoc` default; `blobUrl` or `write` options ([`iframe.js`](https://github.com/futurepress/epub.js/blob/master/src/managers/views/iframe.js)) | `blobUrl` for archives ([`book.js`](https://github.com/futurepress/epub.js/blob/master/src/book.js)) | `allow-same-origin`; scripts optional |
| Readium | Publication from a manifest and [`HttpFetcher`](https://github.com/readium/ts-toolkit/blob/develop/shared/src/fetcher/HttpFetcher.ts), whose fetch client is injectable | `blob:` via [`FrameBlobBuilder`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/frame/FrameBlobBuilder.ts), with an injected `<meta>` CSP | `<base href>` to the publication URL, so the browser loads them directly | `allow-same-origin allow-scripts` |
| prose-reader | Manifest plus resources from a [service worker, page or server](https://github.com/mbret/prose-reader) | `blob:` ([`attachFrameSrc.ts`](https://github.com/mbret/prose-reader/blob/develop/packages/core/src/enhancers/html/renderer/attachFrameSrc.ts)) | Rewritten to `blob:` in [`assets.ts`](https://github.com/mbret/prose-reader/blob/develop/packages/core/src/enhancers/html/renderer/assets.ts) | `allow-same-origin allow-scripts allow-top-navigation-to-custom-protocols` |
| r2-navigator-js | HTTP streamer server (`r2-streamer-js`, Express) | Electron code in [`src/electron`](https://github.com/edrlab/r2-navigator-js/tree/develop/src/electron) | HTTP | n/a |

Security evidence:

- foliate-js [README](https://github.com/johnfactotum/foliate-js#security): sandboxing scripted content is "currently impossible" to do securely due to same-origin `blob:` URLs and the WebKit bug, so "It is therefore imperative that you use Content Security Policy (CSP) to block all scripts except `'self'`". A test EPUB exists at [johnfactotum/epub-test](https://github.com/johnfactotum/epub-test) **[doc]**.
- foliate-js needs `style-src 'unsafe-inline'`, and the author argues inline CSS is not an XSS vector inside the iframe — [#64](https://github.com/johnfactotum/foliate-js/issues/64) **[doc]**. The same thread describes injecting a `<meta>` CSP through the `data` transform as extra defence, as suggested in the WebKit bug **[doc]**.
- [ts-toolkit #120](https://github.com/readium/ts-toolkit/issues/120) (open) says EPUB content in a toolkit iframe can easily "escape and execute malicious JS on the parent frame" **[doc]**.
- Readium's per-frame CSP now exists, but it allows `script-src ${d} blob: 'unsafe-inline'`, sets `connect-src 'none'` and `form-action 'none'`, and offers `upgradeInsecureRequests: 'always' | 'never' | 'auto'` — [`FrameBlobBuilder.ts`](https://github.com/readium/ts-toolkit/blob/develop/navigator/src/epub/frame/FrameBlobBuilder.ts) **[src]**. That option came from a [Stump report](https://github.com/readium/ts-toolkit/issues/252) **[doc]**.
- [ts-toolkit #286](https://github.com/readium/ts-toolkit/issues/286) adds a fallback for when injected scripts cannot run. Its author reports that chapters served with `script-src 'none'` worked in their app, while noting they did not re-run the navigator on that commit **[doc]**.
- likecoin's docs state that `allow-scripts` with `allow-same-origin` "removes sandbox protection — the book gains your origin's full authority" ([`rendition.ts`](https://github.com/likecoin/epub.ts/blob/master/src/rendition.ts)) **[src]**.

Readest as a worked Tauri example:

- Its [`tauri.conf.json`](https://github.com/readest/readest/blob/main/apps/readest-app/src-tauri/tauri.conf.json) sets `script-src 'self' 'unsafe-inline' 'unsafe-eval' data: blob: asset: ...`, `style-src 'self' 'unsafe-inline' blob: ...`, `font-src ... blob: data:`, and `frame-src 'self' blob: asset: ...`, with the asset protocol enabled **[src]**. This is much looser than foliate-js's README asks for. It does not set `dangerousDisableAssetCspModification` **[src]**.
- Readest instead relies on sanitising through the content transform: [PR #6819](https://github.com/readest/readest/pull/6819) (merged 2026-10-18) fixed sections escaping the sanitiser because of case-sensitive media-type checks **[doc]**. [Issue #2316](https://github.com/readest/readest/issues/2316) (opened October 2025, now closed) was a stored XSS through an uploaded EPUB **[doc]**.
- Its published [security advisories](https://github.com/readest/readest/security/advisories) include a critical PDF.js script execution item **[repo]**. foliate-js, epub.js and ts-toolkit list none **[repo]**.
- Readest needed `blob:` added to CSP for embedded fonts — [#320](https://github.com/readest/readest/issues/320) **[doc]**.

Tauri constraints:

- Tauri "appends its nonces and hashes to the relevant CSP attributes" at compile time. Local scripts are hashed; styles and external scripts get a nonce — [Tauri CSP docs](https://v2.tauri.app/security/csp/) **[doc]**.
- `dangerousDisableAssetCspModification` disables that modification — [config reference](https://v2.tauri.app/reference/config/) **[doc]**.
- Service workers cannot be registered on `tauri://` in WKWebView. A Tauri maintainer closed the request saying "There's nothing we can do here", pointing to the localhost plugin until a CEF-based alternative — [tauri #13031](https://github.com/tauri-apps/tauri/issues/13031) **[doc]**.
- A 2021 Tauri change was made to ensure [custom protocols are handled as a secure context on macOS](https://github.com/tauri-apps/tauri/commit/5909c1e01437e10c45694c24f9037f4b176a03ec) **[repo]**.
- The Tauri isolation pattern has a Windows and Android bug with `crypto.subtle` in its iframe — [tauri #16219](https://github.com/tauri-apps/tauri/issues/16219) **[doc]**. That report does not cover macOS.
- prose-reader's docs call their non-service-worker guide "old and obsolete" and "suited to prototypes" — [web-dom.md](https://github.com/mbret/prose-reader/blob/develop/apps/docs/src/content/docs/learn/streamer/web-dom.md) **[doc]**. Its README says you decide how resources are served, including [from a server](https://github.com/mbret/prose-reader) **[doc]**.

### Inferences

- Because an iframe with `allow-same-origin allow-scripts` shares the app origin, any script that runs in a book could reach `window.parent` and Tauri's IPC bridge. For a library app with file access, this is the top threat. Not verified by a proof of concept here.
- Recommended stance for foliate-js in Tauri: a parent CSP of `script-src 'self'` with no `blob:` or `'unsafe-inline'`, plus `frame-src blob:`, `img-src`/`font-src`/`media-src` with `blob: data:`, and `style-src 'self' 'unsafe-inline'`. Blob and srcdoc documents should inherit the creator's CSP, which is why foliate-js's advice works, but this needs testing in WKWebView. Add transform-time `<meta>` CSP as a second layer, and keep Tauri capabilities tight. Do not copy Readest's `script-src`.
- Potential Tauri trap: under CSP rules, a nonce or hash in a directive makes `'unsafe-inline'` ignored by browsers. If Tauri adds a nonce to `style-src`, foliate's inline styles could break unless that directive is excluded with `dangerousDisableAssetCspModification`. Readest works without that flag, so the practical result may depend on whether the built HTML has inline style tags. This must be tested.
- Readium plus a custom URI scheme looks feasible: a Rust `register_uri_scheme_protocol` handler could serve the manifest and resources from the EPUB. But `<base href>` means the scheme must answer direct browser loads with correct MIME and CORS headers, so an injected fetch client alone will not do. Each sub-resource passes through the custom protocol, which makes wry #1775 worth a look. The parent CSP would also need to admit Readium's blob-injected scripts or rely on the #286 approach.
- foliate-js is the most natural fit for a Rust backend, since the `EPUB` constructor takes `loadText`, `loadBlob` and `getSize`, so entries could come from Tauri commands or a custom scheme with no service worker or HTTP server.
- `crypto.subtle` should be available for font deobfuscation given the 2021 secure-context change, and the `sha1` constructor option is a fallback either way.
- Avoid `tauri-plugin-localhost` as a way to get service workers: it trades the custom scheme for a local HTTP server reachable by other processes (a general concern, not researched further here).

Overall ranking for this project (inference, based on all sections):

1. **`readest/foliate-js` fork (or a project-owned fork of it) as a submodule.** Best coverage, proven on Tauri 2 + React 19 + macOS, small bundle, MIT, strict CSP possible. Costs: untyped, unstable API, single-maintainer dependency, Vite patch, and building all reader UI and settings. Pin a commit and track upstream and the fork.
2. **Readium ts-toolkit with a Rust manifest and positions generator.** Best-engineered npm package, with typed APIs, semver and strong preferences. Costs: no parser, no CFI, no client search, no footnote handling, weak vertical writing, a looser frame CSP, and no known Tauri deployment. Good choice if standards alignment and accessibility outweigh that scope.
3. **`@likecoin/epub-ts`** as a typed, maintained epub.js path only if simple books suffice. The missing font deobfuscation, media overlays and footnotes are serious for a general library.
4. **prose-reader**: worth watching and has WebKit test projects, but its service-worker preference, rapid breaking releases and very small adoption make it a higher risk.
5. Not recommended: epub.js upstream, react-reader, Flow's fork, `@btpf/epubjs`, r2-navigator-js (Electron-bound), Vivliostyle (AGPL and heavy, built for typesetting), and the unofficial `foliate-js` npm package.

### Gaps

- No engine has been tested with this project's real CSP, custom scheme or WKWebView. A spike running [epub-test](https://github.com/johnfactotum/epub-test) and a font-obfuscated EPUB in a minimal Tauri app would settle most open points here.
- Tauri's nonce handling of `style-src` for a Vite-built `index.html` was not tested.
- Whether Tauri's IPC is reachable from a `blob:` subframe was not tested.
- Whether Tauri's isolation pattern works on macOS with these engines was not tested.
- WebKit's handling of `blob:` documents with `<base>` pointing to a custom scheme, as Readium needs, was not tested, and neither were CORS needs for fonts in that setup.
