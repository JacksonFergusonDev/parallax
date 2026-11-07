//! Exercise the real binary with captured streams and an isolated local library.
use std::path::Path;
use std::process::{Command, Output};

use erc_library::Library;
use tempfile::TempDir;

fn run(root: &Path, args: &[&str], force_color: bool) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_erc"));
    cmd.args(["library", "--library"])
        .arg(root)
        .args(args)
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR")
        .env_remove("NO_COLOR");
    if force_color {
        cmd.env("CLICOLOR_FORCE", "1");
    } else {
        cmd.env("NO_COLOR", "1");
    }
    let output = cmd.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn library() -> (TempDir, i64, String) {
    let dir = tempfile::tempdir().unwrap();
    let mut lib = Library::open(dir.path()).unwrap();
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/epub/minimal.epub");
    let imported = lib.import(&fixture).unwrap();
    let erc_library::ImportOutcome::Added { book, .. } = imported else {
        panic!("fresh library")
    };
    let title = format!("長い題名 — Café {}", "A long book title ".repeat(10));
    let mut edit = book.edit();
    edit.title.clone_from(&title);
    edit.authors = vec!["Zoë 著者".into()];
    let saved = lib.update(book.id, &edit).unwrap();
    (dir, book.id, saved.title)
}

#[test]
fn piped_book_table_preserves_long_unicode_metadata_without_ansi() {
    let (dir, _, title) = library();
    let output = run(dir.path(), &["ls"], false);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Title"));
    assert!(text.contains("Authors"));
    assert!(text.contains(&title));
    assert!(text.contains("Zoë 著者"));
    assert!(!text.contains('\x1b'));
    assert!(output.stderr.is_empty());
}

#[test]
fn json_is_unstyled_and_has_no_table_or_progress_even_when_color_is_forced() {
    let (dir, id, title) = library();
    let output = run(dir.path(), &["ls", "--json"], true);
    let books: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(books[0]["id"], id);
    assert_eq!(books[0]["title"], title);
    assert!(!output.stdout.contains(&0x1b));
    assert!(output.stderr.is_empty());
}

#[test]
fn rule_table_has_readable_statuses_and_json_retains_original_fields() {
    let dir = tempfile::tempdir().unwrap();
    run(dir.path(), &["rules", "add", "series", "--min", "2"], false);
    let output = run(dir.path(), &["rules", "ls"], false);
    let text = String::from_utf8(output.stdout).unwrap();
    assert!(text.contains("Minimum books"));
    assert!(text.contains("series"));
    assert!(text.contains("enabled"));
    assert!(!text.contains('\x1b'));
    let output = run(dir.path(), &["rules", "ls", "--json"], true);
    let rules: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rules[0]["min_books"], 2);
    assert_eq!(rules[0]["enabled"], true);
}

#[test]
fn empty_library_keeps_a_useful_summary_and_empty_json_array() {
    let dir = tempfile::tempdir().unwrap();
    let output = run(dir.path(), &["ls"], false);
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "0 books\n");
    let output = run(dir.path(), &["ls", "--json"], true);
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap(),
        serde_json::json!([])
    );
}

fn amazon_fixture() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/amazon/book.azw3")
}

fn add(root: &Path, file: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_erc"))
        .args(["library", "--library"])
        .arg(root)
        .arg("add")
        .arg(file)
        .env("NO_COLOR", "1")
        .output()
        .unwrap()
}

/// Goes through the real child process, as the app does.
#[test]
fn adding_an_amazon_book_converts_it_in_a_child_process() {
    let dir = tempfile::tempdir().unwrap();
    let first = String::from_utf8(add(dir.path(), &amazon_fixture()).stdout).unwrap();
    assert!(first.contains("added #1 Amazon Fixture"), "{first}");
    let second = String::from_utf8(add(dir.path(), &amazon_fixture()).stdout).unwrap();
    assert!(second.contains("already in library"), "{second}");
}

#[test]
fn a_drm_book_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let locked = dir.path().join("locked.kfx");
    std::fs::write(&locked, b"\xeaDRMION\xee rest of the file").unwrap();
    let stderr = String::from_utf8(add(dir.path(), &locked).stderr).unwrap();
    assert!(stderr.contains("DRM-protected"), "{stderr}");
}
