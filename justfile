# dbx — project task runner
# Install: cargo install just
# Usage:   just <recipe>

# ── Shell configuration ───────────────────────────────────────────────────────
# Use PowerShell on Windows so just works without Git Bash / WSL.
set windows-shell := ["powershell.exe", "-NoLogo", "-NonInteractive", "-Command"]

# ── Settings ──────────────────────────────────────────────────────────────────
# Automatically load a .env file if present (useful for RUST_LOG, etc.)
set dotenv-load := true

# Default: list all available recipes
default:
    @just --list

# ── Build ─────────────────────────────────────────────────────────────────────

# Build the debug binary
build:
    cargo build

# Build the optimised release binary
release:
    cargo build --release

# Check without producing build artefacts (fastest feedback loop)
check:
    cargo check

# ── Test ──────────────────────────────────────────────────────────────────────

# Run all in-binary unit tests (no external tools required)
test:
    cargo test --bin dbx

# Run the hermetic black-box CLI tests (no container runtime required)
test-cli:
    cargo test --test cli

# Run the full distrobox integration suite (requires Distrobox + Podman/Docker)
test-integration:
    cargo test --test distrobox_integration -- --ignored --test-threads=1

# Run every test target (unit + CLI; skips ignored integration tests)
test-all:
    cargo test

# ── Lint & Format ─────────────────────────────────────────────────────────────

# Run clippy with warnings-as-errors (mirrors CI)
lint:
    cargo clippy -- -D warnings

# Auto-format all source files
fmt:
    cargo fmt

# Check formatting without modifying files (mirrors CI)
fmt-check:
    cargo fmt --check

# ── Run & Install ─────────────────────────────────────────────────────────────

# Forward arbitrary args to the debug binary: `just run up --dry-run`
run *args:
    cargo run -- {{args}}

# Install the release binary into ~/.cargo/bin
install:
    cargo install --path .

# ── Maintenance ───────────────────────────────────────────────────────────────

# Remove all build artefacts
clean:
    cargo clean

# Run check + lint + fmt-check + test-all (suitable for a pre-push hook)
ci: check lint fmt-check test-all
