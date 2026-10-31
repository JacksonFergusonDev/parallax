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
    cargo test --workspace
    cd app && pnpm test

# Run the desktop app with hot reload.
dev: setup
    cd app && pnpm tauri dev

# Opt-in test that mounts a FAT32 disk image to check for `._*` files.
test-fat:
    cargo test -p erc-device-kobo --test transfer -- --ignored
