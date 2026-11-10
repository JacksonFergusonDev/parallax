"""Restore Cargo input mtimes after a fresh checkout using the cached contents."""

import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import time

SNAPSHOT = Path("target/.ci-cargo-inputs.json")
FINISHED = Path("target/.ci-cargo-finished")
GENERATED = (Path("app/dist"), Path("app/src-tauri/binaries"))


def inputs():
    """Include checked-out sources and generated assets that Tauri consumes."""
    paths = subprocess.check_output(["git", "ls-files", "-z"]).split(b"\0")
    files = {Path(os.fsdecode(path)) for path in paths if path}
    for directory in GENERATED:
        files.update(directory.rglob("*"))
    return sorted(path for path in files if path.is_symlink() or path.is_file())


def prepare():
    cached = {}
    fresh_time = time.time_ns()
    if SNAPSHOT.is_file() and FINISHED.is_file():
        cached = json.loads(SNAPSHOT.read_text())
        # Even if runners' clocks differ, a changed input must be newer than
        # the last completed build whose artifacts we restored.
        fresh_time = max(fresh_time, FINISHED.stat().st_mtime_ns + 1_000_000_000)

    files = inputs()
    # An added or deleted file can affect module resolution even when it was
    # absent from Cargo's previous list of inputs. Recheck the entire tree.
    can_restore = (
        cached.keys() == {path.as_posix() for path in files}
        and all(cached.values())
        and not any(path.is_symlink() for path in files)
    )
    current = {}
    reused = 0
    for path in files:
        name = path.as_posix()
        if path.is_symlink():
            current[name] = None
            continue
        info = path.stat()
        with path.open("rb") as source:
            digest = hashlib.file_digest(source, "sha256").hexdigest()
        identity = [digest, stat.S_IMODE(info.st_mode)]
        previous = cached.get(name)
        if can_restore and previous[:2] == identity:
            os.utime(path, ns=(info.st_atime_ns, previous[2]))
            reused += 1
        elif cached:
            os.utime(path, ns=(info.st_atime_ns, fresh_time))
        current[name] = [*identity, path.stat().st_mtime_ns]

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
