# Kobo comics spike

Throwaway tool that makes four versions of each manga CBZ (original CBZ, lossless KEPUB, processed CBZ, processed KEPUB) for comparing on a Kobo Libra 2. The results and device checklist are in [docs/kobo-comics-spike.md](../../docs/kobo-comics-spike.md). It is outside the Cargo workspace and isn't part of the product. It never touches a Kobo: you copy the output yourself.

## Running it

Needs the kepubify sidecar from `just setup`, or `ERC_KEPUBIFY` pointing at a kepubify binary.

```bash
cargo run --release -- --out out --author "Kentaro Miura" ~/Downloads/"Berserk (Digital) (danke-Empire)"/*.cbz
```

Files land in `out/` (ignored by Git) with a `report.md` of sizes and times. File names must contain the volume as a space then `vNN`, as in `Berserk v01 (2003).cbz`.
