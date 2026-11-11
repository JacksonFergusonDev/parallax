import datetime
import json
import subprocess
from pathlib import Path

import pytest

import release

CARGO = """[workspace]
members = ["crates/*"]

[workspace.package]
edition = "2024"
version = "0.1.0"

[workspace.dependencies]
serde = { version = "1" }
"""

CHANGELOG = """# Changelog

## Unreleased

- Check for Updates… in the Parallax menu.

## 0.1.0 (2026-12-01)

- The first release.
"""


@pytest.fixture
def root(tmp_path: Path) -> Path:
    (tmp_path / "app/src-tauri").mkdir(parents=True)
    (tmp_path / "Cargo.toml").write_text(CARGO)
    (tmp_path / "CHANGELOG.md").write_text(CHANGELOG)
    for path in ("app/src-tauri/tauri.conf.json", "app/package.json"):
        (tmp_path / path).write_text(
            json.dumps(
                {"name": "x", "version": "0.1.0", "nested": {"version": "9"}}, indent=2
            )
        )
    return tmp_path


def test_check_accepts_a_tag_every_manifest_matches(root: Path) -> None:
    assert release.check("v0.1.0", root) == "0.1.0"


@pytest.mark.parametrize("tag", ["0.1.0", "v0.1", "vnext"])
def test_check_rejects_tags_that_are_not_versions(root: Path, tag: str) -> None:
    with pytest.raises(ValueError, match="isn't a release tag"):
        release.check(tag, root)


def test_check_names_the_manifests_that_disagree(root: Path) -> None:
    with pytest.raises(ValueError, match=r"Cargo\.toml says 0\.1\.0"):
        release.check("v0.2.0", root)


def test_bump_sets_every_manifest_and_dates_the_notes(root: Path) -> None:
    release.bump("0.2.0", root, datetime.date(2027, 1, 15))

    assert release.check("v0.2.0", root) == "0.2.0"
    assert 'serde = { version = "1" }' in (root / "Cargo.toml").read_text()
    tauri = json.loads((root / "app/src-tauri/tauri.conf.json").read_text())
    assert tauri["nested"]["version"] == "9"
    changelog = (root / "CHANGELOG.md").read_text()
    assert "## Unreleased\n\n## 0.2.0 (2027-01-15)\n\n- Check for Updates…" in changelog
    assert release.notes("0.2.0", root) == "- Check for Updates… in the Parallax menu."
    assert release.notes("0.1.0", root) == "- The first release."


def test_bump_refuses_without_unreleased_notes(root: Path) -> None:
    release.bump("0.2.0", root)
    with pytest.raises(ValueError, match="Unreleased section is empty"):
        release.bump("0.3.0", root)


def test_bump_refuses_a_version_already_released(root: Path) -> None:
    with pytest.raises(ValueError, match="already has a section"):
        release.bump("0.1.0", root)


def test_prerelease_versions_are_allowed(root: Path) -> None:
    release.bump("0.2.0-beta.1", root)
    assert release.check("v0.2.0-beta.1", root) == "0.2.0-beta.1"


def test_first_release_keeps_the_manifests_version(root: Path) -> None:
    # The repository starts at 0.1.0, so the first `just release 0.1.0` only
    # dates the changelog.
    (root / "CHANGELOG.md").write_text(
        "# Changelog\n\n## Unreleased\n\n- The first release.\n"
    )
    release.bump("0.1.0", root, datetime.date(2026, 12, 1))
    assert release.check("v0.1.0", root) == "0.1.0"
    assert release.notes("0.1.0", root) == "- The first release."


def run_git(cwd: Path, *args: str) -> str:
    return subprocess.run(
        ["git", *args], cwd=cwd, check=True, capture_output=True, text=True
    ).stdout.strip()


@pytest.fixture
def repo(
    root: Path,
    tmp_path_factory: pytest.TempPathFactory,
    monkeypatch: pytest.MonkeyPatch,
) -> Path:
    """`root` as a clean clone of main, in sync with a bare origin."""
    home = tmp_path_factory.mktemp("git")
    (home / "gitconfig").write_text(
        "[user]\n\tname = Test\n\temail = test@example.test\n[init]\n\tdefaultBranch = main\n"
    )
    monkeypatch.setenv("GIT_CONFIG_GLOBAL", str(home / "gitconfig"))
    monkeypatch.setenv("GIT_CONFIG_NOSYSTEM", "1")
    origin = home / "origin.git"
    run_git(home, "init", "--bare", "--quiet", str(origin))
    run_git(root, "init", "--quiet")
    (root / "Cargo.lock").write_text("# lock\n")
    run_git(root, "add", ".")
    run_git(root, "commit", "--quiet", "-m", "init")
    run_git(root, "remote", "add", "origin", str(origin))
    run_git(root, "push", "--quiet", "origin", "main")
    return root


