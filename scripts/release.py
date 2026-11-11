"""Version bumps, release notes and cutting a Parallax release.

`release 0.1.0` is what `just release` runs. It checks, without changing
anything, that the checkout is a clean `main` matching origin and that
neither the changelog nor origin has 0.1.0 yet, then runs any `--pre-flight`
commands. Only then does it bump, commit, tag and push the branch and tag in
one atomic push, so the release workflow sees both or neither. If a step
fails or is interrupted, it deletes the tag and resets to where it started.
(The checks and rollback follow ci-cd-release-infrastructure's release.py.)

`bump 0.1.0` sets the version in every manifest and turns CHANGELOG.md's
Unreleased section into that version's. `check v0.1.0` fails unless every
manifest says 0.1.0, which the release workflow runs before building.
`notes 0.1.0` prints that version's changelog section, which becomes the
GitHub release's notes and the in-app update's.
"""

import argparse
import datetime
import json
import re
import subprocess
import sys
from collections.abc import Sequence
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
# Brings Cargo.lock's workspace versions in line with Cargo.toml.
LOCK_UPDATE = ("cargo", "update", "--workspace", "--offline", "--quiet")
RELEASE_FILES = (
    "Cargo.toml",
    "Cargo.lock",
    "app/package.json",
    "app/src-tauri/tauri.conf.json",
    "CHANGELOG.md",
)


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


class ReleaseError(ValueError):
    """A release that can't go ahead, with the reason for the person running it."""


def git(root: Path, *args: str, check: bool = True) -> str:
    result = subprocess.run(
        ["git", *args], cwd=root, capture_output=True, text=True, check=False
    )
    if check and result.returncode:
        detail = (result.stderr or result.stdout).strip()
        raise ReleaseError(f"git {' '.join(args)} failed: {detail}")
    return result.stdout.strip()


def preflight(version: str, root: Path, branch: str, remote: str) -> str:
    """Checks the release can go ahead, changing nothing. Returns HEAD."""
    tag = f"v{version}"
    if not SEMVER.match(version):
        raise ReleaseError(f"{version} isn't a version like 1.2.3")
    found = sections((root / CHANGELOG.relative_to(ROOT)).read_text())
    if version in found:
        raise ReleaseError(f"CHANGELOG.md already has a section for {version}")
    if not found.get(UNRELEASED):
        raise ReleaseError("CHANGELOG.md's Unreleased section is empty")
    current = git(root, "rev-parse", "--abbrev-ref", "HEAD")
    if current != branch:
        raise ReleaseError(f"Release from {branch}, not {current}")
    dirty = git(root, "status", "--porcelain")
    if dirty:
        raise ReleaseError(f"Commit or stash these changes first:\n{dirty}")
    git(root, "fetch", "--quiet", remote, branch)
    head = git(root, "rev-parse", "HEAD")
    upstream = git(root, "rev-parse", f"{remote}/{branch}")
    if head != upstream:
        raise ReleaseError(
            f"{branch} ({head[:8]}) doesn't match {remote}/{branch} "
            f"({upstream[:8]}). Pull or push first."
        )
    if git(root, "tag", "--list", tag):
        raise ReleaseError(f"Tag {tag} already exists here")
    if git(root, "ls-remote", "--tags", remote, f"refs/tags/{tag}"):
        raise ReleaseError(f"Tag {tag} already exists on {remote}")
    return head


def say(message: str) -> None:
    print(message, flush=True)


def release(
    version: str,
    root: Path = ROOT,
    *,
    branch: str = "main",
    remote: str = "origin",
    pre_flight: Sequence[str] = (),
    push: bool = True,
    dry_run: bool = False,
    lock_update: Sequence[str] = LOCK_UPDATE,
) -> None:
    """Bumps, commits, tags and pushes `version`, or changes nothing."""
    tag = f"v{version}"
    say(f"Checking {root.name} can release {tag}")
    start = preflight(version, root, branch, remote)
    for command in pre_flight:
        say(f"Running {command}")
        if subprocess.run(command, shell=True, cwd=root, check=False).returncode:
            raise ReleaseError(f"{command} failed")
    if dry_run:
        say(f"Ready to release {tag}. Nothing changed (--dry-run).")
        return

    tagged = False
    try:
        say(f"Setting the version to {version}")
        bump(version, root)
        subprocess.run(list(lock_update), cwd=root, check=True)
        check(tag, root)
        git(root, "add", "--", *(f for f in RELEASE_FILES if (root / f).exists()))
        git(root, "commit", "--quiet", "-m", f"chore(release): {tag}")
        git(root, "tag", "-a", tag, "-m", f"Parallax {version}")
        tagged = True
        if push:
            say(f"Pushing {branch} and {tag} to {remote} together")
            git(
                root,
                "push",
                "--atomic",
                remote,
                f"HEAD:refs/heads/{branch}",
                f"refs/tags/{tag}",
            )
    except (Exception, KeyboardInterrupt):
        print(f"Rolling back to {start[:8]}", file=sys.stderr)
        if tagged:
            git(root, "tag", "-d", tag, check=False)
        # The tree was clean, so this discards only the release's changes.
        git(root, "reset", "--hard", "--quiet", start, check=False)
        raise
    if push:
        say(f"Released {tag}. The release workflow builds and publishes it.")
    else:
        say(
            f"Tagged {tag} without pushing. Push both: git push --atomic {remote} HEAD:refs/heads/{branch} refs/tags/{tag}"
        )


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    commands = parser.add_subparsers(dest="command", required=True)
    cut = commands.add_parser("release", help="check, bump, commit, tag and push")
    cut.add_argument("version")
    cut.add_argument("--branch", default="main")
    cut.add_argument("--remote", default="origin")
    cut.add_argument(
        "--pre-flight",
        action="append",
        default=[],
        metavar="COMMAND",
        help="a command that must pass before anything changes (repeatable)",
    )
    cut.add_argument("--no-push", action="store_true", help="commit and tag only")
    cut.add_argument(
        "--dry-run", action="store_true", help="run the checks and change nothing"
    )
    commands.add_parser("bump").add_argument("version")
    commands.add_parser("check").add_argument("tag")
    commands.add_parser("notes").add_argument("version")
    args = parser.parse_args(argv)
    try:
        if args.command == "release":
            release(
                args.version,
                branch=args.branch,
                remote=args.remote,
                pre_flight=args.pre_flight,
                push=not args.no_push,
                dry_run=args.dry_run,
            )
        elif args.command == "bump":
            bump(args.version)
        elif args.command == "check":
            print(check(args.tag))
        else:
            text = notes(args.version.removeprefix("v"))
            if text is None:
                raise ValueError(f"CHANGELOG.md has no section for {args.version}")
            print(text)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    except KeyboardInterrupt:
        print("error: release interrupted", file=sys.stderr)
        return 130
    return 0


if __name__ == "__main__":
    sys.exit(main())
