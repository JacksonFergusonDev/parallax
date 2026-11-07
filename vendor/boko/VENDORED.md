# Vendored boko

A copy of [boko](https://github.com/zacharydenton/boko) 0.6.0 (GPL-3.0-or-later) with two fixes, used through `[patch.crates-io]` in the root `Cargo.toml`. Delete this folder and that patch entry once a release contains both.

- AZW3 links: `kindle:pos` links were rewritten to `#aid-X` while the `aid` attributes they point at were stripped, so internal links lost their targets. `transform::aid_to_id` turns them into ids first.
- Combo files: images live in the first half, but they were looked up in the KF8 half, so every image and the cover were lost. `import/azw3.rs` keeps the first half's image index.

Only `src/`, `LICENSE` and a trimmed `Cargo.toml` (no CLI binary, tests or examples) are kept.
