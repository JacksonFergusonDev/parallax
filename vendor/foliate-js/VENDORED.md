# Vendored foliate-js

An unmodified copy of [readest/foliate-js](https://github.com/readest/foliate-js) at `b34a0aa` (MIT), the fork of [foliate-js](https://github.com/johnfactotum/foliate-js) that carries the macOS WebKit fixes. The reader in `app/src/reader` renders EPUBs with it. There is no release to depend on, and the project calls its API unstable, so updates are deliberate.

Only the files the EPUB path needs are kept. `app/vite.config.ts` points imports of the other formats (MOBI, FB2, comics, PDF) and the zip library at a stub, because the reader loads books through Rust. `fixed-layout.js` also needs `construct-style-sheets-polyfill`, which is in `app/package.json`.

After updating, run the hostile EPUB in `spikes/reader` against the new copy. See [docs/reader-spike.md](../../docs/reader-spike.md).
