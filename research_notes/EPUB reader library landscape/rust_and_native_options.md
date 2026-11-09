# Rust ecosystem and native (non-webview) options for an in-app EPUB reader

State as of 2026-11-07. Evidence labels used below:

- **[registry]**: fetched today from the crates.io API or GitHub API.
- **[source]**: read in the published crate source or the local repo.
- **[probe]**: run today on this Mac (macOS 27.0.1, build 26A434) with throwaway code in the session scratchpad. Nothing in the repo was touched. The WebKit probe drives a raw `WKWebView` with `WKURLSchemeHandler` from Swift, which is the layer wry wraps. It did not go through wry or Tauri. The sanitiser probe ran `ammonia` 4.2.4 and `lol_html` 3.0.3 on a hand-written EPUB-style XHTML sample. The scratchpad is temporary, so the method is described inline.
- **[secondary]**: a search snippet or a summarising fetch, not read in full at the primary source.
- Items older than 2026 are flagged with their date.

Overall conclusion: use webview rendering, with Rust owning parsing, serving, indexing and search. No native engine is credible for this app today. The most important new finding is that HTML-mode sanitisers are unsafe on XHTML (see the sanitising section), so the security boundary should be a CSP response header, not a rewriter.

## Rust EPUB parsing crates: replace, complement or extend parallax-epub?

### Takeaway

Keep parallax-epub for byte-preserving metadata edits. Don't replace it. For the reader's read-side model, `rbook` is the best fit (Apache-2.0, EPUB 2/3 nav, NCX, landmarks, page-list, lazy resources, same `zip` major version). Its one real gap is font deobfuscation, which boko already implements. Extending parallax-epub alone is simple for manifest and spine, but it stops being simple at navigation documents, because parallax-epub's own code notes that these are often malformed.

### Cited Findings

