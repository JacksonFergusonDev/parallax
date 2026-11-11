import datetime
import json
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
