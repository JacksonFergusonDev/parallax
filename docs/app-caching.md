# App caching

The Library and On Kobo panels remain mounted when switching tabs. The inactive panel uses `display: none`, keeping it out of layout, keyboard focus and the accessibility tree while preserving its data, image elements and scroll position. Device data starts loading as soon as a Kobo connects. Switching tabs does not issue reads or rebuild either book grid.

`app/src/readCache.ts` provides a shared cache for API reads and cover dimensions. Identical pending requests share one promise, including React Strict Mode's repeated effects and collection reads requested by both screens. Successful reads expire after five minutes; online cover searches last ten minutes and image dimensions last one hour. Errors are discarded immediately. The cache holds at most 128 entries and evicts the least recently used entry. It is memory-only and is never persisted across app launches.

`app/src/api.ts` owns invalidation beside the commands that change data:

- Library imports, edits and removals invalidate library data and collection-rule previews. Metadata edits and removals also invalidate the affected book's cover search. Cover changes invalidate library data and device-copy status.
- Device writes invalidate that mount's books, collections, copy status, history, folder groups and rule previews, including when an operation fails after partial changes. Successful reconciliation only invalidates when it actually changes something.
- Rule edits invalidate the rule list and all rule previews.
- Completed device detection results are never reused; simultaneous detections share an outstanding request, and polling waits for the previous detection to finish. Device detection and copy-status reads run on background workers, keeping slow USB reads off the UI thread. Disconnects, reconnects and changes of serial at the same mount discard that mount's cached reads. The retained device panel is unmounted on disconnect and keyed by mount and serial.

Invalidating a pending read removes its entry; completing it later cannot put old data back in the cache. The retained Kobo panel also numbers requests so late responses cannot replace a newer screen snapshot. Transfers and reconciliation trigger a refresh while retaining the current grid until fresh data arrives. Device-local operations refresh the same panel after completion.

Local library cover URLs include the book's modification timestamp. Kobo cover files use content hashes in the host disk cache, so changed image bytes get a new URL. Online covers retain their provider URLs and use the webview's HTTP cache; the app separately caches dimensions so reopening the cover picker does not download every full-size image just to measure it again.

Cover grids keep every card in layout and use native lazy image loading. Do not apply `content-visibility: auto` or estimated intrinsic sizes to cards: deferring their entire subtree can expose blank rows during fast scrolling in WKWebView, and estimated heights do not match the responsive cover aspect ratio. List rows retain their separate content-visibility optimization.

Regression coverage includes request deduplication, expiry, retries, eviction, mutation invalidation, device isolation and reconnects, invalidation during an outstanding read, image measurements, repeated tab switching without reads or image-node replacement, transfer refreshes and out-of-order device responses.
