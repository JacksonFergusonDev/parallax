#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

if ! command -v inkscape >/dev/null; then
    echo "Install Inkscape and put its CLI on PATH to build the README logos." >&2
    exit 1
fi

export_dir=$(mktemp -d)
trap 'rm -rf "$export_dir"' EXIT

for theme in light dark; do
    inkscape "branding/readme-${theme}-source.svg" \
        --export-text-to-path \
        --export-plain-svg \
        --export-area-page \
        --export-filename="$export_dir/readme-${theme}.svg"
done

mkdir -p assets
mv "$export_dir/readme-light.svg" "$export_dir/readme-dark.svg" assets/
