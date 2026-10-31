#!/usr/bin/env bash
# Downloads the pinned kepubify release into app/src-tauri/binaries/ using the
# target-triple names Tauri expects for sidecars. Checksums were recorded on
# first download (upstream publishes none). Idempotent.
set -euo pipefail

VERSION="v4.0.4"
DEST="$(cd "$(dirname "$0")/.." && pwd)/app/src-tauri/binaries"
mkdir -p "$DEST"

fetch() { # asset triple sha256
  local out="$DEST/kepubify-$2"
  if [ -f "$out" ] && echo "$3  $out" | shasum -a 256 -c --status; then
    return
  fi
  curl -sSfL -o "$out.part" "https://github.com/pgaskin/kepubify/releases/download/$VERSION/$1"
  echo "$3  $out.part" | shasum -a 256 -c --status || { rm -f "$out.part"; echo "checksum mismatch for $1" >&2; exit 1; }
  mv "$out.part" "$out"
  chmod +x "$out"
  echo "fetched kepubify $VERSION for $2"
}

fetch kepubify-darwin-arm64 aarch64-apple-darwin 6467d44439ee899113c7f710b509ef9e5ce65e8df711c85192b9ea5b683594b7
fetch kepubify-darwin-64bit x86_64-apple-darwin 851afab0b83ecaf11f6965c901483eed3e74a6b41a3ab0a68f7321bc48bac4a3
