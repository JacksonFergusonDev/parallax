# Rust comes from Homebrew's keg-only rustup.
export PATH := "/opt/homebrew/opt/rustup/bin:" + env_var("PATH")

default: check

# One-time setup: UI dependencies and the kepubify sidecar.
setup:
    scripts/fetch-kepubify.sh
    cd app && pnpm install --frozen-lockfile

# Everything that must pass before a commit.
check: setup fmt-check lint test

fmt:
    cargo fmt --all
    cd app && pnpm format

fmt-check:
    cargo fmt --all --check
    cd app && pnpm format:check

lint:
    cargo clippy --workspace --all-targets -- -D warnings
    cd app && pnpm typecheck && pnpm lint

test:
    node --test scripts/dev.test.mjs
    cargo test --workspace
    cd app && pnpm test

# Build the Core ML cover upscaler (slow, once). Not needed for `just check`.
upscale-model:
    scripts/build-upscale-model.sh

# Run the desktop app with hot reload.
dev: setup upscale-model
    scripts/dev.sh

# Build and run the app exactly as a release behaves. No hot reload.
dev-optimized: setup upscale-model
    cd app && pnpm tauri build --no-bundle --config src-tauri/tauri.upscale.conf.json
    target/release/Parallax

# Opt-in tests that mount FAT32 disk images to check the device write path.
test-fat:
    cargo test -p erc-device-kobo -- --ignored

# Opt-in: the reader in a real build, with a hostile book. Opens windows.
test-reader:
    node --test scripts/reader-e2e.mjs

# Build the macOS app bundle (target/release/bundle/macos/).
build: setup upscale-model
    cd app && pnpm tauri build --bundles app --config src-tauri/tauri.upscale.conf.json
