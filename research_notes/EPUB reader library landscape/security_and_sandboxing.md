# Security and sandboxing for untrusted EPUB content in Tauri 2 on WKWebView

State as of 2026-11-07. Evidence labels used throughout:

- **Source-verified**: read in the pinned source linked next to the claim. Pins: tauri `tauri-v2.12.5` and wry `wry-v0.57.1` (the exact versions in the project's `app/src-tauri/Cargo.lock`), WebKit `main` at `ebd4c5dee7` (2026-11-07), foliate-js `78914ae`, Readest `5a105f4`, Thorium `b00cee5`, Readium ts-toolkit `561ced5`, calibre `2abf07c`, Foliate `67b6676`, epub.js `eee359d`.
- **WKWebView-tested**: run on this Mac (macOS 27.0.1 build 26A434, WebKit.framework 22625.1.29.11.28, Safari 27.0.1) with a small benign Swift harness. It reports what a frame can see and calls nothing privileged. It copies wry's setup: `WKURLSchemeHandler` for custom schemes, a script message handler named `ipc`, and main-frame-only `WKUserScript`s. It uses a fake internals object and a local 127.0.0.1 HTTP server that logs hits. **It is not a Tauri app.** Results for WebKit behaviour are direct; anything about Tauri's own handling combines the test with source reading. The harness and raw logs are in the session scratchpad (`/private/tmp/claude-501/-Users-jacksonferguson--claude-spares-spare-Lr1qYW/80db6223-1ba3-4aeb-a863-26441237fe52/scratchpad/harness/`: `h.swift`, `run1.txt`, `hits1.log`, `out2.txt`), which is temporary. Note that `h.swift` was last edited for the second run, so `run1.txt` came from an earlier revision of it.
- **Inference**: my reasoning from the above, listed under Inferences.

## Threat model for this app: what does a malicious EPUB get if script runs?

### Takeaway

The dangerous path is short and has already been exploited on macOS in a Tauri reader: book script in a same-origin iframe calls `parent.__TAURI_INTERNALS__.invoke`. In this project, that would reach all 61 app commands, including Kobo database writes, because app commands are not ACL-checked today. Treat a book as hostile, remote-origin content, and treat the Kobo as the asset most worth protecting.

### Cited Findings

- Closest precedent: Readest CVE-2026-82642 (CVSS 8.8, fixed in 0.11.16). DOMPurify did not inspect `iframe srcdoc`, the content iframe had `sandbox="allow-same-origin allow-scripts"`, and script reached `parent.parent.__TAURI_INTERNALS__.invoke(...)`. The PoC was tested on macOS — [GHSA-p4x7-pf2c-xrvj](https://github.com/readest/readest/security/advisories/GHSA-p4x7-pf2c-xrvj); [CVE-2026-82642](https://api.osv.dev/v1/vulns/CVE-2026-82642).
- Readest's privilege exposure made this worse: unscoped `download_file`/`upload_file` commands reachable from `reader-*` windows gave arbitrary file read and write, though the advisory rates book-JS reachability at "Medium" confidence — [GHSA-55vr-pvq5-6fmg](https://github.com/readest/readest/security/advisories/GHSA-55vr-pvq5-6fmg). The PDF.js path escalated the same way — [GHSA-pprj-f382-v52p](https://github.com/readest/readest/security/advisories/GHSA-pprj-f382-v52p).
- Project state (source-verified): `app/src-tauri/build.rs` is a bare `tauri_build::build()`, 61 `#[tauri::command]`s are registered in `app/src-tauri/src/lib.rs` (including `device_restore`, `remove_from_device`, `send_books`, `tidy_device`, `reconcile_device`, `library_remove`, `library_restore`, `setting_set`, `rules_apply`, `eject_device`, `cover_set_url`), and `app/src-tauri/capabilities/default.json` grants only `core:default` plus three dialog permissions to `main`.
- Without an app manifest, app commands skip the ACL when the origin is local. The rejection branch only runs on `plugin_command.is_some() || has_app_acl_manifest || !is_local` — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2076-L2083). `has_app_manifest` is only true if the build declares app permissions — [tauri-build acl.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri-build/src/acl.rs#L416-L426).
- The asset protocol lets same-origin script read files in scope: it sends `Access-Control-Allow-Origin: <window origin>` — [protocol/asset.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/protocol/asset.rs#L13-L43). WKWebView-tested: a same-origin blob frame under a permissive CSP fetched a stand-in `asset://` URL with status 200, while a `book://b1` frame failed on CORS.
- Attack classes demonstrated in the wild or in research are in the next section. Also relevant to this codebase's parsers, not the renderer: path traversal from `META-INF/encryption.xml` in calibre — [CVE-2026-25636](https://github.com/kovidgoyal/calibre/security/advisories/GHSA-8r26-m7j5-hm29); symlinks embedded in the ZIP — [Franken et al. 2021](https://lirias.kuleuven.be/retrieve/616428); code execution from book metadata reaching a template engine — [CVE-2026-53511](https://github.com/kovidgoyal/calibre/security/advisories/GHSA-2j4m-2q7x-2c47).
- A privileged helper can undo a good sandbox: calibre's viewer `background_image` endpoint fetched arbitrary URLs for sandboxed book script, enabling exfiltration — [CVE-2026-33205](https://github.com/kovidgoyal/calibre/security/advisories/GHSA-4926-v9px-wv7v).
- Base rate (2021, older): of 9,045 EPUBs from torrents and 4shared, no abuse was found; of the 7,238 torrent files, only 65 used JavaScript, all were functional without it, and none needed local or remote resources — [Franken et al. 2021](https://lirias.kuleuven.be/retrieve/616428). Self-publishing channels could carry scripts, so "store-bought" is not a trust signal (same paper, section VII-B).

### Inferences

- Assets, in priority order: (1) the Kobo database and files, because writes are hard to undo and sit on a physical device; (2) the local library and settings; (3) local files reachable through commands that take paths or URLs; (4) reading privacy (tracking pixels, "this book was opened"); (5) UI integrity (spoofed dialogs, links opening other apps).
- `cover_set_url` is the local analogue of calibre's CVE-2026-33205. It must stay out of any reader capability.
- Attacker model: a book file the user imports from anywhere. No network position is needed. A single open is enough, and hidden 0-size frames mean the user sees nothing.
- Out of scope for rendering isolation, but needs its own check: book metadata and description HTML shown in the React main window, and ZIP handling in the Rust import path. A reader sandbox does not cover these.

### Gaps

- I did not audit each of the 61 commands for path, URL, or SQL handling. Their real blast radius is estimated from names and registration only.

## What attacks have been demonstrated against EPUB readers?

### Takeaway

The recurring failure is the same in almost every case: book content runs with more origin or file access than a web page would. Script-to-host bridges dominate recent CVEs. File disclosure dominated older ones. Native parser bugs are a separate class that isolation does not stop.

### Cited Findings

- Main academic source (IEEE S&P 2021, testing 2019-2020 versions, so older): 97 reading systems across seven platforms plus five devices. Almost none of the JS-supporting ones met the spec's security recommendations; 16 leaked file-system information and eight extracted file contents — [Franken, Van Goethem, Joosen 2021](https://lirias.kuleuven.be/retrieve/616428). Presented at [Black Hat Europe 2021](https://infocondb.org/con/black-hat/black-hat-europe-2021/how-your-e-book-might-be-reading-you-exploiting-epub-reading-systems).
- Techniques from that paper: local file probing through `iframe`, `img`, `audio`, `video`, and font references with load events; content reads via XHR, fetch, `contentWindow`, and canvas `toDataURL`; symlinks inside the ZIP to appear same-origin; timing attacks to detect existence; shared `localStorage` across books (5 of 6 Android apps with LocalStorage); opening other apps through URI schemes (four macOS readers, without consent) — [paper](https://lirias.kuleuven.be/retrieve/616428).
- macOS results (older): all reading systems evaluated on macOS supported JS except FBReader and Kindle; all of those could contact a remote server silently; three of ten leaked arbitrary files — [paper](https://lirias.kuleuven.be/retrieve/616428).
- Apple Books (older): symlink-based library disclosure and persistent denial of service, CVE-2019-8789 and CVE-2019-8774 — [paper](https://lirias.kuleuven.be/retrieve/616428).
- Adobe Digital Editions: file enumeration, CVE-2020-3798 (older) — [paper](https://lirias.kuleuven.be/retrieve/616428); OS command injection from a crafted EPUB, [CVE-2021-39826](https://tenable.com/cve/CVE-2021-39826) (older).
- calibre: version 3 used a 2014 WebKit, allowing arbitrary file leaks and opening of third-party apps (older) — [paper](https://lirias.kuleuven.be/retrieve/616428). Current run of viewer or conversion issues: [CVE-2026-33205](https://github.com/kovidgoyal/calibre/security/advisories/GHSA-4926-v9px-wv7v) (viewer SSRF), [CVE-2026-33206](https://github.com/kovidgoyal/calibre/security/advisories/GHSA-h3p4-m74f-43g6) (traversal, chained with the former), [CVE-2026-25636](https://github.com/kovidgoyal/calibre/security/advisories/GHSA-8r26-m7j5-hm29), [CVE-2023-46303](https://github.com/0x1717/ssrf-via-img) (older).
- epub.js: [CVE-2021-33040](https://github.com/futurepress/epub.js/commit/ab4dd46408cce0324e1c67de4a3ba96b59e5012e), XSS in `managers/views/iframe.js`, fixed in 0.3.89 (older). Downstream: Audiobookshelf turned on `allowScriptedContent`, and epub.js's forced `allow-same-origin` then made the sandbox ineffective; token theft and a path to RCE — [CVE-2024-35236](https://github.com/advplyr/audiobookshelf/security/advisories/GHSA-7j99-76cj-q9pg).
- foliate-js embedders: Grimmory, script in an iframe with both flags, session theft and possible service-worker registration, fixed by removing `allow-scripts` — [CVE-2026-42451](https://github.com/grimmory-tools/grimmory/security/advisories/GHSA-frv6-5wq5-9p24). The advisory places the flaw in the embedding, not in foliate-js. Also File Browser's EPUB preview — [GHSA-5vpr-4fgw-f69h](https://github.com/filebrowser/filebrowser/security/advisories/GHSA-5vpr-4fgw-f69h).
- Thorium: a university library notice says version 3.3 fixed a vulnerability from malicious JavaScript in an EPUB, and that EDRLab urged everyone to upgrade — [University of Oulu](https://www.oulu.fi/en/news/do-you-use-thorium-reader-ellibs-e-book-service-updating-reader-software-recommended). No CVE or vendor advisory found; the repo's advisory API returned none.
- Kindle: [Check Point's chain](https://research.checkpoint.com/2021/i-can-take-over-your-kindle/) used a PDF JBIG2 overflow, [CVE-2021-30354](https://nvd.nist.gov/vuln/detail/CVE-2021-30354), with CVE-2021-30355 for root (older). This is a native parser bug, so iframe isolation is irrelevant to it.
- Microsoft Edge's EPUB reader ran user content script and could leak a local file — [2017 write-up](https://shhnjk.blogspot.com/2017/05/is-your-epub-reader-secure-enough.html) (older).
- CSP-allowlisted `blob:` was abused in EPUBReader: the paper fetched a JS file via an "image", leaked the blob UUID with CSS, then referenced it as script, producing universal XSS — [paper](https://lirias.kuleuven.be/retrieve/616428). This is direct evidence against ever allowing `blob:` in `script-src`.

### Inferences

- The Readest, Grimmory, and Audiobookshelf cases share one root cause, `allow-same-origin` plus executing script. Their failed defence differed (sanitizer, flag choice), so a sanitizer on its own has already been shown to be insufficient.

### Gaps

- The search found no CVE for Thorium, no 2025-2026 CVE for Apple Books or Adobe Digital Editions, and no Kobo desktop or Kobo device reader CVE. That is a search result, not proof that none exist.
- CVE-2025-63365 (SoftSea EPUB Reader traversal) appeared only on an aggregator, so I left it out of the findings.

## What does EPUB 3 require or recommend about scripts and reading system security?

### Takeaway

Scripting is optional in practice (SHOULD), so a reader that never runs book scripts is conformant as long as it honours fallbacks. The firm MUST rules are few: no `file:` access, and a unique origin if you do support spine-level scripts.

### Cited Findings

- Versions checked: EPUB 3.3 is a W3C Recommendation dated 13 January 2026 — [EPUB 3.3](https://www.w3.org/TR/epub-33/). EPUB Reading Systems 3.3 at the TR URL is dated 17 October 2024 — [RS 3.3](https://www.w3.org/TR/epub-rs-33/).
- Reading systems SHOULD support container-constrained scripting in reflowable documents and spine-level scripting in fixed-layout ones. If they do not support scripting they MUST process fallbacks — [RS 3.3 section 6.4](https://www.w3.org/TR/epub-rs-33/).
- If spine-level scripts are supported, the system MUST assign a unique origin shared by all spine-level scripts of the publication, and it is unique "for each user-specific instance" — [RS 3.3](https://www.w3.org/TR/epub-rs-33/).
- Container-constrained scripts MUST NOT modify the host document's DOM or manipulate the size of their containing rectangle — [RS 3.3 section 6.4](https://www.w3.org/TR/epub-rs-33/).
- "Reading systems should treat all scripts as untrusted (and potentially malicious)". The spec lists attacks on the runtime, on the reading system, between documents, and on the local network, and says compliance does not guarantee protection — [RS 3.3 section 6.4.3](https://www.w3.org/TR/epub-rs-33/).
- Reading systems MUST prevent access to resources referenced via file URLs. Reading systems MAY support network access, and it is recommended to notify users of network activity and let them block it. Only `https` remote resources SHOULD be loaded — [RS 3.3 sections 3.3, 3.5, 3.8](https://www.w3.org/TR/epub-rs-33/).
- Content from any new source, including an integrated bookstore, SHOULD be treated as insecure, and users SHOULD be prompted to allow scripting and network access the first time a source is accessed — [RS 3.3 section 15.3](https://www.w3.org/TR/epub-rs-33/).
- Reading systems MAY block scripts from persistent storage, and those allowing it SHOULD let users inspect or delete it — [RS 3.3 section 6.4.1](https://www.w3.org/TR/epub-rs-33/).
- Creators MUST store everything inside the container except audio, video, fonts, and script data resources — [EPUB 3.3 section 3.6](https://www.w3.org/TR/epub-33/). So a remote image reference is already non-conformant content, and blocking it costs a valid book nothing.
- The 2021 paper's authors called the recommendations lenient and argued for banning local references, requiring consent for remote ones, and reconsidering unrestricted script execution — [paper](https://lirias.kuleuven.be/retrieve/616428).

### Inferences

- "No scripts, no network, process fallbacks" is defensible under the spec: the scripting and network language used is SHOULD and MAY.
- The per-publication unique origin is only a MUST if spine-level scripting is supported. A no-scripts reader can defer it.

### Gaps

- I read RS 3.3 through a page extractor in chunks, with quotes capped in length. I did not read EPUB 3.3 section 11 (the creator-facing security section) in full.

## Is Tauri 2 IPC reachable from iframes, and what do capabilities, remote scope, Isolation and labels actually limit?

### Takeaway

No IPC API is injected into subframes on macOS, but a same-origin frame simply borrows the parent's. Cross-origin, sandboxed, and custom-scheme frames can still reach the raw transport, so a random 128-bit invoke key is all that stops them. A window with no capabilities does not block this app's custom commands until an app manifest exists.

### Cited Findings

- All core scripts, including the invoke key closure and `__TAURI_INTERNALS__`, are injected `for_main_frame_only: true` — [manager/webview.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/manager/webview.rs#L159-L216). wry defines `window.ipc` as main-frame-only too — [wkwebview/mod.rs](https://github.com/tauri-apps/wry/blob/wry-v0.57.1/src/wkwebview/mod.rs#L693-L698).
- This came from a fix: Tauri used to inject into iframes on macOS. The invoke key was added as the fix, and Tauri says it is "only intended to block IPC access from sub-frames", not to protect a compromised window — [GHSA-57fm-592m-34r7 / CVE-2024-35222](https://github.com/tauri-apps/tauri/security/advisories/GHSA-57fm-592m-34r7).
- The key is 16 random bytes — [lib.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/lib.rs#L1281-L1285), held in a closure — [ipc-protocol.js](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/scripts/ipc-protocol.js#L6-L12). A mismatch drops the request — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2005-L2023). It is one key per app, from `manager.invoke_key()`, not per window.
- WKWebView-tested reachability matrix (S and P labels are harness scenarios). "Own internals" is `window.__FAKE_INTERNALS__` in the frame. "Parent internals" is the same object via `parent`.

| Frame | Script ran? | `self.origin` | Own internals / `window.ipc` | Parent internals | `webkit.messageHandlers.ipc` | `ipc://` POST reached handler (Origin seen) |
|---|---|---|---|---|---|---|
| blob, `allow-same-origin allow-scripts`, permissive CSP (P1) | yes | `app://localhost` | no | **yes** | yes | yes (`app://localhost`) |
| blob, `allow-scripts` only, permissive CSP (P2) | yes | `null` | no | blocked | yes | yes (`null`) |
| srcdoc, `allow-scripts` only, permissive CSP (P3) | yes | `null` | no | blocked | yes | yes (`null`) |
| `book://b1`, no sandbox, no own CSP (S4) | yes | `book://b1` | no | blocked | yes | yes (`book://b1`) |
| `book://b1`, `sandbox=allow-scripts` (S7) | yes | `null` | no | blocked | yes | yes (`null`) |
| `book://b1`, response CSP `sandbox allow-scripts` (S6) | yes | `null` | no | blocked | yes | yes (`null`) |
| `book://b1`, both sandbox flags (S8) | yes | `book://b1` | no | blocked | yes | yes (`book://b1`) |

- So `sandbox` does not remove `webkit.messageHandlers.ipc`. The native handler received those messages and could see `isMainFrame=false` and the real frame URL even for opaque-origin frames (WKWebView-tested). wry forwards the frame's request URL — [wry_web_view_delegate.rs](https://github.com/tauri-apps/wry/blob/wry-v0.57.1/src/wkwebview/class/wry_web_view_delegate.rs#L46-L59).
- Origin handling differs by path. The custom-protocol path parses the `Origin` header and errors if it is not a valid URL — [ipc/protocol.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/ipc/protocol.rs#L479-L495). The `ipc://` handler answers with `Access-Control-Allow-Origin: *` — [same file](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/ipc/protocol.rs#L47-L56).
- User-registered custom schemes count as **local**: `is_local_url` returns true for any scheme in `uri_scheme_protocols` on macOS — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L1961-L2002). A custom scheme therefore gets no "remote" protection from the ACL.
- ACL resolution uses window and webview labels supplied by the native side plus the origin — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2051-L2060). Capabilities support `windows`, `webviews`, and `remote` — [capability.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri-utils/src/acl/capability.rs#L146-L170). Overlapping capabilities merge — [Tauri docs](https://v2.tauri.app/security/capabilities/).
- A `remote` capability is an allowance for URLs that would otherwise get nothing. Remote origins are denied custom commands unless a capability grants them — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2076-L2083). The docs warn that on Linux and Android Tauri cannot distinguish iframe from window requests — [Tauri docs](https://v2.tauri.app/security/capabilities/).
- Isolation Pattern: a sandboxed iframe hook that sees and encrypts all IPC, designed mainly for supply-chain threats — [Tauri docs](https://v2.tauri.app/concept/inter-process-communication/isolation/). It is injected into the main frame with `iframe.sandbox.add('allow-scripts')` — [isolation.js](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/scripts/isolation.js).
- Child webviews in one window (`add_child`) need the `unstable` feature — [window/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/window/mod.rs#L1224-L1225). A plain second window needs no feature flag.
- Relevant advisories, both fixed at the project's 2.12.5: channel fetch ACL bypass that allowed cross-webview data theft, fixed in 2.11.6 — [GHSA-w28w-mhc8-qvjv](https://github.com/tauri-apps/tauri/security/advisories/GHSA-w28w-mhc8-qvjv); origin confusion on Windows and Android only, fixed in 2.11.1 — [CVE-2026-42184](https://github.com/tauri-apps/tauri/security/advisories/GHSA-7gmj-67g7-phm9). The `plugin:__TAURI_CHANNEL__|fetch` ACL exemption itself still exists at 2.12.5, now bound to the owning webview — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2080-L2082).
- `Cargo.toml` pins only `tauri = "2"`, so the safe version rests on the lockfile (`app/src-tauri/Cargo.toml`, `Cargo.lock`).

### Inferences

- **Same-origin frame (what foliate-js, Readium, and epub.js use)**: if script runs, it has the parent's full IPC. No Tauri feature can tell it apart from the app.
- **Sandboxed, blob-opaque, or custom-scheme frame**: API absent, transport reachable, gated only by the key. The key cannot be read cross-origin, so this looks sound. It is one secret though, so I would not make it the only barrier for hostile scripts.
- `Origin: null` frames on the `ipc://` path should fail at URL parsing. On the `postMessage` path the frame URL is used instead, so `book://...` would classify as local even when the frame's origin is opaque. Both still need the key.
- The Isolation Pattern is the wrong tool. It cannot tell a book's call from the app's call when both go through the same `invoke`, and its hook sees no frame identity.
- ACL enforcement by label is the only control that still holds if script does run in the reader's own origin, because labels are set natively. That is why the app manifest is the most valuable single change here.

### Gaps

- Not run in a real Tauri app: the end-to-end rejection for a wrong key, and `null` Origin parsing failure. Source reading and the harness together support both, but a 10-minute test in a real build should confirm them.
- Whether separate WKWebViews get separate WebContent processes in this configuration was not tested. They share a process pool by default in WebKit's design as far as I know, so do not assume process separation.

## iframe sandbox semantics in WebKit: what do the flags do, why do engines need them, and what bugs force them?

### Takeaway

Together, `allow-scripts` and `allow-same-origin` make the sandbox worthless against script from the same origin. foliate-js needs `allow-same-origin` by design, because it drives the frame DOM directly. It needs `allow-scripts` only because of a WebKit bug that was fixed on `main` yesterday (2026-11-06) and is still present in the WebKit I tested. The real script barrier has to be CSP.

### Cited Findings

- MDN: when embedded and embedding documents are same-origin, using both flags is "strongly discouraged" since the frame can remove its own `sandbox` attribute — [MDN iframe](https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/iframe). `allow-same-origin` also lets the parent access the frame DOM without `allow-scripts` — same page.
- WebKit gives a sandboxed document an opaque origin when the origin sandbox flag is set — [Document.cpp `initSecurityContext`](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/Source/WebCore/dom/Document.cpp#L8781-L8785).
- foliate-js sets `sandbox="allow-same-origin allow-scripts"` with a comment that `allow-scripts` "is needed for events because of WebKit bug" 218086 — [paginator.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/paginator.js#L242-L244); [fixed-layout.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/fixed-layout.js#L84-L86). It reads `iframe.contentDocument` directly — [paginator.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/paginator.js#L251).
- The foliate-js README states scripted content is unsupported because safe execution is impossible from a same-origin `blob:` URL, that the WebKit bug makes the iframe sandbox useless, and that it is "imperative" to use CSP to block all scripts except `'self'` — [README Security](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md#security).
- Root cause in WebKit: event listeners were checked against the target frame. Before the fix, `canExecuteScripts` failing for a sandboxed target dropped a parent's listener — [ScriptController.cpp](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/Source/WebCore/bindings/js/ScriptController.cpp#L856-L880).
- **New as of yesterday**: [commit 64114422f68e](https://github.com/WebKit/WebKit/commit/64114422f68e) (325977@main, 2026-11-06), "Events should be dispatched to nodes inside a sandboxed iframe", under [bug 98951](https://bugs.webkit.org/show_bug.cgi?id=98951) (RESOLVED FIXED). It runs the listener in the frame of the function's realm when the target is script-sandboxed — [JSEventListener.cpp](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/Source/WebCore/bindings/js/JSEventListener.cpp#L176-L187).
- The bug foliate-js cites, [218086](https://bugs.webkit.org/show_bug.cgi?id=218086), still showed NEW when fetched and has been reported up to 2025. A comment marks it a duplicate of a different bug (221283). I did not find an explicit link from 218086 to the fix. The commit describes the same symptom, so the link is my reading.
- Commit limitations: inline handlers in the sandboxed document stay blocked, isolated-world listeners stay blocked, and listeners compiled in the parent then adopted into the iframe stay blocked — [commit message](https://github.com/WebKit/WebKit/commit/64114422f68e).
- **WKWebView-tested on macOS 27.0.1**: with a blob frame and `sandbox="allow-same-origin"`, parent listeners on element and document fired 0 times (E1). With `allow-same-origin allow-scripts` they fired once each (E2). Parent DOM access worked in both. So the bug is live in the shipped framework here.
- epub.js defaults to `sandbox="allow-same-origin"` and adds `allow-scripts` only with `allowScriptedContent` — [iframe.js](https://github.com/futurepress/epub.js/blob/eee359d/src/managers/views/iframe.js#L93-L100). Audiobookshelf's advisory confirms consumers cannot change this — [CVE-2024-35236](https://github.com/advplyr/audiobookshelf/security/advisories/GHSA-7j99-76cj-q9pg).
- Readium ts-toolkit uses `allow-same-origin allow-scripts` — [FrameManager.ts](https://github.com/readium/ts-toolkit/blob/561ced5/navigator/src/epub/frame/FrameManager.ts#L34) — and injects its own scripts as blobs — [epubInjectables.ts](https://github.com/readium/ts-toolkit/blob/561ced5/navigator/src/injection/epubInjectables.ts#L30-L43).
- WKWebView-tested: without `allow-same-origin`, the parent's `contentDocument` is null (S3, P2, S7), and any cross-origin custom-scheme frame is also unreadable even with both flags (S4, S8). So a truly isolated frame cannot be driven by these engines.

### Inferences

- Engine consequences on current WKWebView:
  - foliate-js: must keep both flags. Works only with CSP as the barrier. Compatible with blocking scripts: S1 and E2 show events still work while inline script is blocked.
  - epub.js: default sandbox has no script risk, but parent event listeners will silently fail on shipped WebKit (E1), so selection and link handling would likely need `allowScriptedContent`, the same trade.
  - Readium ts-toolkit: needs its injected blob scripts to run, so an inherited `script-src 'self'` would break it (S12 shows blob scripts blocked). Allowing `blob:` in `script-src` would let book scripts that the engine rewrites to blob URLs run too. It is the worst fit for this threat model.
- Removing `allow-scripts` from foliate-js later is plausible once the fix ships in a macOS version the app can require. Treat the fix as a bonus layer. CSP stays primary, and you would need to retest everything foliate-js does in the frame, as I only tested click listeners.

### Gaps

- Which Safari or macOS release will ship 325977@main is unknown; the bug gives no version and I found no release note. Safari 27 shipped 2026-09-14 per [Apple](https://support.apple.com/en-al/149039), before the fix, and E1 is consistent with that.
- Only `click` listeners were tested. foliate-js also uses other callback types in the frame that I did not exercise.

## CSP design: how do we block scripts and network requests from book content?

### Takeaway

CSP works well for this in WKWebView: every script and network probe in a blob or srcdoc frame was blocked by a policy on the parent. The current CSP must be loosened for frames and blobs in a very specific way, and `script-src` must never gain `blob:`, `'unsafe-inline'`, or `'unsafe-eval'`. Custom-scheme documents do not inherit CSP, so each response must carry its own.

### Cited Findings

- Inheritance in source: `about:srcdoc` inherits the parent's policy container, and other requester-triggered local loads inherit the requester's — [DocumentWriter.cpp](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/Source/WebCore/loader/DocumentWriter.cpp#L243-L256). WebKit has a layout test that a blob iframe inherits **all** CSP policies of its creator — [blob-url-inherits-multiple-csp-policies](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/LayoutTests/http/tests/security/contentSecurityPolicy/blob-url-inherits-multiple-csp-policies.html).
- `'self'` in a blob document points at the creator's origin, via `updateSourceSelf` — [DocumentWriter.cpp](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/Source/WebCore/loader/DocumentWriter.cpp#L254-L255). Readium's comment that `'self'` "is useless" for blob documents — [FrameBlobBuilder.ts](https://github.com/readium/ts-toolkit/blob/561ced5/navigator/src/epub/frame/FrameBlobBuilder.ts#L46) — does not match WebKit here.
- Custom-scheme responses honour CSP headers: Tauri itself sends CSP as a response header on `tauri://` — [protocol/tauri.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/protocol/tauri.rs#L182-L183). WKWebView-tested: `default-src 'none'` on a `book://` response blocked inline script and a remote image (S5). The CSP `sandbox` header gave an opaque origin (S6).
- WKWebView-tested: a custom-scheme iframe does **not** inherit parent CSP. Under a strict parent, S4 ran its inline script and called `ipc://`.
- Custom schemes are secure contexts: `schemeIsHandledBySchemeHandler` makes the origin potentially trustworthy — [SecurityOrigin.cpp](https://github.com/WebKit/WebKit/blob/ebd4c5dee7/Source/WebCore/page/SecurityOrigin.cpp#L90-L105). WKWebView-tested: `isSecureContext` was true, and `book://b1` is a host-separated origin.
- WKWebView-tested strict policy on the parent: `default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline' blob:; img-src 'self' blob: data:; font-src 'self' blob:; media-src 'self' blob:; frame-src blob: book:; connect-src 'self'; object-src 'none'; base-uri 'none'; form-action 'none'`. With `sandbox="allow-same-origin allow-scripts"` frames, these all failed to execute: inline `<script>` in blob (S1) and srcdoc (S2), `img onerror` (S9), nested `srcdoc` (S10), nested `data:` iframe (S11), external blob script (S12), and `javascript:` iframe (S13).
- WKWebView-tested network: under that policy the server logged **zero** hits for stylesheet, `@font-face`, CSS background, `img`, `video poster`, `audio` (S14), meta refresh (S15), nested remote iframe (S16), or `link rel=prefetch/dns-prefetch/preconnect` (S17). The positive control under a permissive policy logged hits for stylesheet, font, background, image, poster, audio, meta refresh, and a `book://` image.
- A `<meta>` CSP inside a blob document was enforced: script blocked and remote image not fetched under a permissive parent (P4).
- WKWebView-tested: a `<meta>` CSP in the **creator** document, tighter than its header, also stopped a child blob frame's image that the header allowed (X1, no server hit). There was no same-run control without the meta tag, so read this as consistent with inheritance rather than conclusive.
- Directives with no default-src fallback: `form-action` falls back to nothing — [MDN](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/form-action). `base-uri`, `form-action`, `frame-ancestors`, and `sandbox` are grouped as document or navigation directives, not fetch directives — [CSP3 WD, 27 October 2026](https://www.w3.org/TR/CSP3/). `frame-src` defers to `child-src`, then `default-src` — same. Meta CSP cannot use `report-uri`, `frame-ancestors`, or `sandbox` — same.
- Tauri CSP mechanics: one app-wide `app.security.csp`. Tauri appends nonces and hashes at compile time — [Tauri docs](https://v2.tauri.app/security/csp/). If `index.html` contains `<style>`, a nonce is added to `style-src` — [manager/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/manager/mod.rs#L126-L154); [html.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri-utils/src/html.rs#L147-L159). The `app/index.html` source currently has no inline style or script.
- CSP is attached in the `tauri://` asset path — [protocol/tauri.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/protocol/tauri.rs#L176-L184) — while in dev the app URL is `devUrl` — [manager/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/manager/mod.rs#L345-L352).
- Comparison of what ships: Foliate: `script-src 'self'; frame-src blob: data:; form-action 'none'` — [reader.html](https://github.com/johnfactotum/foliate/blob/67b6676/src/reader/reader.html#L4). Readest: `script-src 'self' 'unsafe-inline' 'unsafe-eval' data: blob: ...` — [tauri.conf.json](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/tauri.conf.json).

### Inferences

- Current project CSP problems for a reader: there is no `frame-src`, so blob frames fall back to `default-src 'self'` and would be blocked; there is no `blob:` in `style-src`, `font-src`, `img-src`, or `media-src`. The existing `img-src` includes `asset:` and remote cover hosts (`*.mzstatic.com`, `covers.openlibrary.org`, and others), and a book frame would inherit those. A book could then ping those hosts or probe `asset:` paths without any script. Also missing: `object-src`, `base-uri`, `form-action`.
- Recommended split. Keep the Tauri-wide CSP as the loose outer bound, adding `frame-src blob:` and the `blob:` sources, and add a tighter `<meta>` to a dedicated `reader.html`: `default-src 'none'; script-src 'self'; style-src 'self' 'unsafe-inline' blob:; img-src blob: data:; font-src blob: data:; media-src blob:; frame-src blob:; connect-src ipc: http://ipc.localhost; object-src 'none'; base-uri 'none'; form-action 'none'`. This is a starting point and has not been tested as a whole in Tauri.
- Leave `ipc:` out of `connect-src` if large book transfers do not need it. Tauri then falls back to `postMessage` — [ipc-protocol.js](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/scripts/ipc-protocol.js#L60-L70). Project CSP has no `ipc:` today, so main-window invokes probably already take this route after one failed fetch. Note `connect-src` is inherited by frames, though with no script running it is moot.
- Do not give the reader `connect-src asset:`. Pass book bytes through a narrow command, so a frame has no route to other library files.
- `script-src 'self'` still allows a book to reference the app's own scripts by absolute URL, since `'self'` is `tauri://localhost` in blob frames. Subframes have no Tauri internals, so impact looks low, but a separate small reader bundle reduces this. Pinning by exact file path or hash is stronger.
- Inline style attributes in books need `'unsafe-inline'` in `style-src`. If a build ever injects a `<style>` into HTML, Tauri adds a nonce, and CSP semantics then ignore `'unsafe-inline'`, which would silently break book styling. Check the built `dist/*.html`, and use `dangerousDisableAssetCspModification` for `style-src` if needed.
- `tauri dev` probably does not apply this CSP, because the page comes from `devUrl`, not the `tauri://` asset path. A `<meta>` in `reader.html` works in both, which is another reason to put it there. Security tests must run against a built app.

### Gaps

- The interaction between Tauri's header and a `reader.html` meta in a real build, particularly inheritance into foliate-js's blob frames, was only approximated by X1.
- Form submission blocked by `form-action 'none'` was not tested, because a form needs a user click. Bypasses through user clicks on book links generally were not tested.
- `<base href>` handling was not tested separately.

## Options compared: unprivileged window, custom scheme origin, script stripping, per-response CSP

### Takeaway

Best protection for the least work is a **dedicated reader window plus an app manifest that denies commands by default, with strict CSP and foliate-js's blob frames**. CSP stops script, and ACL limits the damage if CSP ever fails. Add WKContentRuleList and `on_navigation` as cheap native layers. Keep sanitizing as a third layer only. Save custom-scheme origin work for a later scripted-EPUB mode, should you ever want one.

### Cited Findings

- Separate window alone is **not** enough here: with no app manifest, a local-origin window with no capabilities still passes the ACL branch for app commands — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L2076-L2083). `tauri_build::AppManifest::new().commands(&[...])` turns on ACL for app commands — [tauri-build lib.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri-build/src/lib.rs#L48).
- Tauri's own advisory recommends dedicated windows or disabling iframe scripting for untrusted content — [GHSA-57fm-592m-34r7](https://github.com/tauri-apps/tauri/security/advisories/GHSA-57fm-592m-34r7).
- Isolated storage: `.incognito(true)` maps to `nonPersistentDataStore`, and `data_store_identifier` to a custom store — [wkwebview/mod.rs](https://github.com/tauri-apps/wry/blob/wry-v0.57.1/src/wkwebview/mod.rs#L275-L289); [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L1076).
- `on_navigation` is available per webview — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L535). wry calls it with no main-frame filter — [navigation.rs](https://github.com/tauri-apps/wry/blob/wry-v0.57.1/src/wkwebview/navigation.rs). WKWebView-tested: subframe navigation, including a meta refresh inside a blob frame, reached the delegate (`targetMain=false`).
- Popups: wry returns no new webview when no handler is set — [wry_web_view_ui_delegate.rs](https://github.com/tauri-apps/wry/blob/wry-v0.57.1/src/wkwebview/class/wry_web_view_ui_delegate.rs#L170-L290). WKWebView-tested: an unsandboxed `book://` frame's `window.open` reached `createWebView` with no user gesture, and a `sandbox=allow-scripts` frame did not. Cross-origin `top.location=` triggered no top-frame navigation in either case.
- WKContentRuleList, WKWebView-tested: rule `{"trigger":{"url-filter":"^https?://"},"action":{"type":"block"}}` under the permissive page gave zero additional server hits, including meta refresh, while custom-scheme pages kept working. A `.*` rule blocked the custom-scheme main page too (`WebKitErrorDomain 104`), so use the `^https?://` filter. Tauri exposes the controller pointer through `with_webview` and `PlatformWebview::controller()` — [webview/mod.rs](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri/src/webview/mod.rs#L204-L215).
- Script stripping has a known failure history: Readest's bypass — [CVE-2026-82642](https://api.osv.dev/v1/vulns/CVE-2026-82642). Its fix added `iframe`, `object`, `embed` to `FORBID_TAGS` and `srcdoc` to `FORBID_ATTR`, and it is now one layer — [sanitizer.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/services/transformers/sanitizer.ts).
- foliate-js gives hooks for this: the `load` event with `isScript`/`allow` — [epub.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/epub.js#L767-L778), and `transformTarget` — [README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md). Its own source notes inline scripts are not handled in rewriting — [epub.js](https://github.com/johnfactotum/foliate-js/blob/78914ae/epub.js#L861).
- `file:` access, WKWebView-tested under a CSP that permitted it from a custom-scheme page: an image failed to load (X2), and `fetch` failed (X4). An iframe of `file:///etc/hosts` gave no load report and no navigation log line (X3). wry sets no file-URL access preference on macOS — grep of [wkwebview/mod.rs](https://github.com/tauri-apps/wry/blob/wry-v0.57.1/src/wkwebview/mod.rs) found none.

| Option | Stops book script? | If script runs anyway | Stops remote loads? | Works with foliate-js? | Effort (my estimate) |
|---|---|---|---|---|---|
| A. Strict CSP, same window | Yes (tested) | Full IPC, all 61 commands | Yes (tested) | Yes | Hours |
| B. Separate window, no app manifest | No, by itself | App commands **still reachable** | No, by itself | Yes | About a day; gives little |
| C. Separate window + app manifest + per-window capability | With A | Limited to reader commands | With A | Yes | 1-2 days |
| D. Custom scheme, distinct origin, engine outside the frame | Needs per-response CSP | Parent internals blocked; `ipc://` and message handler need the key | Needs per-response CSP | **No**, `contentDocument` is unreadable (S4, S8) | Engine rewrite |
| E. Whole reader on a custom-scheme origin, same-origin frames | Needs per-response CSP | Same as C; custom scheme is treated as local | Yes with CSP | Yes | 2-4 days; per-book origin, but not needed without scripts |
| F. Script stripping at serve time | Mostly, with bypass history | Nothing | Partly | Yes | 1 day; layer only |
| G. Calibre-style: engine inside sandboxed, separate-host frame, `postMessage` to host | Allows scripts deliberately | Opaque or isolated, key-gated | Needs CSP | **No**, needs a different engine | Weeks |

### Inferences

Recommended design, in order:

1. **Policy**: never execute book scripts in v1. It is conformant, and 2021 data suggests it has very little reader impact.
2. **Enable app ACL**: add `AppManifest` with all 61 commands, grant them to `main` in `default.json`, and add `capabilities/reader.json` for `reader-*` with only what the reader needs (book bytes for the given ID, save position, save annotation). Validate in Rust that the book ID maps to a library record. Never accept a path or URL from the reader. This improves security regardless of the reader, and it is the only layer that survives a CSP mistake.
3. **Dedicated reader window**: label `reader-<n>`, own `reader.html` and bundle with no React app code, `.incognito(true)` so `localStorage` is not shared with `main`, no devtools in release, and no `opener` or `shell` plugin access.
4. **CSP**: as in the previous section.
5. **Engine hooks**: foliate-js with `allow=false` for script items, plus a transform stripping `script`, `on*`, `iframe`, `object`, `embed`, `srcdoc`, `meta refresh`, and `base`.
6. **Native guards**: `on_navigation` allowlisting `tauri://localhost`, `blob:`, and `about:` for the reader, the `^https?://` content rule list, and default-deny popups. Handle links in a host-side handler that only offers `http(s)` after confirmation. That covers the URI-scheme attack from the paper.
7. **Regression test**: a hostile EPUB fixture run against a **built** app, asserting no script ran, no outbound request occurred, and no privileged command succeeded. johnfactotum's [epub-test](https://github.com/johnfactotum/epub-test) is a starting point — [foliate-js README](https://github.com/johnfactotum/foliate-js/blob/78914ae/README.md#security).

Other points:

- Steps 2, 3, and 4 are the priority. A is the cheapest, but alone it puts the Kobo behind one configuration line. Someone adding `'unsafe-inline'` for a library later would reopen the Readest bug. C changes that failure from "device data at risk" to "this book's reading position".
- Any later "enable scripts" toggle must not reuse this reader. Readest's `allowScript` setting calls `contentWindow.eval` in a same-origin frame — [FoliateViewer.tsx](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/components/FoliateViewer.tsx#L685-L700). Copying that would put book code in the reader's IPC context. Use option G instead, with `connect-src 'none'` per response.
- Option C also likely gives better resilience to a WebContent compromise than CSP, since Tauri checks labels natively. Tauri does not claim to defend against WebView 0-days — [Tauri docs](https://v2.tauri.app/security/capabilities/), so this is only a partial benefit.

### Gaps

- All effort numbers are my estimates and not measured.
- `on_navigation` behaviour with `blob:` and `about:srcdoc` URLs in a real Tauri app is untested; the Rust `Url` parse path should be checked, since a parse failure returns `true` (allow) — [tauri-runtime-wry](https://github.com/tauri-apps/tauri/blob/tauri-v2.12.5/crates/tauri-runtime-wry/src/lib.rs#L4820-L4826).
- Calling `addContentRuleList` on the pointer from `with_webview` has not been done in a Tauri build. The harness used the Swift API directly.
- Performance of moving book bytes over IPC was not measured.

## How do Readest, Thorium, Foliate and Calibre handle it?

### Takeaway

None of them can be copied wholesale. Foliate's model (scripts off, CSP, no privileged bridge beyond its own) is the right base. Calibre has the strongest isolation design but its own engine. Readest is the warning, since it has the same stack as this project and shipped the exact failure. Thorium relies on Electron features with no WKWebView equivalent.

### Cited Findings

- **Readest** (Tauri, foliate-js fork, package version 0.12.20 at the pin):
  - Fork keeps `allow-same-origin allow-scripts` — [paginator.js](https://github.com/readest/foliate-js/blob/b34a0aa63163d19cd13e2ecd19b678394c32fee5/paginator.js#L781-L783).
  - CSP is broad, including `script-src 'self' 'unsafe-inline' 'unsafe-eval' data: blob:` — [tauri.conf.json](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/tauri.conf.json).
  - `reader-*` windows share the `default` capability with `main`, including `shell:allow-spawn` and `fs:*` — [default.json](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src-tauri/capabilities/default.json).
  - Protection is DOMPurify unless `allowScript` is on — [sanitizer.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/services/transformers/sanitizer.ts). `allowScript` defaults to false — [constants.ts](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/services/constants.ts#L422).
  - With it on, inline scripts are eval'd into the frame — [FoliateViewer.tsx](https://github.com/readest/readest/blob/5a105f4/apps/readest-app/src/app/reader/components/FoliateViewer.tsx#L685-L700).
  - A large batch of advisories was published on 2026-10-15 — [advisories](https://github.com/readest/readest/security/advisories).
- **Foliate** (GTK, WebKitGTK):
  - CSP in `reader.html` as above — [reader.html](https://github.com/johnfactotum/foliate/blob/67b6676/src/reader/reader.html#L4).
  - Local storage and HTML5 database off, hyperlink auditing off — [book-viewer.js](https://github.com/johnfactotum/foliate/blob/67b6676/src/book-viewer.js#L168-L176).
  - Reader page is served by a custom scheme with a message-handler bridge — [webview.js](https://github.com/johnfactotum/foliate/blob/67b6676/src/webview.js#L6-L63).
- **Thorium** (Electron, 3.6.0-beta.1 at the pin):
  - Content is in a `<webview>` with `nodeIntegration=0, contextIsolation=1, sandbox=0, webSecurity=1` on a dedicated session partition — [dom.ts](https://github.com/edrlab/thorium-reader/blob/b00cee5/src/r2-xxx-js/r2-navigator-js/electron/renderer/dom.ts#L313-L320).
  - Permissions are denied on that session — [sessions.ts](https://github.com/edrlab/thorium-reader/blob/b00cee5/src/main/sessions.ts#L129-L142).
  - `setWindowOpenHandler` always denies and `will-navigate` always prevents — [browser-window-tracker.ts](https://github.com/edrlab/thorium-reader/blob/b00cee5/src/r2-xxx-js/r2-navigator-js/electron/main/browser-window-tracker.ts#L244-L271).
  - CSP on publication responses is commented out, and responses send `Access-Control-Allow-Origin: *` — [streamerNoHttp.ts](https://github.com/edrlab/thorium-reader/blob/b00cee5/src/main/streamer/streamerNoHttp.ts#L534-L538). Publications share the `thoriumhttps://0.0.0.0` host — [streamerProtocol.ts](https://github.com/edrlab/thorium-reader/blob/b00cee5/src/common/streamerProtocol.ts#L10-L12).
- **Calibre viewer** (Qt WebEngine):
  - A separate `SANDBOX_HOST`, with that host restricted to `book/` and `mathjax/` paths — [web_view.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/gui2/viewer/web_view.py#L56) and [L163-L174](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/gui2/viewer/web_view.py#L163-L174).
  - The iframe uses `allow-popups allow-scripts allow-popups-to-escape-sandbox`, **without** `allow-same-origin` — [view.pyj](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/pyj/read_book/view.pyj#L291-L299).
  - The rendering engine runs inside the frame and talks over `postMessage`, and it declares `spine-scripting` support — [iframe.pyj](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/pyj/read_book/iframe.pyj#L104).
  - `LocalContentCanAccessFileUrls` is off — [webengine.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/utils/webengine.py#L69). Navigation is default-deny, with http(s) only on link click — [web_view.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/gui2/viewer/web_view.py#L414-L428).
  - Its content server sends `Content-Security-Policy: sandbox allow-scripts allow-downloads` plus `nosniff` — [content.py](https://github.com/kovidgoyal/calibre/blob/2abf07c/src/calibre/srv/content.py#L246-L255).
- **Readium ts-toolkit** (web): per-document meta CSP with `connect-src 'none'`, `form-action 'none'`, but `script-src ... blob: 'unsafe-inline'` — [FrameBlobBuilder.ts](https://github.com/readium/ts-toolkit/blob/561ced5/navigator/src/epub/frame/FrameBlobBuilder.ts#L38-L58). A JS "execution prevention" shim replaces storage APIs — [_readium_executionPrevention.js](https://github.com/readium/ts-toolkit/blob/561ced5/navigator/src/dom/_readium_executionPrevention.js).

### Inferences

- Worth copying: from Foliate, the CSP and disabled storage; from Calibre, default-deny navigation and "the sandbox host serves only book files"; from Thorium, deny-by-default permissions, popups, and navigation; from Readium, a per-document meta CSP as an extra layer.
- Worth avoiding: from Readest, a shared capability across reader and main windows, a permissive `script-src`, and relying on a sanitizer; from Readium, treating a JS shim as a security boundary.
- Thorium's model depends on Electron's `<webview>` and preload isolation. WKWebView content worlds would be the nearest analogue, but Tauri's internals live in the page world, so it does not transfer directly.

### Gaps

- I could not find how Calibre blocks direct network access from book script. `web_view.py` has no request interceptor or CSP that I could find, and yet CVE-2026-33205 implies the sandbox normally stops it. This needs a closer read before citing Calibre as a network-blocking model.
- Thorium's actual handling of book JS, and the content of its 3.3 fix, were not determined. Scripts appear to run in the `<webview>`.
- I did not test any of these apps; everything here comes from reading their source and advisories.
