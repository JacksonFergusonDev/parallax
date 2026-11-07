# Vendored boko

A copy of [boko](https://github.com/zacharydenton/boko) 0.6.0 (GPL-3.0-or-later) with two fixes, used through `[patch.crates-io]` in the root `Cargo.toml`. Delete this folder and that patch entry once a release contains both. Verified 2026-11-06: upstream's fix for the link bug plus the combo fix give the same results as this copy on six real books.

- AZW3 links: `kindle:pos` links were rewritten to `#aid-X` while the `aid` attributes they point at were stripped, so internal links lost their targets. `transform::aid_to_id` turns them into ids first. Upstream has a better fix in open [PR #37](https://github.com/zacharydenton/boko/pull/37), so this one isn't being submitted.
- Combo files: images live in the first half, but they were looked up in the KF8 half, so every image and the cover were lost. `import/azw3.rs` keeps the first half's image index. Not reported upstream as of 2026-11-06.

`optimize.rs` also gates `css_referenced_text` behind `optimize-images`, the only feature that calls it. Without this, building with `default-features = false` warns about dead code. This is a build cleanup, not a behaviour change.

Only `src/`, `LICENSE` and a trimmed `Cargo.toml` (no CLI binary, tests or examples) are kept.
