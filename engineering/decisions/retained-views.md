# Retained views and prefetched data

Decided November 13, 2026. The mechanics live in [App caching](../app-caching.md); this records the rule and why.

## Decision

Moving around the main window never shows a loading state for something the app already has, or could have had cheaply.

1. **Views stay mounted once shown.** Every tab panel (Library, On Kobo, Reading) and every sub-view inside one (Reading's Kobo and Goodreads sources) stays mounted after its first showing and is hidden with the `hidden` attribute when inactive. Hidden views keep their data, scroll position, expanded rows and filters. Only a change that makes the view's data wrong unmounts it, such as disconnecting the Kobo that the On Kobo panel shows.
2. **Library reads are prefetched at launch.** Once the library and first device detection settle, the app loads in the background what each view first shows from the library database, through the shared read cache: today `prefetchReading()` in `app/src/api.ts`, which loads the saved Kobo stats and the Goodreads insights. A view opened later finds its data already there.
3. **Views draw cached data on their first render.** A view initialises its state with `readCache.peek` (wrapped by a `peek…` helper in `api.ts`) and only shows a loading state when nothing is cached. Its effect still runs the cached read, so data invalidated by a change in the meantime is fetched again, while the old data stays on screen until the new data arrives.
4. **Device reads wait until someone looks.** Reading a Kobo goes over USB and can hold the library lock, so it is never prefetched at launch. A view shows the saved copy first and reads the device when it becomes visible, once per connection, without blocking what's already drawn. Looking away mid-read doesn't cancel it.
5. **Hidden views aren't rendered ahead of time.** Prefetching fetches data; it doesn't mount views the user hasn't opened. Building charts, tables and cover grids for every tab would slow the window's first appearance, which [App caching](../app-caching.md) keeps to the library and the first viewport of covers.

## Why

Switching tabs or sources used to unmount the Reading views, which flashed "Reading your stats…" each time, even straight after showing the same stats. Library and On Kobo were already retained; Reading wasn't, so the app felt inconsistent. The library reads behind Reading take milliseconds, so loading them at launch costs nothing noticeable. Reading a Kobo is the exception: it's slow, it competes with device detection and the library's first load, and it holds the library lock while it runs.

## Consequences for new work

- **New tabs or sub-views:** render them behind `hidden`, mounted on first showing, and never as `{active && <View/>}`.
- **New library reads a view shows on opening:** go through `read()` in `api.ts`, are added to the launch prefetch when cheap, and get a `peek…` helper that the view uses for its initial state.
- **New device reads:** show saved data first and read the device when the view is visible, never at launch.
- **Mutations:** keep invalidating the cache tags they affect, as [App caching](../app-caching.md) describes. Retained views refetch through their effects and keep the old data on screen meanwhile.
- **Memory:** retained views hold their DOM and data for the session. A view that grows very large, such as a list of thousands of rows, should keep its own rendering bounded rather than give up being retained.

`app/src/App.test.tsx` covers the prefetch, the deferred Kobo read and returning to Reading without a new read or a rebuilt view. `app/src/readCache.test.ts` covers `peek` and `put`.
