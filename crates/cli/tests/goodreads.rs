//! `goodreads` reads the anonymized export; `--json` keeps stdout to the payload.
use std::path::Path;
use std::process::{Command, Output};

/// `parallax-cli [global...] goodreads <fixture> [after...]`.
fn run(global: &[&str], after: &[&str]) -> Output {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/goodreads/library-export.csv");
    Command::new(env!("CARGO_BIN_EXE_parallax-cli"))
        .args(global)
        .arg("goodreads")
        .arg(fixture)
        .args(after)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

#[test]
fn json_shelf_counts_are_the_only_output() {
    let out = run(&["--json"], &[]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let counts: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(counts["read"], 116);
    assert_eq!(counts["to-read"], 166);
}

#[test]
fn json_lists_one_shelf_in_shelf_order() {
    let out = run(&["--json"], &["--shelf", "favorites"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let books: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(books.len(), 19);
    let position = |book: &serde_json::Value| {
        let shelves = book["shelves"].as_array().unwrap();
        let shelf = shelves.iter().find(|s| s["name"] == "favorites").unwrap();
        shelf["position"].as_u64().unwrap()
    };
    assert!(books.windows(2).all(|w| position(&w[0]) <= position(&w[1])));
}

#[test]
fn unknown_shelf_fails_with_a_message() {
    let out = run(&[], &["--shelf", "nope"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no books on a shelf called nope"));
}
