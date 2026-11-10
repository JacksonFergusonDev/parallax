set shell := ["bash", "-euc", "-o", "pipefail"]
set windows-shell := ["bash", "-euc", "-o", "pipefail"]
set quiet

# Homebrew installs rustup as keg-only. Use the system PATH elsewhere.
export PATH := if os() == "macos" { "/opt/homebrew/opt/rustup/bin:" + env_var("PATH") } else { env_var("PATH") }

script_python := "uv run --directory scripts --no-project python"
script_tools := "uv run --directory scripts --locked"

# --- ANSI Colors ---

blue := '\033[1;34m'
green := '\033[1;32m'
nc := '\033[0m'

# Show available commands
default:
    just --list

# Install locked frontend dependencies and fetch the native kepubify sidecar
sync:
    printf "\n{{ blue }}=== Installing Dependencies ==={{ nc }}\n"
    {{ script_python }} fetch_kepubify.py
    pnpm --dir app install --frozen-lockfile
    pnpm --dir site install --frozen-lockfile

# Format Rust, frontend, and Python code
format: sync
    printf "\n{{ blue }}=== Formatting Code ==={{ nc }}\n"
    cargo fmt --all
    pnpm --dir app format
    pnpm --dir site format
    {{ script_tools }} ruff format .

# Check formatting without editing files
fmt-check: sync fmt-check-scripts
    cargo fmt --all --check
    pnpm --dir app format:check

# Run Rust, frontend, and Python linters
lint: sync lint-scripts
    printf "\n{{ blue }}=== Running Linters ==={{ nc }}\n"
    cargo clippy --locked --workspace --all-targets -- -D warnings
    pnpm --dir app lint

# Check TypeScript and Python types
typecheck: sync typecheck-scripts
    printf "\n{{ blue }}=== Running Type Checks ==={{ nc }}\n"
    pnpm --dir app typecheck

# Run the development script, Rust workspace, and frontend tests
test: sync test-scripts
    printf "\n{{ blue }}=== Running Tests ==={{ nc }}\n"
    cargo test --locked --workspace
    pnpm --dir app test

# Check Python scripts without setting up Rust or the frontend
check-scripts: fmt-check-scripts lint-scripts typecheck-scripts test-scripts

[private]
fmt-check-scripts:
    {{ script_tools }} ruff format --check .

[private]
lint-scripts:
    {{ script_tools }} ruff check .

[private]
typecheck-scripts:
    {{ script_tools }} mypy

[private]
test-scripts:
    {{ script_tools }} pytest -q

# Check the site's formatting and types, and test it against fixed releases
check-site: sync
    pnpm --dir site format:check
    pnpm --dir site check
    pnpm --dir site test

# Run the site locally at http://127.0.0.1:4600
site: sync
    pnpm --dir site dev

# Run the standard local checks before pushing
ci: fmt-check lint typecheck test check-site
    printf "\n{{ green }}✔ Local CI pipeline completed successfully.{{ nc }}\n"

alias setup := sync
alias fmt := format
alias check := ci

# Build the Core ML cover upscaler (slow, once)
upscale-model:
    {{ script_python }} build_upscale_model.py

# Run the desktop app with hot reload
dev: sync upscale-model
    {{ script_python }} dev.py

# Build and run the app as a release behaves, without hot reload
dev-optimized: sync upscale-model
    pnpm --dir app tauri build --no-bundle --config src-tauri/tauri.upscale.conf.json
    target/release/Parallax

# Run tests that mount FAT32 disk images (macOS only)
test-fat:
    cargo test --locked -p parallax-device-kobo -- --ignored
    cargo test --locked -p parallax-core --test send -- --ignored

# Exercise the reader and Settings in a real WebView (opens macOS windows)
test-reader: sync
    node --test scripts/reader-e2e.mjs

# Build the macOS app bundle (target/release/bundle/macos/)
build: sync upscale-model
    pnpm --dir app tauri build --bundles app --config src-tauri/tauri.upscale.conf.json
