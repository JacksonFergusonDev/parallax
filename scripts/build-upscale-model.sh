#!/usr/bin/env bash
# Builds the Core ML cover upscaler into app/src-tauri/models/. Slow (about
# five minutes, 9 GB of memory) but only needed once: the result is reused
# until the pinned inputs below change. macOS only. Idempotent.
set -euo pipefail

VERSION="dat2-288-v1"
WEIGHTS_URL="https://huggingface.co/Phips/4xBHI_dat2_real/resolve/main/4xBHI_dat2_real.safetensors"
WEIGHTS_SHA256="1f651fe8e2b7ad978eee0db9c8de89b7bbfd9d2d671ffababb9baf6e2d5b38f6"

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DEST="$ROOT/app/src-tauri/models"
OUT="$DEST/cover-upscaler.mlmodelc"
STAMP="$DEST/version"
WORK="$ROOT/target/upscale-model"

if [ -d "$OUT" ] && [ "$(cat "$STAMP" 2>/dev/null)" = "$VERSION" ]; then
  exit 0
fi

mkdir -p "$WORK" "$DEST"
weights="$WORK/weights.safetensors"
if ! { [ -f "$weights" ] && echo "$WEIGHTS_SHA256  $weights" | shasum -a 256 -c --status; }; then
  curl -sSfL -o "$weights.part" "$WEIGHTS_URL"
  echo "$WEIGHTS_SHA256  $weights.part" | shasum -a 256 -c --status || {
    rm -f "$weights.part"
    echo "checksum mismatch for upscaler weights" >&2
    exit 1
  }
  mv "$weights.part" "$weights"
fi

uv venv -q --allow-existing --python 3.12 "$WORK/venv"
uv pip install -q --python "$WORK/venv/bin/python" -r "$ROOT/scripts/upscale-model-requirements.txt"

rm -rf "$OUT" "$WORK/model.mlpackage"
"$WORK/venv/bin/python" "$ROOT/scripts/convert-upscale-model.py" \
  "$weights" "$WORK/model.mlpackage" "$OUT"
rm -rf "$WORK/model.mlpackage"
echo "$VERSION" > "$STAMP"
echo "built $OUT"
