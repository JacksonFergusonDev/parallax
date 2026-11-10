"""Install the native kepubify binary with the sidecar name Tauri expects."""

import subprocess
import sys
from pathlib import Path

from download import download_verified

VERSION = "v4.0.4"
DESTINATION = Path(__file__).resolve().parents[1] / "app/src-tauri/binaries"

# Upstream publishes no checksums. These were recorded on first download.
RELEASES = {
    "aarch64-apple-darwin": (
        "kepubify-darwin-arm64",
        "6467d44439ee899113c7f710b509ef9e5ce65e8df711c85192b9ea5b683594b7",
    ),
    "x86_64-apple-darwin": (
        "kepubify-darwin-64bit",
        "851afab0b83ecaf11f6965c901483eed3e74a6b41a3ab0a68f7321bc48bac4a3",
    ),
    "aarch64-unknown-linux-gnu": (
        "kepubify-linux-arm64",
        "5a15b8f6f6a96216c69330601bca29638cfee50f7bf48712795cff88ae2d03a3",
    ),
    "x86_64-unknown-linux-gnu": (
        "kepubify-linux-64bit",
        "37d7628d26c5c906f607f24b36f781f306075e7073a6fe7820a751bb60431fc5",
    ),
    "aarch64-pc-windows-msvc": (
        "kepubify-windows-arm64.exe",
        "0111c0896c4a82b5ba431768e42870993c8fa44225b10d79691fadf013d13565",
    ),
    "x86_64-pc-windows-msvc": (
        "kepubify-windows-64bit.exe",
        "f3dee3ece063329661fef474262fe403812335bf1676eab35b672565bf195738",
    ),
}


def main() -> int:
    try:
        version = subprocess.check_output(["rustc", "-vV"], text=True)
        target = next(
            (
                line.removeprefix("host: ").strip()
                for line in version.splitlines()
                if line.startswith("host: ")
            ),
            "",
        )
        if not target:
            raise ValueError("rustc did not report a host target")
        if target not in RELEASES:
            raise ValueError(f"kepubify has no configured sidecar for {target}")
        asset, sha256 = RELEASES[target]
        url = f"https://github.com/pgaskin/kepubify/releases/download/{VERSION}/{asset}"
        name = f"kepubify-{target}{Path(asset).suffix}"
        if download_verified(url, DESTINATION / name, sha256, executable=True):
            print(f"fetched kepubify {VERSION} for {target}")
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
