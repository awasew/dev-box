# Contributing to dev-box

We're excited you want to contribute! This document will guide you through the process.

## Code of Conduct

Be respectful, inclusive, and constructive. We're all here to build something great together.

## Getting Started

### Prerequisites

- [Rust 1.70+](https://rustup.rs)
- [Distrobox](https://distrobox.it) (for testing)
- Podman or Docker
- Git

### Local Development Setup

```sh
# Clone the repo
git clone https://github.com/srikanthrayudu/dev-box
cd dev-box

# Build and test
cargo build
cargo test

# Run the binary
cargo run -- --help

# Build release (optimized)
cargo build --release
./target/release/dev-box --help
```

### Project Structure

```
src/
  main.rs          Entry point, CLI dispatcher
  cli.rs           Clap command definitions
  context.rs       AppContext (config + state)
  util.rs          Helpers
  config/          Configuration merging engine
  engine/          Container lifecycle management (Distrobox)
  host/            Platform abstraction (Linux/macOS/Windows)
  sshd/            Embedded SSH server (russh)

tests/
  cli.rs                     Hermetic black-box CLI tests
  distrobox_integration.rs   Opt-in (#[ignore]) tests needing a real Distrobox

docs/
  architecture.md              Module map, design rationale, diagrams -- read this first
  ide-integration.md          IDE setup guides
  filesystem-performance.md   Benchmark & architecture

Cargo.toml         Dependencies and build config
devbox.ini         Example config for this repo
```

**Before touching `host/` or `engine/`**, read
[`docs/architecture.md`](docs/architecture.md) — it explains the `HostTransport` /
`ContainerEngine` split (the two independent axes almost every change in this repo
touches one of) and walks through adding a new platform or backend step by step.

## How to Contribute

### 1. **Report a Bug**

Found something broken? [Open an issue](https://github.com/srikanthrayudu/dev-box/issues) with:

- **Title**: Short, clear description (e.g., "SSH connection fails on macOS with Lima")
- **Environment**: OS, Rust version, Distrobox version
- **Reproduction steps**: How to consistently trigger the bug
- **Expected vs actual**: What should happen, what actually happened
- **Logs**: Run with `RUST_LOG=debug dev-box <command>` and paste output

### 2. **Request a Feature**

Have an idea? [Open a discussion or issue](https://github.com/srikanthrayudu/dev-box/issues) with:

- **Problem statement**: Why you need this
- **Proposed solution**: Your suggested approach
- **Alternatives considered**: Other ways to solve it
- **Use case**: Concrete example

### 3. **Submit a Pull Request**

#### Pick an Issue

Look for issues labeled:
- **`good first issue`** — great for new contributors
- **`help wanted`** — actively seeking contributions
- **`documentation`** — improve docs, guides, examples
- **`test`** — improve test coverage

Don't see something you want to work on? Start a discussion first to brainstorm with the maintainers.

#### Create Your Branch

```sh
git checkout -b fix/issue-name
# or
git checkout -b feature/new-feature
```

#### Make Your Changes

- Follow Rust conventions (run `cargo fmt` and `cargo clippy`)
- Add tests for new functionality
- Update docs if behavior changes
- Keep commits atomic and descriptive

```sh
# Format code
cargo fmt

# Lint
cargo clippy -- -D warnings

# Test
cargo test

# Test on your actual system
./target/debug/dev-box up
./target/debug/dev-box enter
./target/debug/dev-box ssh-proxy <box>
```

#### Commit & Push

```sh
git add .
git commit -m "fix: brief description of the fix

Longer explanation if needed.

Fixes #123"

git push origin fix/issue-name
```

#### Open a Pull Request

- **Title**: Start with `fix:`, `feat:`, `docs:`, or `test:`
- **Description**: Explain what and why
- **Link issues**: Use `Fixes #123` to auto-close
- **Tests**: Confirm `cargo test` passes
- **Changelog**: Note breaking changes in the PR

**Example PR description:**

```markdown
## Description
Fixes the issue where `dev-box enter --scratchpad` fails on Windows 11 with WSL2.

## Changes
- Updated `host/scratchpad.rs` to handle UNC paths correctly
- Added test case for Windows path handling

## Testing
- Tested on Windows 11 + WSL2 (Ubuntu 22.04)
- Verified scratchpad sync works with `cargo build`
- Ran full test suite: `cargo test`

## Checklist
- [x] Code formatted with `cargo fmt`
- [x] Linted with `cargo clippy`
- [x] Tests pass locally
- [x] Docs updated (if needed)
```

## Common Contribution Areas

### 🐛 Bug Fixes
- HostTransport edge cases (WSL2, Lima, Podman Machine)
- Configuration merging corner cases
- SSH server stability

**Good first PR**: Fix a reported bug with a test case.

### ✨ Features
- Additional ContainerEngine backends (Docker Compose)
- Policy layers for team/org config
- Shell completions (bash, zsh, fish, PowerShell)

**Good first PR**: Add a small CLI flag or config option with docs.

### 📚 Documentation
- IDE setup guides (add Helix, Zed, etc.)
- Platform-specific install instructions
- Troubleshooting guide
- Example repositories

**Good first PR**: Improve or expand an existing guide.

### 🧪 Tests
- Increase coverage (especially `src/config` and `src/host`)
- Integration tests for multi-platform scenarios
- Add regression tests for closed issues

**Good first PR**: Add unit tests for utility functions.

### 🎨 Examples
- Create `dev-box-example-go`, `dev-box-example-java`, etc.
- Add real-world configuration snippets
- Showcase special use cases (Jupyter, CUDA, etc.)

**Good first PR**: Clone and push an existing example repo.

## Testing

Run tests before opening a PR:

```sh
# Unit tests
cargo test

# With logging
RUST_LOG=debug cargo test

# Specific test
cargo test config::tests::test_layer_merging

# Check for clippy warnings
cargo clippy -- -D warnings

# Format check
cargo fmt -- --check
```

## Documentation

- **Code comments**: Explain *why*, not just *what*
- **Doc comments**: Use `///` for public items
- **README**: Update if user-facing behavior changes
- **Docs folder**: Add guides for complex features

Example:

```rust
/// Merges configuration from multiple INI files in priority order.
///
/// Later layers override earlier scalar keys (e.g., `image`).
/// Additive keys (e.g., `additional_packages`) are space-joined.
///
/// # Arguments
/// * `layers` - Slice of INI file paths in ascending priority
///
/// # Returns
/// Merged configuration, or error if parsing fails
pub fn merge_configs(layers: &[&str]) -> Result<Config> {
    // implementation
}
```

## Questions?

- **Discussions**: [GitHub Discussions](https://github.com/srikanthrayudu/dev-box/discussions) for questions and ideas
- **Issues**: Use issues for bugs and feature requests
- **Twitter**: [@srikanthrayudu](https://twitter.com/srikanthrayudu)

## Maintainer Notes

### Release Checklist

- [ ] Update version in `Cargo.toml`
- [ ] Run `cargo test` on all platforms
- [ ] Update `CHANGELOG.md`
- [ ] Tag release: `git tag v0.2.0`
- [ ] Push tag: `git push origin v0.2.0`
- [ ] GitHub Actions builds and publishes binaries + crates.io

---

**Thanks for contributing!** 🚀 Every PR, issue, and discussion helps dev-box reach more developers.
