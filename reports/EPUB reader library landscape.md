# Adopt Readest's foliate-js fork, write the rest

> **Update:** the spike has since run in a real Tauri build and supports this design, with corrections. See [the spike results](../docs/reader-spike.md).

Use the MIT-licensed `readest/foliate-js` fork as the rendering engine, pinned as a project-owned fork, and let it parse the EPUB in the webview. Run it in a dedicated `reader-*` window with blob frames, a strict CSP and a Tauri app manifest that denies commands by default. Rust serves zip entries through a narrow command and takes on background indexing later; `rbook` and a `book://` document origin are both unnecessary for v1. No native or Rust rendering engine is credible, and every other JavaScript engine either lacks coverage (epub.js and its forks) or conflicts with the threat model (Readium needs blob scripts to run). What remains for this project is a reader layer of roughly 5k to 8k lines, a locator model that survives kepubify, and the security configuration. The security work comes first because all 61 app commands, including Kobo writes, currently skip the ACL. **Nothing in the research ran in a real Tauri build**: the security evidence comes from a raw WKWebView harness and source reading, so a short spike must confirm it before the design is committed.

## Only the foliate-js line fits this stack today

Four JavaScript engine lines are actively maintained, but only one combines coverage, a security model compatible with a no-scripts reader, and production proof on this stack. Readest ships foliate-js on Tauri 2, React 19 and macOS ([Readest](https://github.com/readest/readest/blob/main/.gitmodules)), and its fork is 273 commits ahead of upstream with +9,338/-840 lines ([compare](https://github.com/johnfactotum/foliate-js/compare/main...readest:foliate-js:b34a0aa)). Upstream made 15 commits in 12 months, does not name Safari as a target, and declined Readest's compatibility patches ([foliate-js #43](https://github.com/johnfactotum/foliate-js/issues/43)). So the fork, not upstream, carries the macOS WebKit fixes, and it depends almost entirely on one maintainer. Plan to own a pinned fork from the start: every serious web reader studied owns its engine or carries a heavy fork ([case studies](../research_notes/EPUB%20reader%20library%20landscape/production_case_studies.md)).

| Engine | Licence | Health | Coverage for a general library | Fit with a no-scripts CSP | Verdict |
| --- | --- | --- | --- | --- | --- |
| [readest/foliate-js](https://github.com/readest/foliate-js) | MIT | 274 commits in 12 months, one main author | Paginated vertical text, FXL, media overlays, footnotes, font deobfuscation, CFI, search; no mixed-layout books | Good: needs no script in the frame | **Use** |
| [foliate-js upstream](https://github.com/johnfactotum/foliate-js) | MIT | 15 commits in 12 months, "not stable", no release | As above, fewer WebKit fixes | Good | Track for fixes |
| [Readium ts-toolkit](https://github.com/readium/ts-toolkit) | BSD-3 | 94 commits, 83 releases in 12 months | Best typography preferences; no parser, CFI generation or client search; vertical text forced to scroll | Poor: injected blob scripts must run | Reject here |
| [@likecoin/epub-ts](https://github.com/likecoin/epub.ts) | BSD-2 | 222 commits, pre-1.0 | No font deobfuscation, media overlays or footnotes | Likely the same event problem as epub.js; unchecked | Reject |
| [prose-reader](https://github.com/mbret/prose-reader) | MIT | 175 releases in 12 months, major version 40 | Good base, thin on footnotes and fonts | Prefers service workers, which `tauri://` cannot register ([tauri #13031](https://github.com/tauri-apps/tauri/issues/13031)) | Watch |
| [epub.js](https://github.com/futurepress/epub.js), [react-reader](https://github.com/gerhardsletten/react-reader) | BSD-2; licence metadata conflicts | npm release from 2022; open Safari reports | Weak FXL and RTL, no deobfuscation | Default sandbox hits the WebKit event bug | Reject |
| [Vivliostyle](https://github.com/vivliostyle/vivliostyle.js) | AGPL-3.0 | Active | Typesetter, 239 KB gzip | Not assessed | Reject |

Licensing is clear of the main path. The workspace is GPL-3.0-only, and MIT, BSD and Apache-2.0 code combines into it. Readest's application code is AGPL-3.0, so its React glue, `range_file.rs` and hooks are for reading only; copying them would carry AGPL terms into the combined work ([GPLv3](https://www.gnu.org/licenses/gpl-3.0.txt)). That is a licence reading, not legal advice.

Native rendering is closed for now. Blitz has no multi-column or fragmentation support ([Blitz CSS status](https://blitz.is/status/css)), Servo ships columns and writing-mode off by default ([prefs.rs](https://github.com/servo/servo/blob/main/components/config/prefs.rs)), MuPDF is an AGPL page rasteriser ([Artifex](https://artifex.com/licensing)), and crengine has no Rust bindings and an unclear licence version ([crengine](https://github.com/koreader/crengine/blob/master/README.TXT)). WKWebView already provides selection, ruby, MathML and SVG.

The engine is also the small part. Foliate carries about 8.5k lines over a 7.1k-line engine, Calibre about 30k in total, and Readest 61k in its reader module alone, much of it sync and TTS ([case studies](../research_notes/EPUB%20reader%20library%20landscape/production_case_studies.md)). Effort concentrates in selection and annotation, publisher-CSS overrides, and input feel. User complaints cluster on lost positions and trackpad paging rather than on engines.

## The three known conflicts all resolve toward the simpler design

### Conflict 1: blob frames in a dedicated window beat a `book://` document origin for v1

The two notes agree on the facts and disagree on the design. `rust_and_native_options.md` prefers a reader host document on `book://<id>/`, with `script-src 'none'` on each chapter response, and falls back to blob frames if the engine objects. `security_and_sandboxing.md` prefers blob frames in a dedicated window with an app manifest, and defers custom-scheme origins to a possible scripted mode. **Blob frames win**, for five reasons.

First, the per-chapter header never reaches foliate-js's frames. foliate-js parses each section with `DOMParser`, rewrites its resources, reserialises it and loads the result as a blob ([epub.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/epub.js#L815-L821), [L862](https://github.com/johnfactotum/foliate-js/blob/78914ae/epub.js#L862)). A blob document inherits its creator's CSP ([WebKit layout test](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/LayoutTests/http/tests/security/contentSecurityPolicy/blob-url-inherits-multiple-csp-policies.html)), and response headers on the fetched bytes are gone. The rust note's main benefit therefore needs a rewritten section loader, and that is my inference from source, not something either researcher tested. Second, a direct `book://` frame from the app page is unreadable by the engine in both probes ([security notes](../research_notes/EPUB%20reader%20library%20landscape/security_and_sandboxing.md)). Third, Tauri treats registered custom schemes as local, so `book://` gains no remote-origin protection from the ACL ([webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L1961-L2002)). Fourth, custom-scheme frames fail open: they do not inherit the parent's CSP, and in the harness one under a strict parent ran inline script and reached `ipc://` (scenario S4). Fifth, this path carries open macOS defects, including a crash on aborted requests ([tauri #9177](https://github.com/tauri-apps/tauri/issues/9177)) and a main-thread wait whose fix is unmerged ([wry #1775](https://github.com/tauri-apps/wry/issues/1775), [wry #1856](https://github.com/tauri-apps/wry/pull/1856)). The spec's unique-origin rule binds only readers that support spine-level scripts ([RS 3.3](https://www.w3.org/TR/epub-rs-33/)), so a no-scripts reader can defer it.

The rust note's strongest point stands: under blob frames, a CSP mistake puts script in the same origin as `reader.html`, which holds `__TAURI_INTERNALS__`, whereas a `book://` host holds none. This is the path in Readest's advisory, whose proof of concept was tested on macOS ([CVE-2026-82642](https://github.com/readest/readest/security/advisories/GHSA-p4x7-pf2c-xrvj)). The dedicated window alone does not answer it, because without an app manifest a local window with no capabilities still reaches every app command ([webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2076-L2083)). The app manifest answers it: a CSP failure then costs one book's reading position rather than Kobo data. The rust note's nested host remains the fallback if the spike undermines this. Its nearest relative in the security note, option E, is estimated at 2 to 4 days, but E as written rates no better than the blob design once script runs, so only a host document without Tauri internals adds protection. Any later scripted mode needs Calibre-style isolation rather than this reader.

### Conflict 2: foliate-js parses for display, and Rust needs only a thin read path

The `rbook` recommendation answers a question the chosen engine removes. foliate-js already reads the OPF, nav and NCX, deobfuscates fonts, and takes entries through `loadText`, `loadBlob` and `getSize` ([epub.js](https://github.com/johnfactotum/foliate-js/blob/main/epub.js)). It also falls back from invalid XHTML to an HTML parse ([epub.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/epub.js#L815-L821)), which the notes did not record and which makes the proposed Rust well-formedness gate redundant for display. Readest reached the same split from production: its Rust side prefetches a few files and deliberately does not parse OPF metadata, because a second parser would diverge from foliate-js ([epub_parser.rs](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/src/epub_parser.rs)). The rbook pick was also judgement from grep-level checks, never run on real books, and porting boko's deobfuscation becomes unnecessary.

**Rust does need to read, but not to model the book.** It must serve zip entries, refuse DRM through the existing `has_drm`, and later extract text for cross-library search and build Kobo span tables. Those jobs need spine order and hrefs, which promoting erc-epub's private `Parts` and adding `linear` provides at small cost. Add `rbook` only when Rust must read navigation or page lists without a webview, and test it against real books first, since erc-epub's own code warns that nav files often do not parse. In-book search stays in the webview, because mapping a hit to a DOM range is the hard part.

### Conflict 3: bug 218086 and bug 98951 describe one symptom, and shipped macOS still has it

Both numbers are correct from different sides. **[WebKit 218086](https://bugs.webkit.org/show_bug.cgi?id=218086)** is the report foliate-js cites, and it still showed NEW with a last change of 2025-02-09. **[WebKit 98951](https://bugs.webkit.org/show_bug.cgi?id=98951)** is the bug under which the fix landed, as [commit 64114422f68e](https://github.com/WebKit/WebKit/commit/64114422f68e) (325977@main) on 2026-11-06. No explicit link between the bugs was found, and a comment marks 218086 as a duplicate of 221283, so treating 98951 as the fix for 218086 is a reading based on the matching symptom.

**The fix has not shipped.** Two separate raw WKWebView harnesses on macOS 27.0.1 saw a parent click listener fire zero times with `sandbox="allow-same-origin"` and once with `allow-scripts` added; I confirmed the 0 and 1 counts in the security harness logs. Safari 27 shipped on 2026-09-14, before the commit ([Apple](https://support.apple.com/en-al/149039)), and a November 2026 Readium PR reports the same failure ([ts-toolkit #286](https://github.com/readium/ts-toolkit/issues/286)). Which release will carry the fix is unknown. So keep `allow-same-origin allow-scripts`, cite 218086 in code as the symptom, and track 325977@main for removal. Even then, the commit leaves some listener types blocked, and only `click` has been tested. CSP stays the barrier regardless.

### Five further conflicts the notes leave open

| Conflict | Resolution | Reason |
| --- | --- | --- |
| Production note: CFI as primary locator. Locator note: CFI as a cache. | **Store a Readium-style Locator; cache CFI.** Keep raw Kobo values beside it. | kepubify changed every tested CFI path and the spine step, while body text stayed identical, in one synthetic book ([locator notes](../research_notes/EPUB%20reader%20library%20landscape/locators_and_kobo_mapping.md)). CFI still drives foliate-js navigation. |
| Security note: strip `script`, `iframe` and others in a transform. Rust note: keep the DOM unchanged. | **Neutralise in place, never remove nodes.** CSP and ACL are the boundary; the transform is optional. | Removing elements shifts CFI steps. The rust note's objection to HTML-mode sanitisers does not apply to a DOM pass in the webview's own parser, but foliate-js reserialises anyway, so "unchanged bytes" holds at the DOM level only. |
| Production note: copy Readest's asset-protocol reads. Security note: no `asset:` in the reader. | **No `asset:` in the reader.** | A same-origin frame fetched a stand-in `asset://` URL under a permissive CSP in the harness, and the app's scope covers the whole library. |
| JS note ranks Readium second. Security note calls it the worst fit. | **Security note wins.** | Readium needs blob scripts, and allowlisted `blob:` was exploited in EPUBReader ([Franken et al.](https://lirias.kuleuven.be/retrieve/616428)). Readium would also need a Rust manifest and positions generator, and has no known Tauri deployment. |
| JS note's `style-src` omits `blob:`. Security note includes it. | **Include `blob:`.** | foliate-js rewrites stylesheets to blob URLs, and the policy that was probed included it. Readest's looser `script-src` must not be copied in any form. |

## Nothing here has run in a real Tauri build

The strongest security evidence is a Swift harness that copies wry's setup, with a fake internals object. Readest proves foliate-js works in real Tauri on macOS, but under a far looser CSP than the one proposed. The strict combination is therefore untested as a whole. The table separates what is known from what is assumed.

| Claim | Evidence tier | Still unverified |
| --- | --- | --- |
| Strict parent CSP stops inline script, `onerror`, nested frames and remote loads in blob and srcdoc frames (S1, S2, S9 to S17) | Raw WKWebView probe | Tauri's header plus `reader.html` meta, in foliate-js frames |
| A meta CSP in the creator document binds its blob children (X1) | Raw probe, no same-run control | Whether it works at all in Tauri; this is load-bearing because Tauri has one app-wide CSP |
| Same-origin frame reaches parent internals | Raw probe, plus a real Tauri exploit in Readest's advisory | Nothing material |
| App commands skip the ACL without a manifest; a manifest enforces labels | Source reading at tauri 2.12.5 | Never exercised in a build |
| Invoke key rejects subframe calls; `null` origin fails parsing | Source plus harness | End-to-end rejection in a build |
| `tauri://` is a secure context, so `crypto.subtle` works for deobfuscation | Raw probe | Obfuscated font in the reader; a custom `sha1` is the fallback |
| Draft `reader.html` policy with `default-src 'none'` | Inference | Whole policy; only a related policy was probed |
| `tauri dev` does not apply the CSP; a `<style>` in built HTML cancels `'unsafe-inline'` | Inference from source | Both |
| `on_navigation` and WKContentRuleList guards | Raw probe | Tauri path; a URL parse failure returns allow, and the rule list needs an `unsafe` call that the app crate's `unsafe_code = "forbid"` lint rejects |
| foliate-js bundles at 57.5 KB gzip with `pdf.js` stubbed | Vite 7.3.7 build of upstream | The project locks Vite 8.3.5, and the fork's `pdf.js` differs; the fork was never built |
| Crate feature tables | Grep-level | Behaviour on real books |
| kepubify span mapping | Synthetic two-chapter book | Real KEPUBs; nothing touched Kobo hardware |
| Security effort figures (for example 1 to 2 days for the window and manifest) | Researcher estimate | Not measured |

The sparse-checkout mistake changes one claim. I re-read both clones, now full at the pinned commits (231 files in foliate-js, 5,045 in Calibre, sparse checkout off), though that shows the current trees and not what each researcher saw. The claim that foliate-js has no TypeScript declarations holds, including in the fork, and the 7,104-line upstream count reproduces. **The claim that Calibre "does not write `Bookmark`" is wrong as stated**: its Kobo driver deletes `Bookmark` rows when removing books ([driver.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/driver.py#L2987)). I found no insert or update of Kobo `Bookmark` rows there, so the note's conclusion survives: Calibre's schema range says nothing about highlight write safety. The gap on how Calibre blocks network access from book script also survives a fresh search of its viewer code, which found no CSP or request interceptor, so do not cite Calibre as a network-blocking model.

## A short spike must settle eight questions

Run these in a minimal Tauri 2.12.5 build, not `tauri dev`, in this order, because the early items gate the design.

| Question | Test | Pass condition | If it fails |
| --- | --- | --- | --- |
| Does the manifest enforce labels? | Add `AppManifest` with all 61 commands, grant them to `main`, and give `reader-*` only the reader commands | `device_restore` from a reader window is rejected; `main` regresses nothing | Stop: blob frames lose their safety net, so move to the custom-scheme origin |
| Does CSP compose? | Tauri header plus `reader.html` meta; hostile fixture from [epub-test](https://github.com/johnfactotum/epub-test), srcdoc path included, local logging server | No script runs, zero server hits, including cover hosts and `asset:` from the outer policy | Inject a meta CSP per section, which the harness showed is enforced (P4) |
| Does the fork build and render under that CSP? | Fork on Vite 8.3.5 with `pdf.js` stubbed; check `dist` for nonces; load an obfuscated-font book | Styles, images and fonts all load | Use `dangerousDisableAssetCspModification` for `style-src`, or fix the fork |
| Do all engine events work? | Selection, keyboard, wheel and trackpad, links, footnotes, with script blocked | All work on macOS 27 | Patch the fork; only `click` is proven |
| Which byte path is fast enough? | Entry command against whole-file command, then a `book://` transport if needed, on small and 100 MB-plus books, with and without `ipc:` in `connect-src` | Page turns show no stall | Use `book://` for bytes only, limited to the open book, with no document origin |
| Do the native guards work? | `on_navigation` with `blob:` and `about:srcdoc`; popups; rule list via `with_webview` | Allowlist holds and no parse-failure allow | Drop the rule list if the `unsafe` cost outweighs a layer CSP already covers |
| Does the engine survive awkward books? | epubtest fundamentals, a 20 MB single-file book, vertical Japanese, RTL, FXL, self-closed `<title/>`, and the real 93-book library | No freezes or blank chapters | File against the fork; decide which classes block v1 |
| Do locators hold? | Round-trip Locator and CFI; foliate-js's filter on a real KEPUB; read-only query of a database backup for `ChapterIDBookmarked` and offset semantics | Highlights anchor to the right text, with a measured hit rate | Fall back to text quote plus `ChapterProgress` |

Two items sit outside the spike. Import should start capturing `Bookmark`'s `Start*` and `End*` columns now, because those rows can vanish when a book leaves the device. VoiceOver needs hands-on testing later; the leading accessible reader avoids paginated iframes by forcing scroll mode ([Thorium](https://thorium.edrlab.org/en/th3/500_accessibility/_print/)), and no test of VoiceOver in paginated WKWebView was found.

## Conclusion

The research shifts the question from which library to where the trust boundary sits. Readest proves the engine and also shows that the same stack shipped the exact failure this design must avoid, so the first commit on this feature should be the app manifest, which improves the app even if the reader slips. Adoption cost is also ongoing: the project takes on a fork of a fork, with one maintainer upstream of it and WebKit fixes it will sometimes write itself. The recommendations below hold if the spike's first two rows pass.

| Component | Decision | What to use or write |
| --- | --- | --- |
| Rendering, pagination, FXL, media overlays | **Use** | `readest/foliate-js` at `b34a0aa`, as a project-owned fork; track upstream |
| Parsing for display, font deobfuscation, TOC | **Use** | foliate-js in the webview |
| CFI generate and resolve, in-book search, progress, footnote extraction | **Use** | foliate-js `epubcfi.js`, `search.js`, `progress.js`, `footnotes.js` |
| TypeScript types, Vite `pdf.js` stub | **Write** | Hand-written `.d.ts` for the surface used; alias `pdf.js` |
| React host | **Write** | Thin effect-based component; skip react-reader |
| Reader UI: TOC, settings, theming, footnote popup, annotations, input | **Write** | About 5k to 8k lines at the Foliate tier; read Readest for ideas, do not copy it |
| Rust book access | **Write** | Narrow entry command on the `zip` crate, book ID only, one open archive per book; no paths or URLs from the reader |
| Rust read-side model | **Extend** | Promote erc-epub's `Parts`; defer `rbook` until Rust needs nav; leave the byte-preserving edit path alone |
| `book://` document origin | **Do not build for v1** | Keep as the fallback if the first spike row fails |
| Trust boundary | **Use Tauri, write config** | App manifest, `reader.json` capability, dedicated incognito window, separate small reader bundle, CSP in `reader.html`, no `blob:`, `'unsafe-inline'` or `'unsafe-eval'` in `script-src` |
| Book scripts | **Do not support in v1** | Conformant if fallbacks are honoured; keep `allow-scripts` only for the WebKit event bug |
| Sanitising | **Do not use `ammonia` or `lol_html`** | Optional in-place DOM neutralisation only |
| Stored position and annotation model | **Write** | Locator layers with raw Kobo fields; replaces the current `REAL` position for cross-format use |
| Text anchoring | **Use and write** | [`approx-string-match`](https://www.npmjs.com/package/approx-string-match) with a small in-house layer; avoid archived Apache Annotator and AGPL `epub-cfi-resolver` |
| Kobo to EPUB mapping | **Write, read-only** | Span table from the actual device KEPUB, keyed by per-entry hash; keep the kepubify sidecar; no highlight write-back |
| Rust CFI | **Do not adopt a crate** | Opaque strings; write a small comparator only if Rust must sort |
| Cross-library search (later) | **Use and write** | SQLite FTS5, already compiled in, with a resumable index job; XML-first extraction with an `html5ever` fallback; no tantivy yet |
| Native or Rust rendering | **Do not use** | Revisit Servo and Blitz in a year or more |
| Test corpus | **Use and write** | epub-test, epubtest.org books and W3C epub-tests, plus a local stress set and a hostile fixture run against a built app |
