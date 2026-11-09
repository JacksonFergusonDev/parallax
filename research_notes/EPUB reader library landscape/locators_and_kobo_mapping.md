# Reading positions, annotations, and plain EPUB to Kobo KEPUB mapping

Evidence labels used throughout, as of 2026-11-07:

- **[source]**: read directly in the project's source at the cited commit.
- **[ran]**: run locally against a synthetic 2-chapter EPUB, using the project's own bundled `app/src-tauri/binaries/kepubify-aarch64-apple-darwin` (reports v4.0.4), with output in a scratch directory. The CFI paths came from a small throwaway script, not a conformant CFI library.
- **[doc]**: stated in project docs, a spec, or a forum post, and not independently checked.
- **Nothing here was tested on Kobo hardware.** No device was touched. Every statement about Nickel's behaviour is second-hand, from other projects' source or from forum posts.

## Which standards and libraries should the app adopt?

### Takeaway

Adopt a Readium-style Locator as the stored model (href, progression, totalProgression, text before/highlight/after), with the raw Kobo position kept beside it, and export through W3C Web Annotation shapes. Treat CFI as an optional cached fast path via foliate-js, not as the source of truth. Map Kobo to desktop read-only using the actual KEPUB file on the device. Defer highlight write-back, and treat position write-back as a later, opt-in experiment.

### Cited Findings

