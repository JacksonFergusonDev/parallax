"""Version bumps and release notes for a Parallax release.

`bump 0.2.0` sets the version in every manifest and turns CHANGELOG.md's
Unreleased section into that version's; `just release` runs it, commits and
tags. `check v0.2.0` fails unless every manifest says 0.2.0, which the release
workflow runs before building. `notes 0.2.0` prints that version's changelog
section, which becomes the GitHub release's notes and the in-app update's.
"""

import argparse
import datetime
import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
CARGO = ROOT / "Cargo.toml"
TAURI = ROOT / "app/src-tauri/tauri.conf.json"
PACKAGE = ROOT / "app/package.json"
CHANGELOG = ROOT / "CHANGELOG.md"

SEMVER = re.compile(r"^\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?$")
# The version line in [workspace.package], the only `version =` at line start.
CARGO_VERSION = re.compile(r'^version = "([^"]+)"$', re.MULTILINE)
HEADING = re.compile(r"^## (.+)$", re.MULTILINE)
UNRELEASED = "Unreleased"


def versions(root: Path = ROOT) -> dict[str, str]:
    """The version each manifest declares."""
    cargo = CARGO_VERSION.search((root / CARGO.relative_to(ROOT)).read_text())
    if not cargo:
        raise ValueError("Cargo.toml has no workspace version")
    tauri = json.loads((root / TAURI.relative_to(ROOT)).read_text())["version"]
    package = json.loads((root / PACKAGE.relative_to(ROOT)).read_text())["version"]
    return {"Cargo.toml": cargo[1], "tauri.conf.json": tauri, "package.json": package}


def check(tag: str, root: Path = ROOT) -> str:
    """The version `tag` names, if every manifest agrees with it."""
    version = tag.removeprefix("v")
    if not tag.startswith("v") or not SEMVER.match(version):
        raise ValueError(f"{tag} isn't a release tag like v1.2.3")
    wrong = {f: v for f, v in versions(root).items() if v != version}
    if wrong:
        found = ", ".join(f"{f} says {v}" for f, v in wrong.items())
        raise ValueError(f"{tag} doesn't match the manifests: {found}")
    if notes(version, root) is None:
        raise ValueError(f"CHANGELOG.md has no section for {version}")
    return version


def sections(text: str) -> dict[str, str]:
    """Each `## ` section's body by its heading's first word."""
    found: dict[str, str] = {}
    matches = list(HEADING.finditer(text))
    for i, match in enumerate(matches):
        end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
        found[match[1].split()[0]] = text[match.end() : end].strip()
    return found


def notes(version: str, root: Path = ROOT) -> str | None:
    """The changelog's section for `version`, without its heading."""
    return sections((root / CHANGELOG.relative_to(ROOT)).read_text()).get(version)


def set_json_version(path: Path, version: str) -> None:
    text = path.read_text()
    updated, count = re.subn(
        r'^(  "version": )"[^"]+"', rf'\1"{version}"', text, count=1, flags=re.M
    )
    if count != 1:
        raise ValueError(f"{path.name} has no top-level version")
    path.write_text(updated)


def bump(version: str, root: Path = ROOT, today: datetime.date | None = None) -> None:
    """Sets `version` everywhere and dates the changelog's Unreleased notes."""
    if not SEMVER.match(version):
        raise ValueError(f"{version} isn't a version like 1.2.3")
    changelog = root / CHANGELOG.relative_to(ROOT)
    text = changelog.read_text()
    found = sections(text)
    if version in found:
        raise ValueError(f"CHANGELOG.md already has a section for {version}")
    if not found.get(UNRELEASED):
        raise ValueError("CHANGELOG.md's Unreleased section is empty")
    date = (today or datetime.date.today()).isoformat()
    changelog.write_text(
        text.replace(
            f"## {UNRELEASED}\n", f"## {UNRELEASED}\n\n## {version} ({date})\n", 1
        )
    )
    cargo = root / CARGO.relative_to(ROOT)
    cargo.write_text(CARGO_VERSION.sub(f'version = "{version}"', cargo.read_text(), 1))
    set_json_version(root / TAURI.relative_to(ROOT), version)
    set_json_version(root / PACKAGE.relative_to(ROOT), version)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("bump").add_argument("version")
    commands.add_parser("check").add_argument("tag")
    commands.add_parser("notes").add_argument("version")
    args = parser.parse_args(argv)
    try:
        if args.command == "bump":
            bump(args.version)
        elif args.command == "check":
            print(check(args.tag))
        else:
            text = notes(args.version.removeprefix("v"))
            if text is None:
                raise ValueError(f"CHANGELOG.md has no section for {args.version}")
            print(text)
    except (OSError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
