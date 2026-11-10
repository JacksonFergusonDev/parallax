"""Run Tauri and offer to retry when Vite's port is occupied."""

import os
import re
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

APP = Path(__file__).resolve().parents[1] / "app"
PORT_CONFLICT = b"Error: Port 1420 is already in use"
LISTENERS = ["lsof", "-nP", "-t", "-iTCP:1420", "-sTCP:LISTEN"]


def run_tauri() -> tuple[int, bool]:
    # Keep partial lines and terminal escape sequences visible as they arrive.
    command = shutil.which("pnpm")
    if command is None:
        print("Cannot start Tauri: pnpm is required.", file=sys.stderr)
        return 127, False
    conflict = False
    tail = b""
    with subprocess.Popen(
        [command, "tauri", "dev"],
        cwd=APP,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
    ) as child:
        assert child.stdout is not None
        while chunk := os.read(child.stdout.fileno(), 8192):
            sys.stdout.buffer.write(chunk)
            sys.stdout.buffer.flush()
            if not conflict:
                window = tail + chunk
                conflict = PORT_CONFLICT in window
                tail = window[-(len(PORT_CONFLICT) - 1) :]
        status = child.wait()
    return (status if status >= 0 else 128 - status), conflict


def listeners() -> list[int]:
    result = subprocess.run(LISTENERS, capture_output=True, text=True, check=False)
    # lsof exits 1 without output if nothing matched.
    if result.returncode == 1 and not result.stdout and not result.stderr:
        return []
    result.check_returncode()
    pids = result.stdout.splitlines()
    if any(re.fullmatch(r"[1-9][0-9]*", pid) is None for pid in pids):
        raise ValueError("lsof reported an invalid process ID")
    return [int(pid) for pid in pids]


def release_port() -> bool:
    if shutil.which("lsof") is None:
        print(
            "Cannot identify the process on port 1420: lsof is required.",
            file=sys.stderr,
        )
        return False

    try:
        approved = listeners()
        if not approved:
            print(
                "No listener found on port 1420. Run just dev again to retry.",
                file=sys.stderr,
            )
            return False

        print("Port 1420 is used by:", flush=True)
        for pid in approved:
            subprocess.run(["ps", "-p", str(pid), "-o", "pid=,comm="], check=False)
        print("Kill these processes and retry app startup? [y/N] ", end="", flush=True)
        answer = sys.stdin.readline()
        if not answer.endswith("\n") or answer.strip(" \t\n") not in {
            "y",
            "Y",
            "yes",
            "YES",
        }:
            return False

        # Only signal approved PIDs that still own the port.
        current = set(listeners())
        for pid in approved:
            if pid in current:
                try:
                    os.kill(pid, signal.SIGTERM)
                except OSError:
                    print(
                        f"Could not stop process {pid}. App startup cancelled.",
                        file=sys.stderr,
                    )
                    return False

        deadline = time.monotonic() + 5
        while listeners():
            if time.monotonic() >= deadline:
                print(
                    "Port 1420 is still in use. App startup cancelled.", file=sys.stderr
                )
                return False
            time.sleep(0.1)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"Cannot identify the process on port 1420: {error}", file=sys.stderr)
        return False

    print("Port 1420 is free. Retrying app startup…", flush=True)
    return True


def main() -> int:
    try:
        while True:
            status, conflict = run_tauri()
            if status == 0 or not conflict or not release_port():
                return status
    except KeyboardInterrupt:
        return 130
    except OSError as error:
        print(f"Cannot start Tauri: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
