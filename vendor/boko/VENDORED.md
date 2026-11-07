# Vendored boko

A copy of [boko](https://github.com/zacharydenton/boko) 0.6.0 (GPL-3.0-or-later) with three fixes, used through `[patch.crates-io]` in the root `Cargo.toml`. Delete this folder and that patch entry once a release contains all three. Verified 2026-11-06: upstream's fix for the link bug plus the combo fix give the same results as this copy on six real books.

- AZW3 links: `kindle:pos` links were rewritten to `#aid-X` while the `aid` attributes they point at were stripped, so internal links lost their targets. `transform::aid_to_id` turns them into ids first. Upstream has a better fix in open [PR #37](https://github.com/zacharydenton/boko/pull/37), so this one isn't being submitted.
- Combo files: each half can hold resources, and `kindle:embed` numbers them as one list, but only the KF8 half was searched, so images in the first half and the cover were lost. `import/azw3.rs` now keeps a range for each half, as calibre and KindleUnpack do. Ready to submit upstream, not yet posted as of 2026-11-06.
- MOBI contents: a MOBI with no NCX index got a one-entry TOC. `import/mobi.rs` now reads the contents page named in `<guide>`, as calibre does. Not reported upstream as of 2026-11-06.

`optimize.rs` also gates `css_referenced_text` behind `optimize-images`, the only feature that calls it. Without this, building with `default-features = false` warns about dead code. This is a build cleanup, not a behaviour change.

Only `src/`, `LICENSE` and a trimmed `Cargo.toml` (no CLI binary, tests or examples) are kept.