def origin_of(repo: Path) -> Path:
    return Path(run_git(repo, "remote", "get-url", "origin"))


def cut(repo: Path, **options: object) -> None:
    release.release("0.2.0", repo, lock_update=["true"], **options)  # type: ignore[arg-type]


def assert_unchanged(repo: Path, start: str) -> None:
    assert run_git(repo, "rev-parse", "HEAD") == start
    assert run_git(repo, "status", "--porcelain") == ""
    assert run_git(repo, "tag", "--list") == ""
    assert run_git(origin_of(repo), "tag", "--list") == ""
    assert release.versions(repo)["Cargo.toml"] == "0.1.0"


def test_release_pushes_the_branch_and_only_the_new_tag(repo: Path) -> None:
    run_git(repo, "tag", "unrelated")
    cut(repo)

    origin = origin_of(repo)
    assert run_git(origin, "tag", "--list") == "v0.2.0"
    assert run_git(origin, "rev-parse", "main") == run_git(repo, "rev-parse", "HEAD")
    assert (
        run_git(origin, "log", "-1", "--format=%s", "main") == "chore(release): v0.2.0"
    )
    assert run_git(origin, "cat-file", "-t", "v0.2.0") == "tag"
    assert release.versions(repo) == dict.fromkeys(release.versions(repo), "0.2.0")
    assert run_git(repo, "status", "--porcelain") == ""


def test_dry_run_and_no_push(repo: Path) -> None:
    start = run_git(repo, "rev-parse", "HEAD")
    cut(repo, dry_run=True)
    assert_unchanged(repo, start)

    cut(repo, push=False)
    assert run_git(repo, "tag", "--list") == "v0.2.0"
    assert run_git(origin_of(repo), "tag", "--list") == ""


def test_refuses_a_dirty_checkout(repo: Path) -> None:
    (repo / "notes.txt").write_text("draft")
    with pytest.raises(release.ReleaseError, match="Commit or stash"):
        cut(repo)


def test_refuses_another_branch(repo: Path) -> None:
    run_git(repo, "switch", "--quiet", "-c", "feature")
    with pytest.raises(release.ReleaseError, match="Release from main, not feature"):
        cut(repo)


def test_refuses_a_checkout_out_of_step_with_origin(repo: Path) -> None:
    run_git(repo, "commit", "--quiet", "--allow-empty", "-m", "local only")
    with pytest.raises(release.ReleaseError, match="doesn't match origin/main"):
        cut(repo)


def test_refuses_a_tag_already_on_origin(repo: Path) -> None:
    run_git(repo, "tag", "v0.2.0")
    run_git(repo, "push", "--quiet", "origin", "v0.2.0")
    run_git(repo, "tag", "-d", "v0.2.0")
    with pytest.raises(release.ReleaseError, match="already exists on origin"):
        cut(repo)


def test_a_failing_pre_flight_changes_nothing(repo: Path) -> None:
    start = run_git(repo, "rev-parse", "HEAD")
    with pytest.raises(release.ReleaseError, match="false failed"):
        cut(repo, pre_flight=["true", "false"])
    assert_unchanged(repo, start)


def test_a_failed_lock_update_rolls_back(repo: Path) -> None:
    start = run_git(repo, "rev-parse", "HEAD")
    with pytest.raises(subprocess.CalledProcessError):
        release.release("0.2.0", repo, lock_update=["false"])
    assert_unchanged(repo, start)


def test_a_rejected_push_rolls_back(repo: Path) -> None:
    start = run_git(repo, "rev-parse", "HEAD")
    hook = origin_of(repo) / "hooks/pre-receive"
    hook.write_text("#!/bin/sh\nexit 1\n")
    hook.chmod(0o755)
    with pytest.raises(release.ReleaseError, match="git push"):
        cut(repo)
    assert_unchanged(repo, start)
    assert run_git(origin_of(repo), "rev-parse", "main") == start


def test_current_is_the_shared_version(root: Path) -> None:
    assert release.current(root) == "0.1.0"
    assert (
        release.notes("Unreleased", root)
        == "- Check for Updates… in the Parallax menu."
    )
    release.set_json_version(root / "app/package.json", "0.2.0")
    with pytest.raises(ValueError, match=r"package\.json says 0\.2\.0"):
        release.current(root)
