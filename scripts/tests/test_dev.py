"""Exercise the launcher with fake Tauri and lsof commands."""

import os
import signal
import subprocess
import sys
from pathlib import Path

import pytest

pytestmark = pytest.mark.skipif(
    sys.platform == "win32", reason="requires Unix signals and lsof"
)

SCRIPT = Path(__file__).resolve().parents[1] / "dev.py"

PNPM = """#!/bin/bash
[[ "$*" == 'tauri dev' ]] || exit 99
if [[ -s "$CALLS" ]]; then
    echo retry
    exit 0
fi
echo called >> "$CALLS"
case "$MODE" in
    success) exit 0 ;;
    unrelated) echo 'Rust compilation failed' >&2; exit 17 ;;
    *) echo 'Error: Port 1420 is already in use' >&2; exit 23 ;;
esac
"""

LSOF = """#!/bin/bash
[[ "$*" == '-nP -t -iTCP:1420 -sTCP:LISTEN' ]] || exit 99
if [[ "$RELEASED" == 1 && -e "$CALLS.lsof" ]]; then exit 1; fi
touch "$CALLS.lsof"
found=1
for pid in $LISTENER_PIDS; do
    state=$(ps -p "$pid" -o stat=)
    if [[ -n "$state" && "$state" != Z* ]]; then echo "$pid"; found=0; fi
done
exit "$found"
"""


def run_dev(
    directory: Path,
    answer: str = "",
    *,
    mode: str = "conflict",
    released: bool = False,
    count: int = 1,
) -> tuple[subprocess.CompletedProcess[str], list[int | None]]:
    for name, body in [("pnpm", PNPM), ("lsof", LSOF)]:
        stub = directory / name
        stub.write_text(body)
        stub.chmod(0o755)

    children: list[subprocess.Popen[bytes]] = []
    try:
        for _ in range(count):
            children.append(
                subprocess.Popen([sys.executable, "-c", "import time; time.sleep(60)"])
            )
        result = subprocess.run(
            [sys.executable, str(SCRIPT)],
            env={
                **os.environ,
                "PATH": f"{directory}{os.pathsep}{os.environ['PATH']}",
                "CALLS": str(directory / "calls"),
                "MODE": mode,
                "LISTENER_PIDS": " ".join(str(child.pid) for child in children),
                "RELEASED": "1" if released else "0",
            },
            input=answer,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            check=False,
            timeout=10,
        )
        return result, [child.poll() for child in children]
    finally:
        for child in children:
            child.kill()
            child.wait()


@pytest.mark.parametrize("answer", ["n\n", "\n", "", "maybe\n", "y"])
def test_declining_preserves_listeners_and_failure_status(
    tmp_path: Path, answer: str
) -> None:
    result, listeners = run_dev(tmp_path, answer)
    assert result.returncode == 23
    assert "[y/N]" in result.stdout
    assert listeners == [None]
    assert "Retrying" not in result.stdout


@pytest.mark.parametrize("answer", ["y\n", "Y\n", "yes\n", "YES\n"])
def test_approval_terminates_all_listeners_and_retries(
    tmp_path: Path, answer: str
) -> None:
    result, listeners = run_dev(tmp_path, answer, count=2)
    assert result.returncode == 0, result.stdout
    assert "Retrying app startup" in result.stdout
    assert "retry\n" in result.stdout
    assert listeners == [-signal.SIGTERM, -signal.SIGTERM]


@pytest.mark.parametrize(("mode", "status"), [("success", 0), ("unrelated", 17)])
def test_other_exits_do_not_prompt(tmp_path: Path, mode: str, status: int) -> None:
    result, listeners = run_dev(tmp_path, "y\n", mode=mode)
    assert result.returncode == status
    assert "[y/N]" not in result.stdout
    assert listeners == [None]


def test_released_listener_is_left_running(tmp_path: Path) -> None:
    result, listeners = run_dev(tmp_path, "y\n", released=True)
    assert result.returncode == 0
    assert "Retrying app startup" in result.stdout
    assert listeners == [None]


def test_conflict_without_listener_exits_without_prompting(tmp_path: Path) -> None:
    result, _ = run_dev(tmp_path, "y\n", count=0)
    assert result.returncode == 23
    assert "No listener found" in result.stdout
    assert "[y/N]" not in result.stdout
