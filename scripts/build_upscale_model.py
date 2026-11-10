"""Build the Core ML cover upscaler on Apple Silicon when the cache is stale."""

import os
import platform
import shutil
import subprocess
import sys
from pathlib import Path
from tempfile import TemporaryDirectory

from download import download_verified

SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parent
DESTINATION = ROOT / "app/src-tauri/models"
WORK = ROOT / "target/upscale-model"
VERSION = "dat2-288-v1"
WEIGHTS_URL = "https://huggingface.co/Phips/4xBHI_dat2_real/resolve/main/4xBHI_dat2_real.safetensors"
WEIGHTS_SHA256 = "1f651fe8e2b7ad978eee0db9c8de89b7bbfd9d2d671ffababb9baf6e2d5b38f6"


def is_current(output: Path, stamp: Path) -> bool:
    try:
        return output.is_dir() and stamp.read_text().strip() == VERSION
    except FileNotFoundError:
        return False


def build(output: Path, stamp: Path) -> None:
    weights = WORK / "weights.safetensors"
    download_verified(WEIGHTS_URL, weights, WEIGHTS_SHA256)
    DESTINATION.mkdir(parents=True, exist_ok=True)

    # Stage the model on the destination volume and keep the old one until conversion succeeds.
    with TemporaryDirectory(dir=DESTINATION) as temporary:
        stage = Path(temporary)
        compiled = stage / output.name
        subprocess.run(
            [
                "uv",
                "run",
                "--project",
                str(SCRIPTS),
                "--locked",
                "--no-active",
                "--only-group",
                "upscale",
                "python",
                str(SCRIPTS / "convert_upscale_model.py"),
                str(weights),
                str(stage / "model.mlpackage"),
                str(compiled),
            ],
            env={**os.environ, "UV_PROJECT_ENVIRONMENT": str(WORK / "venv")},
            check=True,
        )
        if not compiled.is_dir():
            raise ValueError("the converter did not produce a compiled Core ML model")
        new_stamp = stage / stamp.name
        new_stamp.write_text(f"{VERSION}\n")
        stamp.unlink(missing_ok=True)
        if output.exists():
            shutil.rmtree(output)
        compiled.rename(output)
        new_stamp.replace(stamp)


def main() -> int:
    output = DESTINATION / "cover-upscaler.mlmodelc"
    stamp = DESTINATION / "version"
    try:
        if is_current(output, stamp):
            return 0
        if sys.platform != "darwin" or platform.machine() != "arm64":
            raise ValueError("building the cover upscaler requires Apple Silicon macOS")

        import fcntl

        WORK.mkdir(parents=True, exist_ok=True)
        with (WORK / "build.lock").open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX)
            if not is_current(output, stamp):
                build(output, stamp)
                print(f"built {output}")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
