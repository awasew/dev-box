# dev-box

> **IDE-independent developer environments with keyless SSH, native filesystem performance, and zero vendor lock-in.**

[![Release](https://img.shields.io/github/v/release/srikanthrayudu/dev-box?color=blue&label=release)](https://github.com/srikanthrayudu/dev-box/releases)
[![Status: Active Development](https://img.shields.io/badge/status-active--development%20%28alpha%29-orange.svg)](#status)
[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)
[![Crates.io](https://img.shields.io/crates/v/dev-box?logo=rust)](https://crates.io/crates/dev-box)
[![GitHub Stars](https://img.shields.io/github/stars/srikanthrayudu/dev-box?style=social)](https://github.com/srikanthrayudu/dev-box)

A **lightweight, single-binary Rust orchestrator** for reproducible developer environments on any platform — **without vendor lock-in, without SSH key setup, and with 5-10x faster builds on macOS/Windows**.

Unlike VS Code Dev Containers (locked to VS Code), dev-box works with **any SSH-capable editor**: Neovim, VS Code, JetBrains, Zed, Emacs, or plain terminal.

---

## 🚀 30-Second Demo

```bash
# 1. Create config
cat > devbox.ini << 'EOF'
[dev-environment]
name = "my-dev"
image = "ubuntu:24.04"
additional_packages = "git curl build-essential"
EOF

# 2. Boot environment (and wire up keyless SSH)
dev-box up

# 3. Connect from your IDE
ssh my-dev  # or use VS Code Remote-SSH, JetBrains Gateway, etc.
```

**That's it.** No SSH keys generated. No setup inside the box. Works on Linux, macOS, Windows.

---

## Why dev-box?

| Problem | Solution |
|---------|----------|
| **Locked into VS Code** | Works with ANY editor (Neovim, JetBrains, Zed, Emacs, VS Code) |
| **SSH key chaos** | Embedded SSH server (zero setup, no keys ever generated) |
| **10x slower builds** on macOS/Windows | Scratchpad Sync = native ext4 performance |
| **Different setup per OS** | Single config works on Linux/Mac/Windows |
| **Heavyweight tools** | Single 2MB binary, no bloat |
| **Vendor lock-in** | Open source, MIT licensed, Distrobox-based |

---

## ✨ Key Features

- **🎯 IDE-independent** — Neovim, VS Code, JetBrains, Zed, Emacs, plain SSH. Your choice.
- **🔑 Keyless SSH** — Embedded SSH server. Zero keys generated. Zero setup. Works with any SSH client.
- **⚡ Native filesystem speed** — Scratchpad Sync = 5-10x faster builds on Windows (WSL2) & macOS (Podman/Lima)
- **🖥️ Cross-platform config** — Single `devbox.ini` on Linux, macOS, Windows. No OS-specific branching.
- **🧩 Layered configuration** — System defaults → personal → project → local overrides. No Git conflicts.
- **📦 Zero bloat** — Single ~2MB static binary. Doesn't reinvent container runtimes (uses Distrobox + Podman/Docker).
- **🔓 No vendor lock-in** — Open source (MIT). Built on industry-standard tools (Distrobox, SSH, containers).

---

## 📊 How dev-box Compares

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ Feature                  │ dev-box │ VS Code Dev Containers │ Nix │ devenv  │
├─────────────────────────────────────────────────────────────────────────────┤
│ IDE-independent          │    ✅   │         ❌ VS Code      │  ✅ │   ✅   │
│ Embedded SSH (no setup)  │    ✅   │         ❌              │  ❌ │   ❌   │
│ Single binary            │    ✅   │         ❌              │  ❌ │   ❌   │
│ Cross-platform config    │    ✅   │         ✅              │  ❌ │   ✅   │
│ Layered config           │    ✅   │         ❌              │  ✅ │   ✅   │
│ 100% native speed (Win)  │    ✅   │         ❌              │  ~  │   ~    │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## 🎓 Quick Start

### Install (Choose One)

**Crates.io (for Rust developers)**:
```bash
cargo install dev-box
```

**Pre-built binary**:
```bash
# Linux
curl -L https://github.com/srikanthrayudu/dev-box/releases/download/latest/dev-box-linux-x86_64 \
  -o /usr/local/bin/dev-box && chmod +x /usr/local/bin/dev-box

# macOS (Intel)
curl -L https://github.com/srikanthrayudu/dev-box/releases/download/latest/dev-box-macos-x86_64 \
  -o /usr/local/bin/dev-box && chmod +x /usr/local/bin/dev-box

# macOS (Apple Silicon)
curl -L https://github.com/srikanthrayudu/dev-box/releases/download/latest/dev-box-macos-aarch64 \
  -o /usr/local/bin/dev-box && chmod +x /usr/local/bin/dev-box
```

**Build from source**:
```bash
git clone https://github.com/srikanthrayudu/dev-box
cd dev-box && cargo build --release
```

### Create Your First Environment

```bash
# Initialize
cat > devbox.ini << 'EOF'
[dev-environment]
name = "my-project-dev"
image = "ubuntu:24.04"
additional_packages = "git curl build-essential rust-all"
EOF

# Boot (creates container + wires up SSH)
dev-box up

# Connect from your IDE
# Option 1: Plain SSH
ssh my-project-dev

# Option 2: VS Code Remote-SSH
# Install extension, add to ~/.ssh/config (done by dev-box up)

# Option 3: JetBrains Gateway
# Connect via SSH, choose my-project-dev host

# Option 4: Neovim
# nvim scp://my-project-dev/path/to/project
```

### Try an Example

Clone one of our examples and run `dev-box up`:

- **[dev-box-example-rust](https://github.com/srikanthrayudu/dev-box-example-rust)** — Rust + Cargo + clippy (ready to code)
- **[dev-box-example-python](https://github.com/srikanthrayudu/dev-box-example-python)** — Python 3.12 + Poetry + ruff
- **[dev-box-example-node](https://github.com/srikanthrayudu/dev-box-example-node)** — Node.js + pnpm + TypeScript

---

## 📚 Documentation

- **[README](README.md)** — Full feature overview and configuration guide
- **[IDE Integration Guide](docs/ide-integration.md)** — Setup for VS Code, JetBrains, Zed, Neovim, Emacs
- **[Filesystem Performance Guide](docs/filesystem-performance.md)** — Benchmarks, architecture, optimization
- **[CONTRIBUTING](CONTRIBUTING.md)** — How to contribute (we welcome PRs!)

---

## 🏗️ How It Works

```
┌─────────────────────────────────────────────────────────────┐
│ Your IDE / SSH Client (VS Code, Cursor, JetBrains, Zed)   │
└──────────────────┬──────────────────────────────────────────┘
                   │ SSH ProxyCommand (stdio)
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ dev-box (embedded SSH server, keyless auth)                │
│ • Reads merged config (global + project + local)           │
│ • Wires up ProxyCommand (no TCP, no sshd)                  │
│ • Manages container lifecycle via HostTransport            │
└──────────────────┬──────────────────────────────────────────┘
                   │ distrobox enter <box>
                   ▼
┌─────────────────────────────────────────────────────────────┐
│ Linux Container (Distrobox)                                │
│ • Runs on: native Linux / WSL2 / Podman Machine / Lima     │
│ • Mounts: $HOME (git, ssh, dotfiles, API keys)            │
│ • Syncs: Project files via rsync (native ext4 speed)      │
└─────────────────────────────────────────────────────────────┘
```

**Why this is better**:
- ✅ No TCP listener (impossible to MITM)
- ✅ No SSH keys (kernel auth is sufficient)
- ✅ No setup inside container (nothing installed, generated, or modified)
- ✅ Any SSH client works (plain `ssh`, VS Code, JetBrains, scp, rsync, etc.)

---

## 🚀 What's Next?

### Short-term (v0.2)
- Real PTY support (full-screen interactive tools)
- Docker Compose backend (in addition to Distrobox)
- Improved error messages & diagnostics

### Medium-term (v0.3)
- GUI/TUI for easy config management
- Built-in telemetry (opt-in)
- Shell completions (bash, zsh, fish, PowerShell)

### Long-term (v1.0)
- Stable CLI interface
- Production-ready performance
- Official support policy

See [CHANGELOG.md](CHANGELOG.md) for detailed roadmap.

---

## 🤝 Contributing

We're actively seeking contributions! 

**Good first issues**:
- Platform-specific testing (macOS + Lima, Windows + WSL2)
- Documentation and guides
- Example repositories (Go, Java, Node.js templates)
- Test coverage improvements

See [CONTRIBUTING.md](CONTRIBUTING.md) for detailed guide and development setup.

**Ways to help**:
- ⭐ Star the repo (shows community interest)
- 🐛 Report bugs with reproduction steps
- 💬 Discuss ideas in [GitHub Discussions](https://github.com/srikanthrayudu/dev-box/discussions)
- 🔧 Submit PRs (new features, bug fixes, docs)

---

## 📊 Community

- **GitHub Issues** — Bug reports and feature requests
- **GitHub Discussions** — Questions, ideas, show & tell
- **LinkedIn** — [@srikanthrayudu](https://linkedin.com/in/srikanthrayudu)
- **Twitter/X** — [@srikanthrayudu](https://twitter.com/srikanthrayudu)

---

## 📋 Requirements

### Prerequisites
- [Distrobox](https://distrobox.it) + Podman or Docker
  - **Linux**: native installation
  - **macOS**: inside Podman Machine or Lima
  - **Windows**: inside WSL2
- OpenSSH client (just `ssh`, ships by default on Linux/macOS, optional on Windows)

### Supported Platforms
- ✅ Linux (any distro with Podman/Docker)
- ✅ macOS (Intel & Apple Silicon with Podman Machine or Lima)
- ✅ Windows (WSL2)

---

## 📖 Example Workflows

### Scenario 1: Team with Mixed Editors

**Team**: 5 developers (2 Neovim, 2 VS Code, 1 JetBrains)
**Problem**: Each editor needs different remote setup

**Solution**: Single `devbox.ini` committed to repo

```ini
[dev-environment]
name = "team-dev"
image = "ubuntu:24.04"
additional_packages = "git rustup cargo clippy"
forward_env = "GITHUB_TOKEN RUST_BACKTRACE"
```

Everyone runs `dev-box up` → `dev-box` auto-configures SSH → each dev connects from their editor of choice.

### Scenario 2: Mac User with Slow Builds

**Problem**: Cargo builds take 45 minutes on macOS (VirtioFS bottleneck)

**Solution**: Enable Scratchpad Sync

```ini
[dev-environment]
name = "fast-build"
image = "ubuntu:24.04"
scratchpad = true  # Native ext4 workspace on macOS VM
```

Result: Same project, **8-minute builds** (5-6x speedup)

### Scenario 3: Windows Developer Joining Linux Team

**Problem**: Windows dev wants same experience as Linux team

**Solution**: Same config, different host

Linux team:
```bash
dev-box up
dev-box enter  # Direct shell access
```

Windows dev (WSL2):
```bash
dev-box up
dev-box enter --scratchpad  # Use native ext4 for fast builds
# Or connect via SSH from VS Code
```

---

## 🔒 Security

- **No SSH keys**: Uses SSH `"none"` auth (kernel auth sufficient)
- **No network exposure**: SSH over stdio pipe (local process only)
- **No setup in container**: Nothing installed, generated, or modified
- **Host integration**: Leverages Distrobox's bind-mounts (same security model)

See [CONTRIBUTING.md](CONTRIBUTING.md#security) for security considerations.

---

## 📄 License

MIT License — see [LICENSE](LICENSE)

---

## 🙏 Acknowledgments

- [Distrobox](https://distrobox.it) — the orchestration foundation
- [russh](https://github.com/warp-tech/russh) — embedded SSH library
- The Rust and open-source communities

---

## 💬 Feedback?

Have ideas? Found a bug? Want to contribute?

- **GitHub Issues** → [bug reports & features](https://github.com/srikanthrayudu/dev-box/issues)
- **GitHub Discussions** → [Q&A, ideas, show & tell](https://github.com/srikanthrayudu/dev-box/discussions)
- **Direct message** → [@srikanthrayudu on LinkedIn/Twitter](https://linkedin.com/in/srikanthrayudu)

---

**Built with ❤️ in Rust. No vendor lock-in. No VC funding. Just a better way to develop.**

⭐ If dev-box helps you, please star the repo! It helps others discover it.
