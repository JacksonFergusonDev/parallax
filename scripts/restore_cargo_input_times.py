"""Restore Cargo input mtimes after a fresh checkout using the cached contents."""

import hashlib
import json
import os
import stat
import subprocess
import sys
import time
from pathlib import Path

SNAPSHOT = Path("target/.ci-cargo-inputs.json")
FINISHED = Path("target/.ci-cargo-finished")
GENERATED = (Path("app/dist"), Path("app/src-tauri/binaries"))


def inputs() -> list[Path]:
    """Include checked-out sources and generated assets that Tauri consumes."""
    paths = subprocess.check_output(["git", "ls-files", "-z"]).split(b"\0")
    files = {Path(os.fsdecode(path)) for path in paths if path}
    for directory in GENERATED:
        files.update(directory.rglob("*"))
    return sorted(path for path in files if path.is_symlink() or path.is_file())


def build_scope(path: Path, packages: set[Path]) -> Path | None:
    if path.is_relative_to(GENERATED[0]):
        return Path("app/src-tauri")
    return next((parent for parent in path.parents if parent in packages), None)


def prepare() -> None:
    cached: dict[str, list[str | int] | None] = {}
    fresh_time = time.time_ns()
    if SNAPSHOT.is_file() and FINISHED.is_file():
        cached = json.loads(SNAPSHOT.read_text())
        # Even if runners' clocks differ, a changed input must be newer than
        # the last completed build whose artifacts we restored.
        fresh_time = max(fresh_time, FINISHED.stat().st_mtime_ns + 1_000_000_000)

    files = inputs()
    paths = set(files)
    old_paths = {Path(name) for name in cached}
    packages = {
        path.parent
        for path in paths | old_paths
        if path.name == "Cargo.toml" and path.parent != Path(".")
    }
    # A new or deleted file can affect module resolution inside its crate
    # even when it was absent from Cargo's previous list of inputs.
    changed_scopes = {build_scope(path, packages) for path in paths ^ old_paths}
    changed_scopes.discard(None)
    can_restore = all(cached.values()) and not any(path.is_symlink() for path in files)
    current: dict[str, tuple[str, int, int] | None] = {}
    reused = 0
    for path in files:
        name = path.as_posix()
        if path.is_symlink():
            current[name] = None
            continue
        info = path.stat()
        with path.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        mode = stat.S_IMODE(info.st_mode)
        previous = cached.get(name)
        if (
            can_restore
            and previous is not None
            and previous[:2] == [digest, mode]
            and build_scope(path, packages) not in changed_scopes
        ):
            os.utime(path, ns=(info.st_atime_ns, int(previous[2])))
            reused += 1
        elif cached:
            os.utime(path, ns=(info.st_atime_ns, fresh_time))
        current[name] = (digest, mode, path.stat().st_mtime_ns)

    SNAPSHOT.parent.mkdir(parents=True, exist_ok=True)
    SNAPSHOT.write_text(json.dumps(current) + "\n")
    print(f"Restored timestamps for {reused} of {len(files)} Cargo inputs.")


if __name__ == "__main__":
    if sys.argv[1:] == ["--finished"]:
        finished = time.time_ns()
        if FINISHED.exists():
            finished = max(finished, FINISHED.stat().st_mtime_ns)
        FINISHED.touch()
        os.utime(FINISHED, ns=(finished, finished))
    else:
        prepare()