**Registry data [registry]** (`recent` is crates.io's 90-day count; "updated" is the crates.io `updated_at` field)

| Crate | Version | Updated | Licence | Downloads (total / recent) | GitHub stars / open issues | Notes |
|---|---|---|---|---|---|---|
| [rbook](https://crates.io/crates/rbook) | 0.7.12 | 2026-10-10 | Apache-2.0 | 71,041 / 30,996 | 53 / 0 | Created 2022, active changelog |
| [epub](https://crates.io/crates/epub) (epub-rs) | 2.1.6 | 2026-11-01 | GPL-3.0-or-later | 303,723 / 97,310 | 134 / 6 | Created 2017, the oldest option |
| [boko](https://crates.io/crates/boko) | 0.6.0 | 2026-11-04 | GPL-3.0-or-later | 5,208 / 4,660 | 80 / 6 | Already vendored here |
| [iepub](https://crates.io/crates/iepub) | 1.3.8 | 2026-11-05 | MIT | 27,014 / 2,337 | 11 / 0 | Reads and writes EPUB and MOBI |
| [lib-epub](https://crates.io/crates/lib-epub) | 0.3.3 | 2026-08-27 | MIT | 874 / 275 | 4 / 1 | Created 2025-12 |
| [epub-parser](https://crates.io/crates/epub-parser) | 0.3.4 | 2026-02-07 | MIT | 585,394 / 210,353 | not checked | Download count looks artificial, see below |
| [epub-stream](https://crates.io/crates/epub-stream) | 0.1.0 | 2026-02-22 | MIT | 590 / 298 | not checked | Single release |
| [ebook-rs](https://crates.io/crates/ebook-rs) | 0.18.0 | 2026-10-27 | MIT | 1,974 / 1,341 | 15 / 10 | 61 versions since 2026-08-05 |
| [epub-stack](https://crates.io/crates/epub-stack) | 0.3.0 | 2026-10-06 | Apache-2.0 OR MIT | 164 / 105 | 1 / 1 | Repo created 2026-07 |

- `epub-parser`'s downloads are almost all on one version: 0.3.4 has 585,045, while neighbouring versions have about 21 each. This points to automation rather than real adoption, so don't treat it as popularity. — [crates.io API](https://crates.io/api/v1/crates/epub-parser)

**Feature coverage [source]**. This is grep-level presence checking on the published crate source, not behavioural testing on real books.

| Capability | rbook 0.7.12 | epub 2.1.6 | iepub 1.3.8 | lib-epub 0.3.3 | boko 0.6.0 (vendored) | parallax-epub today |
|---|---|---|---|---|---|---|
| Public manifest | Yes | Yes (`resources` map with mime and properties) | Not checked | Not checked | `list_assets` paths only | No |
| Spine | Yes, with `page_direction` | Yes, with `linear` and `properties` | Not checked | Not checked | Yes, but `SpineEntry` is only `id` and `size_estimate`; `page-progression-direction` is parsed; no `linear` match | Private `Parts`, hrefs only; ignores `linear` |
| EPUB3 nav | Yes | Finds the nav id; no `epub:type` handling at all | `epub:type` matches | 2 `epub:type` matches | Yes (`parse_nav_toc`) | No, only text-counts page-list entries |
| NCX | Yes | Yes (doc comment says `toc.ncx`) | Yes | Yes | Yes, preferred over nav | No |
| Landmarks / guide | Yes | No matches | No matches | No matches | Yes | No |
| page-list | Yes (`TocEntryKind::PageList`) | No matches | No matches | No matches | No matches | Count only |
| Font deobfuscation | No (only doc mentions of `encryption.xml`) | No | No | Yes (55 matches) | Yes, transparent in `load_asset` | No |
| Media overlay | Manifest link only | No | No | No | No matches | No |
| CFI | No | No | No | No | No | No |
| `unsafe` in crate | 0 files | 0 files | 1 file | 0 files | 0 lines | Forbidden by lint |

Sources: [rbook crate](https://docs.rs/crate/rbook/0.7.12), [epub crate](https://docs.rs/crate/epub/2.1.6), [iepub crate](https://docs.rs/crate/iepub/1.3.8), [lib-epub crate](https://docs.rs/crate/lib-epub/0.3.3); boko from `vendor/boko/src/book.rs`, `vendor/boko/src/import/epub.rs`, `vendor/boko/src/import/mod.rs`, `vendor/boko/src/epub/parser.rs`; parallax-epub from `crates/epub/src/lib.rs`, `crates/epub/src/extent.rs`.

#### rbook details [source]

- Parsing is lenient by default: `strict: false` in its parser config, with a public `strict()` option. — [rbook source, src/epub/parser.rs and src/epub.rs](https://docs.rs/crate/rbook/0.7.12/source/src/epub/parser.rs)
- Resources load lazily, `Epub` is `Send + Sync` through the default `threadsafe` feature, and writing sits behind a `write` feature that can be turned off. — [rbook README](https://github.com/DevinSterling/rbook)
- It depends on `zip` 9.0.1 with feature `deflate-flate2-zlib-rs`, plus `quick-xml`, `indexmap`, `percent-encoding` and `thiserror`. The workspace has `zip` 9.4.1. — [rbook Cargo.toml](https://docs.rs/crate/rbook/0.7.12/source/Cargo.toml); `Cargo.lock`
- It is 29,017 lines including the writer, and it is a single-maintainer project with planned MOBI/AZW3 support. — [rbook README](https://github.com/DevinSterling/rbook)

#### epub crate details [source]

- Reading needs `&mut self` (`get_resource(&mut self, ...)`), because the document holds a cursor and archive. It uses `xml-rs` and `regex`. — [epub crate source, src/doc.rs](https://docs.rs/crate/epub/2.1.6/source/src/doc.rs)

**What parallax-epub exposes today [source]** (`crates/epub/src/lib.rs`, `crates/epub/src/extent.rs`)

- Public API: `Epub::{open, metadata, cover, cover_media_type, write_with_metadata, write, extent}`, plus `inspect`, `is_clutter`, `has_drm`, `strip_kobo_markup`.
- The stated design goal is "minimal, byte-preserving edits": OPF edits are text splices and other zip entries are copied without recompression. No crate surveyed advertises this.
- `open_zip`, `read_entry`, `read_entry_bytes` and `resolve_href` are private. `read_entry` is lossy UTF-8. `cover()` and `extent()` reopen the zip on each call, and `doc()` reparses the OPF on each call.
- `Parts` in `extent.rs` is private, keeps only hrefs, filters to `html` media types and ignores `linear`.
- `listed_pages` deliberately uses text search, with the comment "these files are too often not well-formed enough to parse". The author's own library has shown that nav documents can't be assumed to parse.
- `Extent` measurement refuses encrypted books through `has_drm`, which is reusable for the reader's refusal path.

**What boko exposes [source]** (`vendor/boko/src/book.rs`, `vendor/boko/VENDORED.md`)

- `Book::{open, metadata, toc, landmarks, spine, source_id, load_raw, load_chapter, load_asset, list_assets, font_faces, resolve_links, export}`, across EPUB, KFX, AZW3 and MOBI. `Importer` is `Send + Sync`.
- `pub(crate) mod epub`, so the low-level EPUB parser is not public. Only the format-neutral `Book` API is.
- `TocEntry` has `title`, `href`, `children`, `play_order` and a resolved `target`.
- It is used only by `crates/convert` today, with `default-features = false`.
- Minor repo inconsistency: the root `Cargo.toml` comment says "two fixes" and `VENDORED.md` says three.

#### Peer practice

- Readest (26,038 stars) hand-rolls a native EPUB path in Rust from `zip` and `quick-xml` rather than using a crate, per its Cargo comments. — [Readest src-tauri/Cargo.toml](https://github.com/readest/readest/blob/main/apps/readest-app/src-tauri/Cargo.toml)

#### Rejected

- `ebook-rs`: default features pull in `server`, `opds`, `mcp`, `mmap` and `zstd`; 61 releases in about 3 months; one main author (153 commits); 28 lines containing `unsafe`. The feature list reads as very broad for that age, and I'd treat its claims as unproven. — [ebook-rs 0.18.0 source](https://docs.rs/crate/ebook-rs/0.18.0)

### Inferences

- Recommendation (judgement, not tested on real books): add `rbook` with `default-features = false` plus `threadsafe`, wrapped behind a small reader-facing type in parallax-epub or a new crate. Keep parallax-epub's edit path unchanged. Port or reuse boko's font deobfuscation, because publishers' obfuscated fonts will otherwise fail to load.
- Apache-2.0 is GPLv3-compatible, so rbook is licence-safe for a GPL-3.0-only workspace (`Cargo.toml` line 10). `epub` and boko are GPL-3.0-or-later, which is also fine here.
- boko is the tempting zero-new-dependency choice, and it would give MOBI/AZW3/KFX through the same API. But it lacks page-list and `linear`, it has no public manifest, and it is a patched fork whose purpose is conversion. A reader for Amazon formats is better served by converting to EPUB first, which the app already does, than by a second reading path.
- If new dependencies are unwanted, extending parallax-epub is a realistic fallback: promoting `Parts` is small work, and the risk sits in tolerant nav/NCX parsing. Readest's choice shows this is workable.
- `rbook`'s and the workspace's `zip` should unify on 9.x, with extra deflate feature flags. This wasn't compile-tested.

### Gaps

- No crate was run against a real library, so tolerance of malformed nav documents (the failure parallax-epub's comment describes) is unverified for all of them. A differential test of rbook against boko on real books would settle it.
- `iepub` and `lib-epub` manifest and spine API shapes weren't examined, since their low adoption and page-list gaps ruled them out.
- `epub-stream` and `epub-parser` sources weren't inspected.

## Serving book resources to the webview: custom scheme, asset protocol, blob URLs

### Takeaway

Use `register_asynchronous_uri_scheme_protocol` with a `book://<book-id>/...` scheme. It reads straight from the zip, gives each book its own origin, and lets Rust attach a CSP header to each response. The `asset:` protocol can't read inside a zip. Blob URLs work but inherit the app's origin, so a strict CSP becomes the only barrier between book content and the app.

### Cited Findings

#### Probe results on macOS 27.0.1 [probe]

- `tauri://localhost` reports `isSecureContext = true`, and so do `book://` iframes.
- Cross-scheme `<img>` loads (`tauri://` page to `asset://` and `book://`) all worked. The regression in [wry #1778](https://github.com/tauri-apps/wry/issues/1778) did not reproduce on this OS.
- `book://b1` and `book://b2` are distinct origins, so the host part gives per-book isolation.
- `fetch` from `tauri://localhost` to `book://` sends `Origin: tauri://localhost`, and fails with "Load failed" unless the response has `Access-Control-Allow-Origin`. `<img>` loads send no `Origin` and need no CORS header.
- A `Range` header reached the handler, and a `206` came back to JS. `Content-Range` was readable only with `Access-Control-Expose-Headers: Content-Range`; the probe set that header, and a run without it wasn't tried.
- An iframe on `book://` inside a `tauri://` page is cross-origin: `contentDocument` access is blocked, with or without `sandbox`. A reader library in the main page can't touch that iframe's DOM.
- A CSP response header from the scheme handler is enforced on the iframe. `script-src 'none'` stopped both inline `<script>` and `onerror=` attributes.
- A blob iframe created by a `tauri://` page gets origin `tauri://localhost`, and its document is readable by the parent. With no CSP, or with `'unsafe-inline'`, its scripts ran. Under a strict `default-src 'self'; frame-src blob:` policy, inline scripts and `onerror` in the blob did not run, and the parent could still read the document.
- With `application/xhtml+xml`, a malformed chapter shows WebKit's error page ("This page contains the following errors ... Entity 'nbsp' not defined"), not the book's text. The sample had no DOCTYPE, so behaviour with an XHTML 1.1 DOCTYPE wasn't tested.

#### WebKit sandbox bug, with the current state confirmed [probe]

- foliate-js says the `allow-scripts` attribute is required on iframes because of WebKit bug 218086, "which renders iframe sandbox useless", and that CSP must therefore block scripts. — [foliate-js README](https://github.com/johnfactotum/foliate-js)
- The bug is still `NEW`, last changed 2025-02-09 (dated). — [WebKit Bugzilla 218086](https://bugs.webkit.org/show_bug.cgi?id=218086)
- It reproduces today: with `sandbox="allow-same-origin"`, a parent-attached click listener fired 0 times. With `sandbox="allow-same-origin allow-scripts"`, it fired once. [probe]
- The workable pattern was verified: `sandbox="allow-same-origin allow-scripts"` plus a CSP header of `script-src 'none'` on the chapter. The parent's listener fired, parent DOM access worked, and the book's `<script>` and `onerror` did not run. [probe]
- That CSP also appears to stop scripts the parent injects into the chapter: only one injected-script message arrived across three frames, and two of those had the CSP. Reader code should therefore operate on the chapter from outside. (Inferred from the count; messages weren't tagged by frame.)

#### Tauri and wry behaviour [source]

- Project versions are tauri 2.12.5 and wry 0.57.1, with `protocol-asset` enabled. — `Cargo.lock`, `app/src-tauri/Cargo.toml`
- Origin for custom schemes on macOS, iOS and Linux is `<scheme_name>://localhost`; on Windows and Android it is `http://<scheme_name>.localhost`. Servers enforcing CORS must return that exact origin or `*`. — [tauri 2.12.5 src/app.rs docs](https://docs.rs/tauri/2.12.5/tauri/struct.Builder.html#method.register_asynchronous_uri_scheme_protocol)
- wry's responder takes a complete `HttpResponse<Cow<'static, [u8]>>`, so a response body is supplied whole. There is no streaming API on this path. — [wry 0.57.1 url_scheme_handler.rs](https://docs.rs/crate/wry/0.57.1/source/src/wkwebview/class/url_scheme_handler.rs)
- wry validates the task before responding and returns early if it is invalid, while its own code carries "FIXME ... not guaranteed to be valid" comments on the retained task. — same file
- Tauri's asset handler caps each range response at `MAX_LEN = 1000 * 1024` bytes, sets `Access-Control-Allow-Origin` to the window origin, and now runs off the webview thread. — [tauri asset.rs](https://github.com/tauri-apps/tauri/blob/dev/crates/tauri/src/protocol/asset.rs), and the same code in the local tauri-2.12.5 source; [PR #15836](https://github.com/tauri-apps/tauri/pull/15836)
- The `asset:` protocol needs the file in scope and the protocol in the CSP. — [Tauri core API docs](https://v2.tauri.app/reference/javascript/api/namespacecore/)
- Tauri's CSP header is attached in `protocol/tauri.rs` for its own assets. — [tauri 2.12.5 protocol/tauri.rs](https://docs.rs/crate/tauri/2.12.5/source/src/protocol/tauri.rs)
- Tauri's invoke initialisation scripts are registered as main-frame scripts. — [tauri 2.12.5 manager/webview.rs](https://docs.rs/crate/tauri/2.12.5/source/src/manager/webview.rs)
- The app's current CSP is `default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' asset: http://asset.localhost data: ...`, with asset scope on `$APPDATA/library/books/**`, and `devUrl` is `http://localhost:1420`. — `app/src-tauri/tauri.conf.json`

#### Known issues (all fetched today through the GitHub API) [registry]

| Issue | State | Relevance |
|---|---|---|
| [wry #1775](https://github.com/tauri-apps/wry/issues/1775) | Open, 2026-07 | Responding off the main thread blocks that thread on an unbounded wait for the main run loop. A bounded pool can stall if the main thread is busy |
| [wry #1856](https://github.com/tauri-apps/wry/pull/1856) | Open PR, updated 2026-10-29 | Fix for the above and for a `stopURLSchemeTask` race. Not merged |
| [tauri #9177](https://github.com/tauri-apps/tauri/issues/9177) | Open, 2024; last updated 2025-08 (dated) | macOS app crash when a custom-protocol request is aborted before or during the response. Relevant to fast page turning. wry's later validation may reduce it, but the issue isn't closed and #1856 still cites a race |
| [wry #1778](https://github.com/tauri-apps/wry/issues/1778) | Open, 2026-07 | macOS 26.6 beta 5 blocked cross-scheme subresource loads, in raw WKWebView. No comments. Not reproduced on 27.0.1 [probe] |
| [tauri #12767](https://github.com/tauri-apps/tauri/issues/12767) | Open, 2025-02 (dated) | HTML in iframes from custom schemes fails on Windows; the reporter says macOS doesn't have the problem. Relevant if the app goes cross-platform |
| [tauri #11505](https://github.com/tauri-apps/tauri/issues/11505) | Open, 2024-10 (dated) | Windows iframe connection refused on custom schemes |
| [tauri #16201](https://github.com/tauri-apps/tauri/issues/16201) | Open, 2026-10 | Asset URLs broken in dev under 2.12 ("Unsafe attempt to load URL asset://...") from `http://localhost:1420`. Reported on Linux; production build fine |
| [tauri #4133](https://github.com/tauri-apps/tauri/issues/4133) | Open, 2022; updated 2026-01 | No true streaming. Whole-file read when no `Range` header is sent |
| [tauri #15837](https://github.com/tauri-apps/tauri/issues/15837) | Closed 2026-08 | Asset protocol multi-range bugs, fixed by #15838 |
| [tauri #11371](https://github.com/tauri-apps/tauri/issues/11371) | Closed, 2024 (dated) | `content-range` hidden from `fetch`. Consistent with the need for an expose header [probe] |
| [tauri #16479](https://github.com/tauri-apps/tauri/issues/16479) | Open, 2026-11-07 | Range failures past byte 0 on the CEF runtime. Doesn't affect wry today, but watch it if Tauri 3 is adopted |
| [tauri #16442](https://github.com/tauri-apps/tauri/issues/16442) | Open, 2026-11-01 | CSP hashes can exceed WebKit's 16 KiB header limit. Keep per-chapter CSP headers short |

#### Other

- Tauri 3 alpha exists (3.0.0-alpha.6, 2026-11-06), alongside the stable 2.12.5 line (2026-10-21). — [crates.io API](https://crates.io/api/v1/crates/tauri)
- Readest and Sard, both Tauri readers, matched GitHub code searches for URI scheme protocol registration in their `src-tauri/src/lib.rs`. I didn't read how they use it. — [Readest](https://github.com/readest/readest), [Sard](https://github.com/Limitless-Soul1/Sard)

### Inferences

- `asset:` can't serve zip members, so using it would mean unpacking every opened book into a cache. That costs disk and cleanup and gains nothing for small resources.
- Whole-body responses are not a limit for EPUB: chapters, CSS, images and fonts are small. Only embedded audio or video could need ranges, and deflated zip entries aren't seekable, so those would need a decompressed cache file.
- Three designs follow from the probes:
  - **A. Blob iframes in the main page**: simplest, and it's what foliate-js does by default, but one CSP mistake exposes the app, since `parent.__TAURI_INTERNALS__` would be same-origin reachable. No per-book origin.
  - **B. Reader host document on `book://<id>/`**: a nested iframe holds the reader JS, chapters are same-origin to it, and each chapter has `script-src 'none'`. The host talks to React by `postMessage`. This keeps book content off the app origin and gives a unique origin per book, at the cost of a more complex bridge.
  - **C. Direct `book://` iframe in the main page**: rejected, because the parent can't reach the DOM.
- I'd pick B if the chosen JS reader tolerates it, otherwise A with a strict CSP. Foliate's internal blob URLs would inherit the `book://<id>` origin under B, which is an inference based on the blob result above.
- The app CSP will need `frame-src` (and `connect-src` if fetching) for `book:`. The `devUrl` origin differs from production, so CORS replies should echo the actual `Origin`, or use `*`, in both modes.
- Handlers should do blocking work on a thread pool that can't be starved by a busy main thread, and tolerate cancelled requests. Keep one open archive per book with a small LRU, not parallax-epub's reopen-per-call pattern.
- Custom-scheme responses appear not to get Tauri's CSP automatically, because the header is attached in Tauri's own protocol. The handler must set it.

### Gaps

- No latency or throughput benchmark was run, and I found no published one for custom schemes on macOS.
- The probe used raw `WKWebView`, so Tauri-level behaviour wasn't tested: IPC availability inside `book://` frames, how capability checks treat that origin, and dev-mode loads from `http://localhost:1420` on macOS.
- A `Content-Range` read without the expose header wasn't tried. Video seeking wasn't tested.
- Whether #9177 still reproduces on wry 0.57.1 wasn't tested.
- The "unique origin" rule below comes from a summarising fetch of the spec, so confirm the wording before quoting it.

## Full-text search and HTML text extraction

### Takeaway

Use a plain per-book scan for in-book search, and SQLite FTS5 for cross-library search. FTS5 is already compiled into the workspace's bundled SQLite, so it adds no dependency. Move to `tantivy` only if multilingual stemming, fuzzy matching or better ranking become requirements. `milli` isn't a practical library dependency. Extract text with an XML-aware path first, because HTML parsers can lose an entire chapter on valid XHTML.

### Cited Findings

#### Options

| Option | Version / updated | Licence | Downloads (total / recent) |
|---|---|---|---|
| [tantivy](https://crates.io/crates/tantivy) | 0.27.0, 2026-10-17 | MIT | 21.8M / 5.3M |
| [rusqlite](https://crates.io/crates/rusqlite) | 0.41.1, 2026-10-28 | MIT | 147.8M / 57.4M |
| [milli](https://crates.io/crates/milli) | 0.1.0, 2020-07-15 (dated) | MIT | 2,134 / 156 |
| [charabia](https://crates.io/crates/charabia) | 0.10.0, 2026-08-13 | MIT | 1.4M / 292K |
| [jieba-rs](https://crates.io/crates/jieba-rs) | 0.11.0, 2026-09-16 | MIT | 5.3M / 2.1M |

[registry]

- FTS5 is enabled: `libsqlite3-sys` 0.39.1's `build.rs` passes `-DSQLITE_ENABLE_FTS5` for bundled builds, and the bundled SQLite is 3.54.1. The workspace uses `rusqlite` 0.41 with `bundled`. [source] — `Cargo.toml`, [libsqlite3-sys](https://crates.io/crates/libsqlite3-sys)
- FTS5 tokenizers: `unicode61` (default), `ascii`, `porter` (English stemming), `trigram` (substring matching). Trigram queries under 3 characters match nothing. — [SQLite FTS5](https://sqlite.org/fts5.html)
- Contentless-delete tables (SQLite 3.43+) support `DELETE`, which suits an index without duplicate text. External-content tables need manual sync. `snippet()` returns up to 64 tokens, and `bm25()` is built in. — [SQLite FTS5](https://sqlite.org/fts5.html) [secondary, summarising fetch of the primary page]
- Trigram indexes were reported at about 1.5 to 2 times raw text size, from one developer's corpus. — [DEV Community](https://dev.to/omochi_dev/why-sqlite-fts5s-default-tokenizer-drops-your-japanese-substrings-and-the-one-line-fix-1k2d) [secondary]
- A Korean benchmark found weak trigram recall on short queries (recall@5 of 0.2857 against 1.0 for `LIKE`), but on only 40 documents. — [llm-kernel docs](https://docs.rs/crate/llm-kernel/latest/source/docs/benchmarks/korean-recall.md) [secondary, narrow test]
- No FTS5 custom-tokenizer wrapper shows up in rusqlite's module listing (`vtab` exists, but no fts tokenizer module). [source] — [rusqlite 0.41.1](https://docs.rs/crate/rusqlite/0.41.1)
- tantivy: stemming for 17 Latin-script languages, CJK only through third-party crates (tantivy-jieba, cang-jie, lindera, Vaporetto), mmap directory, incremental indexing, documents immutable (delete and reindex), commit needed for visibility, claimed startup under 10 ms. — [tantivy README](https://raw.githubusercontent.com/quickwit-oss/tantivy/main/README.md)
- `milli` on crates.io is a 2020 placeholder. The current code is `publish = false` inside the Meilisearch repo, whose licence file says parts fall under the Business Source License 1.1 Enterprise Edition. — [milli Cargo.toml](https://github.com/meilisearch/meilisearch/blob/main/crates/milli/Cargo.toml), [Meilisearch LICENSE](https://github.com/meilisearch/meilisearch/blob/main/LICENSE)
- The library schema has no FTS tables, and has `reading`, `annotations` and `reading_stats` tables, with annotations keyed by `device_serial` and `bookmark_id` and storing a `REAL` position. [source] — `crates/library/src/schema.rs`

#### Text extraction

| Crate | Version / updated | Licence | Downloads (total / recent) |
|---|---|---|---|
| [html5ever](https://crates.io/crates/html5ever) | 0.40.1, 2026-09-14 | MIT OR Apache-2.0 | 144M / 54.8M |
| [scraper](https://crates.io/crates/scraper) | 0.29.0, 2026-10-21 | ISC | 35.8M / 10.8M |
| [lol_html](https://crates.io/crates/lol_html) | 3.0.3, 2026-10-28 | BSD-3-Clause | 7.3M / 2.4M |
| [html2text](https://crates.io/crates/html2text) | 0.17.4, 2026-10-18 | MIT | 7.1M / 1.8M |

[registry]

- A self-closed `<title/>` makes HTML-mode parsers treat the rest of the document as title text: `lol_html` saw no elements after it, and `ammonia` (built on html5ever) escaped the entire chapter into text. [probe] See the next section.
- parallax-epub already has a hand-written scanner (`scan()` in `crates/epub/src/extent.rs`). It skips to `<body`, handles comments, `script` and `style`, and counts CJK separately. [source]

### Inferences

- Scale estimate (assumption, not measured): about 0.5 MB of text per book gives about 2.5 GB of raw text for 5,000 books. FTS5 handles this, but it's large enough to keep in a separate attached database, not inside the library DB and its backups.
- In-book search needs no index. Scanning about 0.5 MB takes milliseconds, and the harder problem is mapping a hit to a DOM range, so doing it in the webview through the JS reader may be simpler. The Rust path only pays off for background or cross-book search.
- For cross-library search, use FTS5 `unicode61` with `remove_diacritics`. For CJK, pre-segment in Rust with `charabia` or `jieba-rs` before inserting, which avoids a custom tokenizer, and with it the FFI that `unsafe_code = "forbid"` would rule out in workspace crates.
- Index asynchronously and resumably. A library of thousands of books shouldn't be processed in one blocking pass. Store `(book_id, spine_index, text)` rows, so hits land on a chapter.
- For extraction: strict XML parse first (`quick-xml` or `roxmltree`), then fall back to `html5ever` only for documents that fail XML parsing. A `<title/>` guard matters less for the fallback, since well-formed documents never reach it.
- tantivy's strengths (per-language stemmers, fuzzy search, faceting) are real, but they cost a second storage system to keep in sync with SQLite.

### Gaps

- No head-to-head FTS5 against tantivy benchmark on book-sized corpora was found, and none was run. Index-size numbers for tantivy weren't found.
- The real library's average text size wasn't measured, so the sizing above is an assumption.
- `rusqlite` tokenizer support was checked by module listing only, not by reading every feature flag.

## HTML sanitising and rewriting for EPUB content

### Takeaway

Yes, sanitising breaks valid EPUB content, badly if the tool is an HTML-mode one. `ammonia` with defaults destroys styling, IDs, SVG and MathML, and one self-closed tag can turn a chapter into escaped text. `lol_html` can silently pass everything through or truncate the chapter. Use a CSP header as the security boundary, and make any rewriting XML-aware. Use `ammonia` and `lol_html` only on content already known to be HTML.

### Cited Findings

**Probe on a hand-written EPUB-style XHTML sample [probe]** (`ammonia` 4.2.4, `lol_html` 3.0.3)

| Input | `lol_html` (remove `script,iframe,object,embed` and `on*` attributes) | `ammonia::clean` (defaults) |
|---|---|---|
| `<title/>` in head (valid XHTML) | **Nothing stripped.** `<script>`, `onclick`, `<iframe>` and `javascript:` all survive, because the tokeniser sees them as title text. In an XML parse these are live, so this is a sanitiser bypass | **Whole document escaped** into visible text (`&lt;p ...`) |
| `<script src="a.js"/>` (valid XHTML) | **Output truncated at that point:** body gone, because `el.remove()` removed everything up to a closing tag that never came | Not run separately |
| No self-closed raw-text elements | Works, and is byte-preserving elsewhere; keeps `<?xml?>`, namespaces, `epub:type`, `id`, `class`. `href="javascript:"` isn't removed unless handled | Removes `<head>` (title text leaks into body as bare text), `<link>`, `<style>`, `id`, `class`, `style`, `epub:type`, all of `<section>`, `<svg>` and `<math>`; `<a href="javascript:">` is defanged |
| `<a id="pg12" epub:type="pagebreak"/>` (common EPUB page marker) | Unchanged | HTML parsing makes it an open anchor, so **the following content is wrapped inside `<a>`** |

- `ammonia` with a widened allow-list kept `section`, `svg`, `math`, `id`, `class`, `epub:type` and `style`, but still flattened `<head>` and still nested content in the unclosed anchor, and `<link>` needs its attributes allowed explicitly. [probe]
- Source explanation: `ammonia` uses `html::parse_fragment`, so document structure isn't kept. Its defaults are `clean_content_tags` of `script` and `style`, `generic_attributes` of `lang` and `title`, and a comment that it "disallows `<svg>`". — [ammonia 4.2.4 src/lib.rs](https://docs.rs/crate/ammonia/4.2.4/source/src/lib.rs)
- `lol_html` documents that "the self-closing syntax has no effect in HTML content". — [lol_html 3.0.3 element.rs](https://docs.rs/crate/lol_html/3.0.3/source/src/rewritable_units/element.rs)

**Spec context** (via a summarising fetch of the current spec) [secondary]

- Scripting support isn't required, and if unsupported, fallbacks must be processed. — [EPUB Reading Systems 3.3](https://www.w3.org/TR/epub-rs-33/)
- "Reading systems should treat all scripts as untrusted (and potentially malicious)". — [EPUB RS 3.3 §6.4.3](https://www.w3.org/TR/epub-rs-33/)
- Each publication instance must have a unique origin (§4.1.1), and `file:` URL access and top-level `data:` URLs must be prevented. — [EPUB RS 3.3](https://www.w3.org/TR/epub-rs-33/)
- Reading systems must use a non-validating XML processor for XHTML, must support SVG content, and must render Presentation MathML given a viewport. — [EPUB RS 3.3](https://www.w3.org/TR/epub-rs-33/)
- Remote resources are a privacy and security matter: https only, and network access should be user-controllable. — [EPUB RS 3.3 §3.3, §3.8](https://www.w3.org/TR/epub-rs-33/)

#### Practice

- foliate-js doesn't support scripted content, and insists on CSP: "Do NOT use this library (or any other e-book library, for that matter) without CSP". — [foliate-js README](https://github.com/johnfactotum/foliate-js)
- `epub-stack` depends on `lol_html`, but I didn't check how it handles XHTML. [source] — [epub-stack 0.3.0](https://docs.rs/crate/epub-stack/0.3.0)

### Inferences

- The XHTML-parsed-as-HTML mismatch is a parser differential: whatever an HTML sanitiser checks is not what WebKit parses under `application/xhtml+xml`. Sanitising as HTML and serving as XHTML is therefore worse than doing nothing, since it looks safe.
- Recommended pipeline per chapter:
  - Serve bytes unchanged with a CSP header of `script-src 'none'; object-src 'none'; base-uri 'none'`, and restrict `default-src` to the book's own origin, which also blocks remote tracking pixels.
  - Well-formed XHTML goes out as `application/xhtml+xml`.
  - Documents that fail XML parsing go through `html5ever`, are reserialised, and go out as `text/html`, avoiding WebKit's XML error page. Well-formedness has to be checked in Rust first.
- Unchanged bytes also protect CFI stability, covered in the CFI section.
- Links with `javascript:` hrefs are covered by the CSP (`script-src 'none'`), but the reader should still intercept link clicks, opening external links in the system browser with consent.
- CSS isn't sanitised by this approach. CSP limits where CSS can load from, but exfiltration through remote `url()` needs the same-origin `default-src` restriction above.
- If a stripping step is still wanted as defence in depth, an XML event-based pass with `quick-xml` is the safe shape, not `lol_html` or `ammonia`.

### Gaps

- The sample was hand-written to cover known patterns. Frequency of `<title/>`, `<script/>` and `<a/>` in real books wasn't measured. Scanning the user's library would give real numbers.
- Whether `text/html` fallback rendering differs visibly from XML rendering in real books wasn't tested.
- CSP `script-src 'none'` against `<svg>` inline script and SMIL wasn't specifically tested.
- Fallback content for scripted documents (`properties="scripted"`) isn't handled by simply blocking scripts, and I didn't look at how often this matters.

## Native rendering alternatives

### Takeaway

Webview rendering is the clear choice. Every native option fails on at least one hard requirement: pagination and CSS coverage (Blitz, Servo defaults), licence or fidelity (MuPDF, crengine), or the platform (Readium). Readium isn't actually native rendering, since it uses WKWebView too. Revisit Servo or Blitz in a year or more, not now.

### Cited Findings

| Engine | Licence | State | Blocking issue |
|---|---|---|---|
| MuPDF ([`mupdf` crate](https://crates.io/crates/mupdf) 0.8.0, 2026-06-22) | AGPL-3.0 or commercial | Mature, active. 1.78M downloads | Page rasteriser with a limited CSS engine, and AGPL for the combined work |
| crengine ([koreader/crengine](https://github.com/koreader/crengine)) | GPL, version wording unclear | Active (pushed 2026-11-04), 102 stars | C++, no Rust bindings, licence compatibility risk |
| [Readium Swift toolkit](https://github.com/readium/swift-toolkit) | BSD-3-Clause | 3.11.1, 2026-10-16 | iOS only, and uses a webview anyway |
| [Servo](https://crates.io/crates/servo) | MPL-2.0 | 0.8.0, released 2026-11-07 | Pre-1.0, separate engine, columns off by default |
| [Blitz](https://github.com/DioxusLabs/blitz) | MIT OR Apache-2.0 | 0.3.0-beta.4, 2026-10-16 | No multi-column, no fragmentation |
| [Plato](https://github.com/baskerville/plato)'s built-in renderer | GitHub reports NOASSERTION | Last pushed 2026-03-30 | Built for e-ink, not a library |
| [Taffy](https://crates.io/crates/taffy) / [Parley](https://crates.io/crates/parley) / [Stylo](https://crates.io/crates/stylo) | MIT; Apache-2.0 OR MIT; MPL-2.0 | Active | Building blocks, not an engine |

[registry]

#### MuPDF

- Most Artifex products, MuPDF included, are dual-licensed AGPLv3 or commercial. "If you can't meet the requirements of the GNU AGPLv3 above, a commercial license is required." — [Artifex licensing](https://artifex.com/licensing)
- EPUB must be laid out for a target page size, font size and CSS. — [MuPDF Explored](https://ghostscript.com/~robin/mupdf_explored/mupdf_exploredse26.html) [secondary]
- CSS issues on record, all dated: ignored `width` on a cover image in 1.7 (2015), an open umbrella bug from 2015, and a forum review reporting problems with relative values and SVG cover wrappers. — [Bug 695941](https://bugs.ghostscript.com/show_bug.cgi?id=695941), [Bug 695935](https://bugs.ghostscript.com/show_bug.cgi?id=695935), [MobileRead](https://www.mobileread.com/forums/showthread.php?p=3783761) [secondary, dated; a 2019 commit addressed image sizing]
- CVE-2025-71382: unbounded recursion in the EPUB CSS engine before 1.27.0-rc1 can crash the host on crafted input. — [Ubuntu CVE-2025-71382](https://ubuntu.com/security/CVE-2025-71382) [secondary]

#### crengine

- Its README says "GNU GPL license, version 2", while file headers say only "GNU General Public License ... See LICENSE file", and there is no LICENSE or COPYING file at the repo root. [source] — [crengine README.TXT](https://github.com/koreader/crengine/blob/master/README.TXT), [lvtinydom.cpp](https://github.com/koreader/crengine/blob/master/crengine/src/lvtinydom.cpp)
- KOReader, which ships it, is AGPL-3.0 per GitHub. — [KOReader](https://github.com/koreader/koreader)
- No `crengine-sys` crate exists on crates.io. [registry]
- A 2018 user called its CSS "average", and 2020 notes show CoolReader lagging KOReader on CSS. — [MobileRead](https://www.mobileread.com/forums/showthread.php?p=3725781) [secondary, dated]

#### Readium

- Maintainers say macOS support and a native SwiftUI navigator are long-term, and that the navigator is the last piece after v4. A draft cross-platform view relies on `WKWebView`. — [swift-toolkit discussion #817](https://github.com/readium/swift-toolkit/discussions/817) [secondary]
- No Rust port of Readium exists: search found only LCP decryption projects. — [GitHub search results: lcp-rs](https://github.com/duralumind/lcp-rs), [readium-rs](https://github.com/pawanjay176/readium-rs)

#### Servo

- First crates.io release was v0.1.0 on 2026-04-13, with monthly feature releases that may break the API, and six-monthly LTS branches getting security fixes only. — [Servo blog](https://servo.org/blog/2026/04/13/servo-0.1.0-release/)
- Releases since: 0.2.0 on 2026-06-05 through 0.8.0 on 2026-11-07, with 0.1.x maintenance alongside. [registry] — [crates.io API](https://crates.io/api/v1/crates/servo)
- `layout_css_columns_enabled` and `layout_css_writing_mode_enabled` both default to `false`. [source] — [servo prefs.rs](https://github.com/servo/servo/blob/main/components/config/prefs.rs)
- Verso is archived (last push 2025-10-08). `tauri-runtime-verso` isn't archived, last pushed 2026-09-10. [registry] — [verso](https://github.com/versotile-org/verso), [tauri-runtime-verso](https://github.com/versotile-org/tauri-runtime-verso)

#### Blitz / Dioxus Native

- README: "currently in a **beta** state", with "many bugs and missing features". — [Blitz README](https://github.com/DioxusLabs/blitz)
- `column-count`, `column-width` and all other multi-column properties are unsupported. `break-before`, `break-after` and `break-inside` are unsupported. `writing-mode` is partial (block but not inline). `display: table` is partial. SVG renders, but `fill` and `stroke` styling is unsupported. Ruby and MathML aren't listed. — [Blitz CSS status](https://blitz.is/status/css)
- Text selection work is still arriving (open issues #1039, #1050, #1184, #1285). — [Blitz issues](https://github.com/DioxusLabs/blitz/issues)
- Dioxus 0.8 is still alpha (0.8.0-alpha.3, 2026-10-16). [registry] — [dioxus-native](https://crates.io/crates/dioxus-native)

#### Plato and others

- Plato's README says "ePub through a built-in renderer", with `css.rs`, `dom.rs`, `engine.rs`, `layout.rs`, `parse.rs`, `style.rs` and `xml.rs` under `crates/core/src/document/html`. — [Plato](https://github.com/baskerville/plato)
- [`litehtml`](https://crates.io/crates/litehtml) 0.2.6 (MIT, 2026-07) exists, and I didn't inspect it.

### Inferences

- Reader essentials are selection, annotations, reflow, pagination, vertical and RTL text, ruby, MathML, SVG and fonts. WebKit provides all of these already in `WKWebView`, and the app already ships it, so a native engine adds binary size and removes capability.
- The usual webview pagination approach uses CSS multi-column layout. That's unavailable in Blitz and off by default in Servo. Servo's flag could be enabled, but its quality is untested.
- Servo in Tauri would mean a second browser engine in the app, or replacing the runtime through an integration whose upstream is archived. A monthly API-breaking cadence is also a poor fit for a solo project.
- MuPDF licence analysis: GPLv3 section 13 lets GPLv3 code be combined with AGPLv3 code, each part keeping its own licence. This is from my own knowledge, since gnu.org returned HTTP 429 on all three attempts. For a desktop app this is workable, but it adds AGPL duties to the combined distribution. Its real problems are technical: it renders pages to images, so selection, annotation and theming would all need rebuilding. It remains a reasonable candidate for PDF support only, which is a separate decision.
- crengine licence risk: this workspace is `GPL-3.0-only`. If crengine is GPLv2-only, combining them is incompatible. KOReader's AGPL-3.0 distribution suggests "or later" terms in practice, but I couldn't confirm that. It would also need a custom C++ FFI layer, outside `unsafe_code = "forbid"` crates.
- `unsafe_code = "forbid"` applies to workspace crates, not dependencies, so wrapper crates such as `mupdf` don't violate it. Hand-written FFI to crengine or Swift would.
- Taffy, Parley and Stylo together are essentially what Blitz is built on, so building a custom engine from them means re-doing Blitz's work.

### Gaps

- Servo binary size, memory use and multi-column quality weren't measured. I found no official embedder numbers.
- MuPDF's current CSS coverage isn't documented in any list I could find. The evidence is old, and the official supported-files page returned 404.
- crengine's exact licence version couldn't be established.
- Plato's licence (GitHub reports NOASSERTION) wasn't resolved.
- The Servo book's embedding page returned 404, so the API details come from the blog and the source.

## Rust implementations of EPUB CFI

### Takeaway

Yes, but nothing established. `epub-stack` has the most spec-shaped implementation, and `ebook-rs` has a simpler and weaker one. Neither has meaningful adoption. CFI generation and resolution belong in the webview against the live DOM, so Rust only needs parse, compare and sort, which is small enough to write or to take from `epub-stack` after review.

### Cited Findings

- `epub-stack` 0.3.0 (Apache-2.0 OR MIT): 2,648 lines across `src/cfi/mod.rs`, `src/cfi/resolution.rs` and `src/publication/cfi.rs`, with typed `Cfi`, `CfiRange`, `CfiPath`, `Step`, `Offset`, `RedirectedPath` and assertions, and 14 unit tests in `mod.rs`. README says it's modelled on EPUB 3.4. Dependencies include `symphonia`, `jxl-oxide`, `xot` and `lol_html`, and I didn't check which are optional. 164 downloads, 1 star, repo created 2026-07-26, last push 2026-11-04. [source] [registry] — [epub-stack 0.3.0](https://docs.rs/crate/epub-stack/0.3.0)
- `ebook-rs` 0.18.0 (MIT): 434-line `cfi.rs` with `parse`, `from_spine_index`, `compare`, `resolve_dom_path` and `to_cfi_string`. `compare` looks only at spine index and character offset, which isn't full CFI ordering. 2 tests. [source] — [ebook-rs cfi.rs](https://docs.rs/crate/ebook-rs/0.16.4/source/src/cfi.rs)
- `twn39/epub-rs` (MIT): keyword `cfi`, 0 stars, last push 2026-08-22, and its claims rest on its own README. The name `epub-rs` isn't on crates.io (lookup of the normalised name returned not found). [registry] — [twn39/epub-rs](https://github.com/twn39/epub-rs)
- `orhnk/epubcfi.rs`: "EpubCFI generator for rust", 0 stars, last push 2024-11-21 (dated). [registry] — [orhnk/epubcfi.rs](https://github.com/orhnk/epubcfi.rs)
- The crate names `epub-cfi`, `epubcfi` and `cfi` are not on crates.io. [registry]
- `rbook`, `epub`, `iepub`, `lib-epub`, boko, parallax-epub and the app have no CFI code. [source]
- foliate-js's `epubcfi.js` can be used "in any environment if you only need to parse or sort CFIs". — [foliate-js README](https://github.com/johnfactotum/foliate-js)
- The current `annotations` table stores a `REAL` position plus device serial and bookmark ID, not a CFI. [source] — `crates/library/src/schema.rs`

### Inferences

- CFI steps count element positions, so they depend on the exact DOM. Rewriting a chapter (removing `<script>`, or HTML reparsing that changes nesting as in the `<a/>` case) moves steps and invalidates stored CFIs. This is another reason to serve unmodified bytes.
- Generate CFIs in JS from the live DOM, store them as opaque strings, and sort in JS or with a small Rust comparator. Rust needs real CFI resolution only for jobs like mapping search hits or Kobo bookmarks to CFIs without a webview. If that happens, it must parse chapters the same way the webview does (XML), or positions will disagree.
- `epub-stack` is a useful reference, and possibly a dependency if its features can be trimmed. Adopting a 1-star crate for a persistent data format needs a code review first.

### Gaps

- Neither Rust implementation was tested against foliate-js or the [CFI spec](https://idpf.org/epub/linking/cfi/) test cases, so conformance is unverified.
- `epub-stack`'s optional feature layout wasn't checked.
- The `ebook-rs` citation link points at 0.16.4, while the line counts are from the 0.18.0 download.