- The Readium Locator has required `href` and `type`, optional `title`, `locations` (`fragments`, `progression`, `totalProgression`, `position`) and `text` (`before`, `highlight`, `after`). Its `locations` are alternative expressions of one position, and fragments are the extension point. The page does not mention CFI [doc] — [Readium Locators](https://readium.org/architecture/models/locators/)
- Readium Annotations is a profile of the W3C Web Annotation Data Model, serialised as JSON for embedding or sharing; repo last pushed 2026-09-03 — [readium/annotations](https://github.com/readium/annotations)
- EPUB Annotations 1.0 is a W3C Working Draft dated 15 October 2026 and has neither `TextQuoteSelector` nor CFI in its text — [W3C TR](https://www.w3.org/TR/epub-anno-10/)
- foliate-js ships a 349-line `epubcfi.js` (MIT, last changed 2026-05-01) with a node filter for injected markup [source] — [epubcfi.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/epubcfi.js)
- Kavita, the closest prior art found, maps between XPath and KoboSpan by opening both the EPUB and the KEPUB [source] — [KoboPositionHelper.cs](https://github.com/Kareadita/Kavita/blob/c78f8a8/Kavita.Services/Helpers/Kobo/KoboPositionHelper.cs)
- Local repo gaps: `KoboDb::annotations` selects `Text`, `Annotation`, `ChapterProgress`, dates and `Type` but not `StartContainerPath`, `StartContainerChildIndex`, `StartOffset` or the `End*` columns, and computes `position` as spine index plus `ChapterProgress` [source] — `crates/device-kobo/src/annotations.rs` lines 95-108 and 192
- `Reading.percent` is a `u8`, and `Annotation.position` is an `f64` [source] — `crates/library/src/stores.rs`
- The app calls the kepubify sidecar with default conversion options; the only flag found in `crates/convert` is `--output` [source] — `crates/convert/src/lib.rs` line 88

### Inferences

Suggested stored shape, in resolution order:

| Layer | Field | Role |
| --- | --- | --- |
| 1 | `source.kobo`: span id, child index, offset, `ContentID`, KEPUB sha256, converter name and version | Raw evidence, kept verbatim, so mapping can be redone later |
| 2 | `href` + text offsets (TextPositionSelector equivalent) | Exact location in the plain EPUB while its text is unchanged |
| 3 | `text.before/highlight/after` | Survives edits and re-conversion |
| 4 | `progression` within the resource | Coarse fallback |
| 5 | `totalProgression` | Last resort, display only |
| Optional | `cfi` | Cache for foliate-js navigation, regenerated freely |

- The highest-value immediate change is capturing the `Start*`/`End*` columns in the import now. Those rows can vanish when a book is removed from the device, and the app currently discards the information needed for precise mapping.
- `Annotation.position` should not be compared across formats, because its spine index comes from the device's copy, which may include kepubify's dummy title page (see the CFI section). Key on the file href instead.
- Record the kepubify version per sent book. `.parallax/books.json` already holds `sha256` and `parallax_version` per `docs/device-notes.md`, but no converter version is listed.

### Gaps

- No single standard covers this case. No W3C or Readium document defines a mapping between an EPUB and a derived KEPUB.
- The final EPUB Annotations selector set is not settled, so a compliant export format cannot be frozen yet.

## EPUB CFI: status, libraries, pitfalls, and does kepubify invalidate it?

### Takeaway

CFI is a 2017 IDPF specification with no W3C standing and an implementation-defined processing model. kepubify invalidates plain-EPUB CFIs in every case tested: the path changes in every content document and the spine step can change too. A CFI can be carried across only by making the KEPUB's added nodes transparent, which foliate-js's filter appears able to do, but that is untested.

### Cited Findings

- Spec: EPUB Canonical Fragment Identifiers 1.1, IDPF Recommended Specification, 5 January 2017 — [IDPF](https://idpf.org/epub/linking/cfi)
- At the W3C Publishing Maintenance WG on 2026-07-23, Ivan Herman said CFI is not in EPUB 3.4, not referenced by it, and not a W3C Note. He said review had found it "half baked" with no processing model, and "very bound to xml". No decision was taken, though the group may revisit it alongside annotations. These are meeting remarks, not a formal position — [minutes](https://w3c.github.io/pm-wg/minutes/2026-07-23.html)
- Avneesh Singh said in the same meeting that some reading system developers are very invested in CFI and others dislike it — [minutes](https://w3c.github.io/pm-wg/minutes/2026-07-23.html)
- Robustness relies on optional ID assertions and text location assertions. On a mismatch, readers should try to locate the target and correct the CFI. Without a match the CFI is invalid — [CFI editor's copy](https://w3c.github.io/epub-specs/epub33/epubcfi/)
- Offsets count UTF-16 code units, even steps are elements, odd steps are character-data chunks, and comments are ignored — [CFI editor's copy](https://w3c.github.io/epub-specs/epub33/epubcfi/)

Libraries (npm dates read from the registry on 2026-11-07):

| Library | Latest | Date | License | Note |
| --- | --- | --- | --- | --- |
| foliate-js `epubcfi.js` | git only | 2026-05-01 | MIT | Parse, compare, `fromRange`, `toRange`, node filter. Used by Readest. **Stale risk: low** |
| [`epubjs`](https://www.npmjs.com/package/epubjs) | 0.3.93 | 2022-02-16 | BSD-2-Clause | Flag as old |
| [`epub-cfi-resolver`](https://www.npmjs.com/package/epub-cfi-resolver) | 1.0.2 | 2021-06-02 | AGPL-3.0 | Flag as old. Repo is GPLv3, so check licensing before use |
| [`readium-cfi-js`](https://www.npmjs.com/package/readium-cfi-js) | 1.0.0-alpha.1 | 2018-11-08 | BSD-3-Clause | Flag as old, effectively legacy |
| Rust crates `cfi`, `epub-cfi`, `epubcfi`, `rbook-cfi` | none | n/a | n/a | crates.io returned "does not exist" for all four names on 2026-11-07. Other names were not searched |

- foliate-js's filter returns `FILTER_SKIP` to flatten a node's children into its parent, then merges adjacent text nodes into one chunk, so skipped wrappers do not add steps [source] — [epubcfi.js lines 191-231](https://github.com/johnfactotum/foliate-js/blob/78914ae/epubcfi.js#L191-L231); [README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md)
- `partsToNode` resolves straight to `getElementById` when the last part has an id, so id assertions take priority over steps [source] — [epubcfi.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/epubcfi.js)
- kepubify mandatorily adds `div#book-columns > div#book-inner` around body content and `koboSpan` spans around sentence fragments and images [source] — [transform.go](https://github.com/pgaskin/kepubify/blob/9546034/kepub/transform.go#L118-L130)
- By default kepubify may prepend `kepubify-titlepage-dummy.xhtml` to the spine, and the code says that heuristic "is subject to change between kepubify versions" [source] — [transform.go](https://github.com/pgaskin/kepubify/blob/9546034/kepub/transform.go#L649-L662)

Measured effect on the synthetic book [ran]:

| Target | Plain EPUB path | KEPUB path |
| --- | --- | --- |
| Third paragraph | `/4/12[p3]/1:0` | `/4/2[book-columns]/2[book-inner]/12[p3]/2[kobo.4.1]/1:0` |
| "And more" (inside `em`) | `/4/4/2/1:10` | `/4/2[book-columns]/2[book-inner]/4/6/4[kobo.2.4]/1:0` |
| "text after" | `/4/4/3:1` | `/4/2[book-columns]/2[book-inner]/4/8[kobo.2.5]/1:1` |
| "Bare div" | `/4/8/1:0` | `/4/2[book-columns]/2[book-inner]/8/2[kobo.3.2]/1:0` |

- The same run added the dummy title page, giving spine `kepubify-titlepage-dummy, c1, c2`, which shifts the CFI spine step by one [ran]
- Body text was unchanged: `textContent` was identical in both files at 213 characters [ran]
- Pitfall seen in the wild: calibre's text-node interpretation disagreed with both Kobo and `epub-cfi-resolver` for some highlights, and some stayed shifted by a few words after correction [doc, 2024] — [kobo-highlights-to-calibre](https://github.com/valeriangalliat/kobo-highlights-to-calibre/blob/7f04a7a/README.md)

### Inferences

- A plain-EPUB CFI is not usable on a KEPUB, or the reverse, without translation. Element steps below `body` shift, offsets reset at each span, and the spine step can move.
- Passing foliate-js a filter that skips `span.koboSpan`, `#book-columns` and `#book-inner` should produce a KEPUB-derived CFI that resolves on the plain EPUB. The source supports this, but it was not executed. Two risks are `nodeToParts` id handling and the whitespace text kepubify leaves around wrappers.
- CFI's strength here is cheap sorting and navigation within one exact file. It is weak as durable storage because any re-conversion or edit that moves nodes breaks it, and recovery depends on the same text assertions that a text quote already provides.

### Gaps

- Conformance of the libraries against each other was not tested. No maintained shared CFI test suite was found.
- The foliate-js filter approach needs a test against real KEPUBs before it is relied on.

## Readium Locator, Web Annotation selectors, and text-quote robustness

### Takeaway

Yes: for cross-format mapping a text quote is more robust than CFI, because kepubify preserves text while restructuring the DOM. Text position offsets are the most precise while text is unchanged, so store both. Use a maintained fuzzy matcher, because the older anchoring packages are stale or archived.

### Cited Findings

- EPUB Annotations 1.0 draft: one Target per annotation; `source` is a relative URL of a top-level content document; `selector` is an array. It defines FragmentSelector, CssSelector and TextPositionSelector, with `refinedBy` chaining. It tells processors to try precise selectors first, fall back when content changed, and ignore unknown types [doc] — [editor's draft](https://w3c.github.io/epub-specs/epub34/annotations/index.html)
- TextPositionSelector counts over DOM `textContent` of the scope, and the draft recommends pairing it with a modification-resistant selector. Text Fragments is flagged AT RISK because its source is a Draft Community Group Report [doc] — [editor's draft](https://w3c.github.io/epub-specs/epub34/annotations/index.html)
- In January 2026 the WG preferred the Text Fragment selector, with CSS as the alternate and a spatial fallback. Brady Duga favoured resiliency over performance and a small set of selector types — [minutes 2026-01-29](https://w3c.github.io/pm-wg/minutes/2026-01-29.html)
- Readium's selector proposal lists CssSelector, TextFragmentSelector, SpatialSelector, TemporalSelector and RangeSelector with TextNodeIndexSelector and CodeUnitSelector. It warns that mapping a text fragment to a DOM Range is "still challenging" [doc] — [Selectors.md](https://raw.githubusercontent.com/readium/annotations/main/Selectors.md)
- The W3C model's TextQuoteSelector remains available for storage, even though the EPUB draft omits it — [W3C Web Annotation Data Model](https://www.w3.org/TR/annotation-model/)
- Apache Annotator's repository is archived (last pushed 2024-06-23), and `@apache-annotator/dom` and `/selector` are at 0.2.0 from 2021-09-03 — [apache/incubator-annotator](https://github.com/apache/incubator-annotator); [npm](https://www.npmjs.com/package/@apache-annotator/dom)
- Its matcher does an exact search of quote, prefix and suffix and returns all matches rather than picking one — [API docs](https://annotator.apache.org/docs/api/modules/dom.html)
- `dom-anchor-text-quote` is at 4.0.2 from 2017-02-10, so treat it as stale — [npm](https://www.npmjs.com/package/dom-anchor-text-quote)
- `approx-string-match` 2.0.1 was published 2026-10-09 (MIT) — [npm](https://www.npmjs.com/package/approx-string-match)
- `@readium/navigator` 2.15.0 was published 2026-11-06 (BSD-3-Clause) — [npm](https://www.npmjs.com/package/@readium/navigator)
- Kobo's `Bookmark.Text` holds the highlighted string, and the row's `ContentID` identifies the chapter file — [kobo-highlights-to-calibre](https://github.com/valeriangalliat/kobo-highlights-to-calibre/blob/7f04a7a/README.md)

### Inferences

- `Bookmark.Text` plus the chapter file gives a usable text quote for every highlight, independent of span parsing. This is the simplest reliable path and works for books from any converter.
- A bare quote is ambiguous for short repeated phrases. Disambiguate with `ChapterProgress` as a position hint, or with prefix/suffix derived from the span offsets. `approx-string-match` covers fuzzy matching, and writing the small anchoring layer in-house is likely better than adopting an archived package.
- Whitespace and punctuation normalisation will be needed before matching, since CFI itself collapses whitespace for assertions and Kobo's stored text may differ from DOM text. The exact differences have not been measured.
- Dog-ears have no `Text`, so they need a span or `ChapterProgress` position only.

### Gaps

- No measured success rate exists for quote anchoring on Kobo highlights. A dry run against the reference device's data would give one.
- Whether Text Fragment semantics will suit EPUB content is undecided in the WG.

## Kobo position data and the kepubify span scheme: is the mapping deterministic?

### Takeaway

For one kepubify version, yes. The span algorithm is simple and content output was deterministic in testing. Across versions there is explicitly no guarantee, and the maintainer says applications should save the output or pin kepubify. So do not recompute: read the real KEPUB from the device, or a saved copy, and derive a span-to-text-offset table from it. A clean-room recomputation is possible but imperfect, at a self-reported 543 of 562 books.

### Cited Findings

Span algorithm [source], from [transformContentKoboSpans](https://github.com/pgaskin/kepubify/blob/9546034/kepub/transform.go#L300-L386):

- Ids are `kobo.<para>.<seg>`, class `koboSpan`, in a depth-first walk from `body`, with counters per document.
- `p`, `ol`, `ul`, `table` and `h1`-`h6` set a pending paragraph increment, applied only when the next span is emitted. Leaving an element changes nothing.
- Each `img` immediately increments `para`, resets `seg`, and gets its own span.
- `script`, `style`, `pre`, `audio`, `video`, `svg` and `math` are skipped entirely.
- Whitespace-only segments are unwrapped unless the direct parent is `p`.
- Sentence splitting uses `. ! ?`, then optional `' " ” ’ “ …`, then ASCII whitespace [source] — [splitSentences](https://github.com/pgaskin/kepubify/blob/9546034/kepub/transform.go#L398-L537)
- Documents that already contain `koboSpan` are left unspanned, and the code comment says behaviour "matches Kobo (checked with 3 books) as of 2020-01-12" [source] — [transform.go](https://github.com/pgaskin/kepubify/blob/9546034/kepub/transform.go#L300-L304)

Observed output confirms these rules [ran]: `Mr.` splits as its own span, `co.&#160;left` does not split because NBSP is not ASCII whitespace, `<p> </p>` consumes `kobo.3.1`, a bare `div` continues the same paragraph number as `kobo.3.2`, `pre` is untouched, and `img` gets `kobo.5.1`.

Determinism and stability:

- Two runs produced identical entry contents, but the zip files were not byte-identical [ran]. So hashing the zip is not a valid identity check for re-conversions.
- Latest release is v4.0.4 of 2022-03-09, and master has been static since 2022-05-21 — [releases](https://github.com/pgaskin/kepubify/releases)
- A `v5-dev` branch gained 9 commits dated 2026-11-07, today. Its transform diff so far only changes sentence splitting to iterators, plus doc reformatting and other non-span edits [source] — [compare](https://github.com/pgaskin/kepubify/compare/master...v5-dev)
- pgaskin, 2026-11-01: v5 "will keep the exact same algorithm for spans, but may differ slightly on malformed HTML", and output "will not be bit-identical" — [issue 120](https://github.com/pgaskin/kepubify/issues/120)
- pgaskin, 2026-11-01: `textarea` and `picture` cases "will be skipped entirely" rather than counted, so later ids in those rare documents will change. He added that kepubify "provides no formal guarantees on identical output across versions", and that applications depending on this "save the output or pin kepubify" — [issue 119](https://github.com/pgaskin/kepubify/issues/119)
- kepubify parses XHTML with an HTML parser. kepubverto reports self-closed non-void elements such as `<pre/>` or `<div/>` swallowing following content, with 122 of 544 test books containing one [doc, self-reported] — [SPANS.md](https://github.com/veripublica/kepubverto/blob/706a86a/docs/SPANS.md)
- Note a self-closed `<a id="anchor"/>` came through fine in the local run [ran]; `<pre/>` and `<div/>` were not tested.
- kepubverto is a clean-room Rust converter (AGPL-3.0 or commercial, v0.4.0) that aims for span-id parity. It reports identical spans to kepubify in 543 of 562 books, with deliberate differences on malformed input [doc, self-reported] — [issue 120](https://github.com/pgaskin/kepubify/issues/120); [SPANS.md](https://github.com/veripublica/kepubverto/blob/706a86a/docs/SPANS.md)
- kepubverto's own doc still marks "Kobo readers record … against the id" as "to be confirmed on a device" [doc] — [SPANS.md](https://github.com/veripublica/kepubverto/blob/706a86a/docs/SPANS.md)
- kepubify is MIT licensed, so a port is legally simple [source] — [LICENSE](https://github.com/pgaskin/kepubify/blob/9546034/LICENSE)

Kobo's columns:

- KEPUB example: `StartContainerPath = span#kobo\.153\.1`; EPUB example: `epub/text/endnotes-2.xhtml#point(/1/4/2/4/38/1:6)` [doc, January 2024] — [MobileRead](https://www.mobileread.com/forums/showthread.php?t=358401)
- A second KEPUB-style example shows `span#kobo\.114\.3` — [samedwardes.com](https://samedwardes.com/blog/2024-06-23-querying-kobo-with-duckdb/)
- Plain EPUB row: `StartContainerPath = OEBPS/cha01.xhtml#point(/1/4/2/26/1:223)`, `StartContainerChildIndex = -99`, `StartOffset = 0`, with `ContentID` of the form `<volume>#(6)OEBPS/cha01.xhtml` [doc, 2024] — [kobo-highlights-to-calibre](https://github.com/valeriangalliat/kobo-highlights-to-calibre/blob/7f04a7a/README.md)
- That exporter reports the `point()` offset is a **byte** offset, not a character offset, and its paths use root `/1` where valid CFI starts `/2` [doc, 2024] — [kobo-highlights-to-calibre](https://github.com/valeriangalliat/kobo-highlights-to-calibre/blob/7f04a7a/README.md)
- calibre reads `ChapterProgress`, `StartContainerChildIndex` and `StartOffset`, treats `StartContainerChildIndex == StartOffset == 0` as a dog-ear, and multiplies `ChapterProgress` by 100, so it is a 0-1 chapter fraction [source] — [bookmark.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/bookmark.py)
- The Kobo sync API location is `{Value: "kobo.x.y", Type: "KoboSpan", Source: <HTML resource>}`, alongside `ProgressPercent` and `ContentSourceProgressPercent` [source] — [LocationDto.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/interfaces/api/kobo/dto/LocationDto.kt); [BookmarkDto.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/interfaces/api/kobo/dto/BookmarkDto.kt)

### Inferences

- Because spans only wrap existing text, each span's starting offset within body `textContent` in the KEPUB should equal the same offset in the plain chapter. Text equality held in the one synthetic test [ran]. The mapping is then `kobo.P.S` plus intra-span offset to a text offset to a DOM Range in the plain EPUB, with no fuzzy step.
- `StartContainerChildIndex` and `StartOffset` for KEPUB rows probably give a child and offset within the span, but this is unconfirmed.
- Exceptions where that text identity fails: `transformContentClean` removes some content, optional `--smarten-punctuation` rewrites text (the app does not pass it), and malformed documents may parse differently between kepubify's HTML parser and the webview's XML parser. The synthetic book had no straight quotes or dashes, so its "smarten unchanged" result proves nothing.
- Practical rule: the device's own file is ground truth. The reference device has 93 sideloaded KEPUBs previously managed by calibre (`docs/device-notes.md`), so they were likely produced by other tools or versions. Recomputing from the library EPUB with the current sidecar could silently disagree.
- When the file on the device has been read, cache its span table keyed by content hash. Use a per-entry content hash rather than the zip hash, given the non-identical zips above.
- Porting the algorithm to Rust is only worth doing as a fallback for when the KEPUB is gone. A sidecar run at the pinned version is simpler. kepubverto is AGPL, so its code should not be taken without a licensing check.

### Gaps

- Exact format of `content.ChapterIDBookmarked` for a KEPUB on current firmware was not found in any primary source. Kobo Utilities treats it as opaque. One read-only query on the reference device's database backup would settle this and the `StartContainerChildIndex`/`StartOffset` semantics.
- Whether KEPUB offsets are bytes (as for the `point()` form) or UTF-16 units is unknown.
- Whether Nickel tolerates or re-derives positions after a file is replaced is unknown.

## Prior art: what have sync projects achieved, and has anyone written back over USB?

### Takeaway

Everyone who syncs Kobo positions with precision does it through Kobo's sync API with KoboSpan values, not through USB, and mapping to a web reader is approximate. Kavita's new helper is the best reference design. Position restore over USB exists only as Kobo Utilities' store-and-restore of values the device itself produced. No tool was found that writes highlights into `Bookmark`, or that synthesises a position from another reader.

### Cited Findings

| Project | Direction | Mechanism | Precision and limits | Evidence |
| --- | --- | --- | --- | --- |
| calibre driver | Device to calibre | Reads `Bookmark` | Annotation text only, inserted into comments | [source](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/driver.py) |
| Kobo Utilities | Device to calibre and back, USB | Stores then restores `ChapterIDBookmarked`, `___PercentRead`, `ReadStatus`, `DateLastRead`, `FirstTimeReading`, and `adobe_location` for old EPUB | Same-book restore only. No `Bookmark` writes found by search | [source](https://github.com/janlarres/kobo-utilities/tree/7760787/koboutilities/features) |
| calibre-web | Kobo sync API | Stores `Location` value, type, source opaquely and echoes it | Kobo-to-Kobo roundtrip; no cross-format mapping in the code read | [source](https://github.com/janeczku/calibre-web/blob/master/cps/kobo.py) |
| Komga | Two-way, sync API | Nearest span by byte progression | "May be off by a few pages" | [source](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/infrastructure/mediacontainer/epub/EpubExtractor.kt), [doc](https://komga.org/docs/guides/kobo) |
| BookLore | Two-way option, sync API | Web to Kobo sends percent only, no Location; Kobo to web keeps href and percent and clears the web position | Chapter or percent level | [source](https://github.com/booklore-app/booklore/blob/develop/booklore-api/src/main/java/org/booklore/service/kobo/KoboReadingStateService.java) |
| Kavita | Two-way, sync API | XPath to and from KoboSpan via ids or body-relative paths with text check | Block level; `ContentSourceProgressPercent = 0 // We cannot track this` | [source](https://github.com/Kareadita/Kavita/blob/c78f8a8/Kavita.Services/Helpers/Kobo/KoboPositionHelper.cs) |
| KOReader kosync | Position only | `progress` xpointer plus `percentage`, keyed by partial MD5 or filename | Binary digest differs between EPUB and KEPUB. Highlights not included | [source](https://github.com/koreader/koreader/blob/master/plugins/kosync.koplugin/main.lua) |
| Readest | kosync client | foliate CFI to and from xpointer, percentage as weak fallback | Its code says a server's percentage is not directly comparable | [source](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/hooks/kosyncProgress.ts) |
| BeePub v0.6.0 | kosync | Web reader computes crengine xpointers; falls back paragraph, in-chapter percent, chapter start | Self-described paragraph-exact, for matching digests, on KOReader not Nickel | [doc](https://github.com/oalieno/beepub/releases/tag/v0.6.0) |
| kobo-highlights-to-calibre | One way, into calibre | Kobo `point()` to CFI with byte-to-character fix | Works "the majority of the time"; EPUB only; some highlights off or missing | [doc](https://github.com/valeriangalliat/kobo-highlights-to-calibre/blob/7f04a7a/README.md) |

- Komga's source notes that when a book is finished "Kobo sends the first resource instead of the last", and also says "don't trust the provided total progression, the one from Kobo can be wrong" [source] — [KoboController.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/interfaces/api/kobo/KoboController.kt); [BookLifecycle.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/domain/service/BookLifecycle.kt)
- Komga states mid-chapter progress is lost for regular EPUBs on Kobo because Kobo records only the chapter start [doc] — [Komga guide](https://komga.org/docs/guides/kobo)
- Kavita's position work landed recently: the migration is dated 2026-11-01, with unit tests for encode, decode and round trip. Its real-world accuracy is unmeasured [source] — [KoboPositionHelperTests.cs](https://github.com/Kareadita/Kavita/blob/c78f8a8/Kavita.Services.Tests/Helpers/KoboPositionHelperTests.cs)
- Forum reports on Kobo Utilities restore: it "worked like a charm" after a factory reset in March 2025 — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=4500048)
- The book must already be in the device database before restore, so the order is store, resend, disconnect, wait for home screen, reconnect, restore (older posts) — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=2614029)
- Sideloaded rows are keyed by file name and location. If those differ on the new device, it deletes the rows and treats the books as new (davidfor, 2017, old) — [MobileRead](https://www.mobileread.com/forums/showthread.php?t=286686)
- No thread read documents a successful `Bookmark` re-import. Whole-database replacement is the documented route — [MobileRead](https://www.mobileread.com/forums/showthread.php?t=286686); [softhints](https://softhints.com/how-to-backup-and-reuse-kobo-annotations-on-pc/)
- `export-kobo`'s maintainer says no official specification exists, so schema changes are learned from user-supplied databases — [export-kobo](https://github.com/pettarin/export-kobo)
- Kobo's own export reportedly does not support highlights from sideloaded files, which makes the local database the main source — [kobohighlightsexport.com](https://kobohighlightsexport.com/export-sideloaded-kobo-highlights), a vendor page, so weak evidence

### Inferences

- Kavita's design transfers well to USB import. Our version can do better than block level, because text offsets give sentence or character precision without its XPath step.
- Komga's nearest-span-by-bytes approach is a good model for the reverse direction only as a fallback.
- The sync-API route would need a redirected Kobo API endpoint, a different product than this USB-based app, and is out of scope unless the design changes.

### Gaps

- obok, DeDRM and kobo-book-downloader source was not examined. Their described purpose is DRM removal and downloading, so they are unlikely to bear on positions, but this was not confirmed.
- Kavita's `KoboAnnotations.cs` DTO was not read, so whether it syncs annotations is unknown.
- BookLore's and calibre-web's links point at moving branches, and only the checkouts of 2026-11-07 were read.
- Absence of a `Bookmark` write-back tool is a negative result from searches, not proof none exists.

## Which firmware changes affect these columns?

### Takeaway

The one documented location-format change is 4.17.13651, when plain-EPUB reading locations began to look like KEPUB ones. Otherwise the schema has only been tracked empirically. Read columns defensively and gate writes on known versions, which the repo already does. Nothing found documents a `Bookmark` change in 2025-2026.

### Cited Findings

- calibre: `min_fwversion_epub_location = (4, 17, 13651)`, commented "ePub reading location without full contentid" [source] — [driver.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/driver.py#L1692)
- Kobo Utilities branches on the same constant (`epub_location_like_kepub`). Before it, `ChapterIDBookmarked` carried the ContentID and `adobe_location` held the sub-position, and values matched `\((\d+)\)(.*)\#?.*` [source] — [action.py](https://github.com/janlarres/kobo-utilities/blob/7760787/koboutilities/action.py#L1022-L1031); [toc.py](https://github.com/janlarres/kobo-utilities/blob/7760787/koboutilities/features/toc.py)
- calibre KOBOTOUCH: `supported_dbversion = 222`, `min_supported_dbversion = 53`, `max_supported_fwversion = (6, 0, 274403)`, and `min_fwversion_bookstats = (4, 32, 19501)` [source] — [driver.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/driver.py#L1661-L1701)
- calibre's device database path remains `.kobo/KoboReader.sqlite` [source] — [driver.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/driver.py#L320)
- Reference device: Libra 2, firmware 4.38.23697, DbVersion 174, WAL mode, `TimeSpentReading` in seconds [doc] — `docs/device-notes.md`
- Firmware 4.45.23640 was reported released in late February 2026 — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=4571369)
- Recent calibre commits to the Kobo driver through 2026-11-02 are typing and lint changes, not schema logic [source] — [commits](https://github.com/kovidgoyal/calibre/commits/master/src/calibre/devices/kobo/driver.py)

### Inferences

- The repo's existing `columns()` probing and 53-222 write gate fit this situation well. `calibre` validating schema up to 222 says nothing about `Bookmark` write safety, because calibre does not write `Bookmark`.
- Any write-back should have its own stricter allowlist of exact firmware and DbVersion pairs tested on the user's hardware.

### Gaps

- A search summary claimed "firmware 4.39+ on some models" nests the database in `.kobo/db/`. I could not attribute this to a specific page, a targeted search found no support, and calibre's path contradicts it. Treat as unverified, and check on a 4.39+ device.
- No changelog of `Bookmark` columns exists. Kobo publishes no schema documentation.
- Stylus markup storage across firmware was not researched.

## Read-only versus write-back: what is achievable, how precise, at what risk?

### Takeaway

Read-only import is very achievable, at sentence-to-exact precision with low device risk. Position write-back is plausible at span precision but unproven when synthesised. Highlight write-back has no prior art and should stay out of scope until proven on a sacrificial device or book.

### Cited Findings

- The app already has read-only `immutable=1` access, a safe-write layer, pre-change backups, and WAL checkpoint discipline [source, doc] — `crates/device-kobo/src/db.rs`; `docs/device-notes.md`
- The device is signed in with cloud sync active [doc] — `docs/device-notes.md`
- Project rule: never write to the Kobo volume outside the safe-write layer or a user-run script [doc] — `PROGRESS.md`
- The app's maintenance feature already clears `ChapterIDBookmarked`, `___PercentRead`, `DateLastRead` and sets `FirstTimeReading`, ported from Kobo Utilities [source] — `crates/device-kobo/src/maintenance.rs`
- Kobo Utilities' restore sets these with plain `UPDATE content ... WHERE ContentID = ?` [source] — [action.py, older copy of 2024](https://github.com/cbhaley/kobo-utilities/blob/a061a71/KoboUtilities/action.py#L4469)
- `Bookmark` is keyed by `BookmarkID` with indexes on `ContentID` and `VolumeID` [doc, 2024] — [kobo-highlights-to-calibre](https://github.com/valeriangalliat/kobo-highlights-to-calibre/blob/7f04a7a/README.md)

### Inferences

Assessment (ratings are my judgement, not measured):

| Operation | Achievable precision | Device risk | Confidence | Notes |
| --- | --- | --- | --- | --- |
| Read `___PercentRead`, status | Integer percent | None | High | Already done |
| Read position to plain EPUB chapter | Chapter file | None | Medium | Needs `ChapterIDBookmarked` format check |
| Read position to exact span | Sentence | None | Medium | Needs KEPUB file and span table |
| Read highlight into plain EPUB | Exact or near-exact | None | Medium-high | `Text` quote plus span offsets |
| Restore a position the device produced earlier | Same as original | Low-medium | Medium | Kobo Utilities precedent, same file only |
| Synthesise a position from the desktop reader | Sentence at best | Medium | Low | No USB precedent found. Requires exact file's span map |
| Set percent or status only | Coarse | Low | Medium | Kobo Utilities precedent |
| Insert highlights into `Bookmark` | Unknown | Medium-high | Very low | No precedent; unknown sync, id, and rendering expectations |

Specific hazards:

- Replacing a KEPUB after re-conversion can leave stale span ids in `Bookmark`. If book text or the converter version changed, device highlights may point at wrong text or nothing, even if the file path is unchanged. This applies today, regardless of any sync feature.
- Moving or renaming a sideloaded book may drop its rows, per davidfor's 2017 note. Export annotations before such changes.
- Writes while Nickel holds state are unsafe, so USB mode only. A successful SQL update does not prove Nickel will honour the value on the next open.
- The failure mode for a bad position is likely a benign reset to chapter or book start, but a malformed row's effect is unknown. Treat it as possibly a crash or a database rebuild.
- Cloud sync may overwrite values for store books. Sideloads may be unaffected. Neither is verified.

Suggested order: (1) capture raw columns; (2) run a read-only survey of the reference database backup; (3) build the span table and text anchoring; (4) only then consider position write-back behind a setting with a backup, using one test book, verified by hand on hardware.

### Gaps

- No primary evidence on how Nickel validates `ChapterIDBookmarked` or `Bookmark` rows on load.
- No evidence on Kobo cloud sync's interaction with sideloaded reading state.
- Real corruption rates for these writes are unknown. Forum reports of database corruption exist, but none reviewed were tied to these specific writes.

## How do readers compute progress, and are the numbers comparable?

### Takeaway

They are not comparable beyond a rough indication. Each reader uses a different denominator: bytes, positions, layout pages, or Kobo's own opaque value, and KEPUB markup alters byte-based figures further. Use percent for display and only as a last-resort locator.

### Cited Findings

- foliate-js `SectionProgress` weights by section byte size, gives `linear="no"` sections weight 0, and is constructed with 1500 bytes per location and 1600 per time unit. Zip entries use `uncompressedSize` [source] — [progress.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/progress.js#L58-L95); [view.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/view.js#L41)
- Readium-style positions, as Komga implements them, create one position per 1024 bytes of each resource, with `maxOf(1, ceil(size / 1024))` per file [source] — [EpubExtractor.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/infrastructure/mediacontainer/epub/EpubExtractor.kt)
- Komga derives span progression as the span's end position divided by file size, and recomputes total progression itself rather than trust Kobo's [source] — [EpubExtractor.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/infrastructure/mediacontainer/epub/EpubExtractor.kt); [BookLifecycle.kt](https://github.com/gotson/komga/blob/ff800c2/komga/src/main/kotlin/org/gotson/komga/domain/service/BookLifecycle.kt)
- `Bookmark.ChapterProgress` is a chapter fraction, read by calibre as 0-1 [source] — [bookmark.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/bookmark.py)
- `___PercentRead` is stored as a whole-number value in the app's reads, and calibre reports 100 when `ReadStatus` is 2 [source] — `crates/device-kobo/src/db.rs`; [bookmark.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/devices/kobo/bookmark.py)
- KOReader's kosync sends a rendered-document percentage with an xpointer, and BeePub notes percentage scales diverge in footnote-heavy or image-heavy books [source, doc] — [main.lua](https://github.com/koreader/koreader/blob/master/plugins/kosync.koplugin/main.lua); [BeePub](https://github.com/oalieno/beepub/releases/tag/v0.6.0)
- Readest treats a server's percentage as not directly comparable and never lets it pick the section [source] — [kosyncProgress.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/hooks/kosyncProgress.ts)

### Inferences

- A byte-based fraction computed on a KEPUB differs from one on the plain EPUB, because span markup inflates text-heavy files more than others, and the dummy title page adds a resource.
- For a single shared scale, compute `totalProgression` in the app from the plain EPUB's own sizes after resolving a location, rather than storing Kobo's number as if comparable. Keep Kobo's `___PercentRead` as a separate device-reported field. This is what Komga does in spirit.
- `ChapterProgress` is more useful than the book percentage for mapping, because it is scoped to one file. Its precise denominator (pages, bytes, characters) is unknown.

### Gaps

- Nickel's formula for `___PercentRead` and `ChapterProgress` is undocumented, and no source found states it.
- No study quantifies the divergence between these percentages. Komga's "a few pages" is the only figure, and it is qualitative.
