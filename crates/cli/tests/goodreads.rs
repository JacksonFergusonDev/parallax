//! `goodreads` reads the anonymized export; `--json` keeps stdout to the payload.
use std::path::Path;
use std::process::{Command, Output};

fn fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/goodreads/library-export.csv")
}

/// `parallax-cli [global...] goodreads show <fixture> [after...]`.
fn run(global: &[&str], after: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_parallax-cli"))
        .args(global)
        .args(["goodreads", "show"])
        .arg(fixture())
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

#[test]
fn importing_twice_saves_once_and_json_stays_clean() {
    let dir = tempfile::tempdir().unwrap();
    let import = || {
        let out = Command::new(env!("CARGO_BIN_EXE_parallax-cli"))
            .args(["--json", "goodreads", "import"])
            .arg(fixture())
            .arg("--library")
            .arg(dir.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice::<serde_json::Value>(&out.stdout).unwrap()
    };
    let first = import();
    assert_eq!(
        (first["added"].as_u64(), first["unchanged"].as_u64()),
        (Some(283), Some(0))
    );
    let second = import();
    assert_eq!(
        (second["added"].as_u64(), second["unchanged"].as_u64()),
        (Some(0), Some(283))
    );

    let out = Command::new(env!("CARGO_BIN_EXE_parallax-cli"))
        .args(["--json", "goodreads", "saved"])
        .arg("--library")
        .arg(dir.path())
        .output()
        .unwrap();
    let counts: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(counts["read"], 116);
    assert_eq!(counts["to-read"], 166);
}
