//! Runs the real model, so it needs `just upscale-model` first:
//! `cargo test -p parallax-upscale -- --ignored`

#![cfg(target_os = "macos")]

use std::path::Path;
use std::sync::atomic::AtomicBool;

use image::{Rgb, RgbImage};
use parallax_upscale::{CoreMl, Error, upscale};

const MODEL: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../app/src-tauri/models/cover-upscaler.mlmodelc"
);

#[test]
fn a_missing_model_is_reported() {
    let result = CoreMl::load(Path::new("/no/such/model.mlmodelc"));
    assert!(matches!(result, Err(Error::Unavailable(_))));
}

#[test]
#[ignore = "needs the built model and takes a while"]
fn the_model_keeps_the_picture() {
    let engine = CoreMl::load(Path::new(MODEL)).unwrap();
    // Two flat halves: the result must keep both colours on the right sides.
    let image = RgbImage::from_fn(300, 40, |x, _| {
        Rgb(if x < 150 {
            [200, 40, 40]
        } else {
            [30, 60, 180]
        })
    });
    let out = upscale(&engine, &image, |_, _| {}, &AtomicBool::new(false)).unwrap();
    assert_eq!(out.dimensions(), (1200, 160));
    for (x, expected) in [(100, [200, 40, 40]), (1100, [30, 60, 180])] {
        let Rgb(pixel) = *out.get_pixel(x, 80);
        for (got, want) in pixel.into_iter().zip(expected) {
            assert!(got.abs_diff(want) < 6, "at {x}: {pixel:?} vs {expected:?}");
        }
    }
}
