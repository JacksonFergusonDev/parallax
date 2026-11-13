# App caching

Every tab panel remains mounted once shown, following the [retained views decision](decisions/retained-views.md). Library and On Kobo mount at launch; Reading mounts the first time it's opened, and its Kobo and Goodreads sources each mount the first time they're shown. The inactive panel uses `display: none`, keeping it out of layout, keyboard focus and the accessibility tree while preserving its data, image elements and scroll position. Device data starts loading as soon as a Kobo connects. Switching tabs does not issue reads or rebuild a view.

Once the library and first device detection settle, `prefetchReading()` loads the saved Kobo stats and the Goodreads insights into the read cache. Views draw on their first render from `readCache.peek`, so a prefetched view never shows a loading state. A connected Kobo's stats are read only when its Reading view is on screen, once per connection; `refreshReadingStats` then stores the result with `readCache.put` as the saved stats.

`app/src/readCache.ts` provides a shared cache for API reads and cover dimensions. Identical pending requests share one promise, including React Strict Mode's repeated effects and collection reads requested by both screens. Successful reads expire after five minutes; online cover searches last ten minutes and image dimensions last one hour. Errors are discarded immediately. The cache holds at most 128 entries and evicts the least recently used entry. It is memory-only and is never persisted across app launches.

`app/src/api.ts` owns invalidation beside the commands that change data:

- Library imports, edits and removals invalidate library data and collection-rule previews. Metadata edits and removals also invalidate the affected book's cover search. Cover changes invalidate library data and device-copy status.
- Device writes invalidate that mount's books, collections, copy status, history, folder groups and rule previews, including when an operation fails after partial changes. Successful reconciliation only invalidates when it actually changes something.
- Rule edits invalidate the rule list and all rule previews.
- Completed device detection results are never reused; simultaneous detections share an outstanding request, and polling waits for the previous detection to finish. Device detection and copy-status reads run on background workers, keeping slow USB reads off the UI thread. Disconnects, reconnects and changes of serial at the same mount discard that mount's cached reads. The retained device panel is unmounted on disconnect and keyed by mount and serial.

Invalidating a pending read removes its entry; completing it later cannot put old data back in the cache. The retained Kobo panel also numbers requests so late responses cannot replace a newer screen snapshot. Transfers and reconciliation trigger a refresh while retaining the current grid until fresh data arrives. Device-local operations refresh the same panel after completion.

Local library cover URLs include the book's modification timestamp. Kobo cover files use content hashes in the host disk cache, so changed image bytes get a new URL. Online covers retain their provider URLs and use the webview's HTTP cache; the app separately caches dimensions so reopening the cover picker does not download every full-size image just to measure it again.

Cover grids and book lists keep every card and row in layout. The browser requests thumbnails on mount, including in the hidden Kobo panel. The grid also warms decoded pixels in batches of eight as images mount or their URLs change. Each decode has a two-second limit, and decoded images remain subject to browser memory limits.

The scrolling grids use CSS spine shading and box shadows. The SVG displacement filter can delay rasterization in WKWebView even when every thumbnail has loaded. Do not apply `content-visibility: auto` or estimated intrinsic sizes to cards or list rows. Deferring their subtree can expose blank rows during fast scrolling, and estimated heights can differ from the responsive cover aspect ratio.

The native window starts hidden. After the library read and first device detection settle, the frontend waits for fonts and covers in the initial viewport (at most two seconds), then gives layout a frame to settle before showing and focusing the window. Frame waiting has a timer fallback because hidden webviews can throttle animation frames. Device contents and offscreen thumbnail requests continue in the background. Window-state restoration excludes visibility so it cannot reveal a partially rendered startup window. A failed initial library read still displays its error notice.

Regression coverage includes request deduplication, expiry, retries, eviction, mutation invalidation, device isolation and reconnects, invalidation during an outstanding read, image measurements, repeated tab switching without reads or image-node replacement, transfer refreshes and out-of-order device responses.
