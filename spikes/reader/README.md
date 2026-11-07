# Reader spike

Throwaway Tauri app that tested the reader design. Results are in [docs/reader-spike.md](../../docs/reader-spike.md). It is outside the Cargo workspace and isn't part of the product. Its commands are stubs, so `device_restore` only writes a log line.

The hostile EPUB fixture (`tools/mk_hostile.py`) is the part worth keeping: port it into the regression test for the real reader.

## Running it

```bash
git clone https://github.com/readest/foliate-js.git front/foliate
git -C front/foliate checkout b34a0aa
(cd front && pnpm install --ignore-scripts && pnpm exec vite build)
mkdir -p books www results/shots
python3 tools/mk_hostile.py books/hostile.epub "$PWD/www/secret.png"
cargo build --release
./tools/run.sh strict reader.html entry hostile 14
```

`run.sh` takes a run name, page, byte path (`entry` or `file`), test, seconds, an optional CSP header override and an optional book. Results go to `results/<name>.log`, and remote requests that reached the local hit server go to `results/<name>.hits`.

Pass `reader-nometa.html` to drop the meta policy. Pass a loose CSP as the sixth argument for a positive control: without one, a clean strict run proves nothing.

Tests are `hostile`, `events`, `nav`, `stress`, `perf` and `diag`. `survey` and `locator` take their books from `SPIKE_LIST`, and `GUARD=1` and `RULES=1` turn on the navigation guard and content rule list.

`tools/map.py` and `tools/exact.py` map Kobo positions. They need copies of `KoboReader.sqlite` (as `latest.sqlite`) and `library.sqlite` in the working directory, and never read a device.

## Notes

- Build a release binary. `tauri dev` serves pages differently and may not apply the CSP.
- Deleting `permissions/` isn't enough to test the no-manifest case unless `build.rs` also stops declaring the manifest. Any file in that folder turns on ACL enforcement.
- Test windows open on screen. Timings are only valid on an otherwise quiet machine, with the window uncovered.
