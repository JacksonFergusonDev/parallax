#!/usr/bin/env bash
# Downloads the native kepubify release with the name Tauri expects for its
# sidecar. Checksums were recorded on first download (upstream publishes none).
set -euo pipefail

VERSION="v4.0.4"
target="$(rustc -vV | awk '/^host: / {print $2}' | tr -d '\r')"
case "$target" in
  aarch64-apple-darwin)       asset=kepubify-darwin-arm64;      sha=6467d44439ee899113c7f710b509ef9e5ce65e8df711c85192b9ea5b683594b7 ;;
  x86_64-apple-darwin)        asset=kepubify-darwin-64bit;      sha=851afab0b83ecaf11f6965c901483eed3e74a6b41a3ab0a68f7321bc48bac4a3 ;;
  aarch64-unknown-linux-gnu)  asset=kepubify-linux-arm64;       sha=5a15b8f6f6a96216c69330601bca29638cfee50f7bf48712795cff88ae2d03a3 ;;
  x86_64-unknown-linux-gnu)   asset=kepubify-linux-64bit;       sha=37d7628d26c5c906f607f24b36f781f306075e7073a6fe7820a751bb60431fc5 ;;
  aarch64-pc-windows-msvc)    asset=kepubify-windows-arm64.exe; sha=0111c0896c4a82b5ba431768e42870993c8fa44225b10d79691fadf013d13565 ;;
  x86_64-pc-windows-msvc)     asset=kepubify-windows-64bit.exe; sha=f3dee3ece063329661fef474262fe403812335bf1676eab35b672565bf195738 ;;
  *) echo "kepubify has no configured sidecar for $target" >&2; exit 1 ;;
esac

if command -v sha256sum >/dev/null; then
  checksum=(sha256sum)
else
  checksum=(shasum -a 256)
fi
dest="$(cd "$(dirname "$0")/.." && pwd)/app/src-tauri/binaries"
mkdir -p "$dest"
out="$dest/kepubify-$target"
if [[ "$asset" == *.exe ]]; then out+=.exe; fi
matches() {
  local actual
  actual="$("${checksum[@]}" < "$1")"
  [[ "${actual%% *}" == "$sha" ]]
}

if [[ -f "$out" ]] && matches "$out"; then
  chmod +x "$out"
  exit 0
fi

part="$(mktemp "$out.XXXXXX")"
trap 'rm -f "$part"' EXIT
curl -fsSL --retry 3 -o "$part" "https://github.com/pgaskin/kepubify/releases/download/$VERSION/$asset"
matches "$part" || { echo "checksum mismatch for $asset" >&2; exit 1; }
chmod +x "$part"
mv -f "$part" "$out"
echo "fetched kepubify $VERSION for $target"
