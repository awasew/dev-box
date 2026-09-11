# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Initial alpha release of dev-box
- IDE-independent developer environments on Distrobox
- Keyless SSH authentication (no keys generated, no sshd in container)
- Cross-platform configuration (Linux, macOS, Windows)
- Layered configuration merging (global → project → local)
- Scratchpad Sync for native filesystem performance (Windows WSL2 & macOS)
- Embedded SSH server using russh (no TCP exposure)
- HostTransport abstraction for platform-specific container access
- Support for Podman and Docker
- Environment variable forwarding for API keys and secrets
- Git credentials and SSH key integration via Distrobox bind-mounts
- Real pseudo-terminal (PTY) allocation for the embedded SSH server, so
  interactive full-screen programs (vim, htop, ...) and shell job control
  work correctly over `ssh <box>` (falls back to plain pipes for
  non-interactive/scripted sessions, matching OpenSSH semantics)
- Cross-process file locking around `~/.ssh/config` /
  `~/.ssh/dev-box_config` writes, so concurrent `dev-box` invocations for
  different boxes can't interleave and corrupt either file

### Documentation
- Architecture and design overview (`docs/architecture.md`), with diagrams
  covering the `HostTransport`/`ContainerEngine` split, config layering,
  and the `dev-box up` / interactive `ssh <box>` control flow
- IDE Integration Guide (VS Code, Cursor, JetBrains Gateway, Zed, Neovim)
- Filesystem Performance Guide (benchmarks and optimization tips)
- Configuration layering documentation
- Quick start guide

### Known Limitations
- Single ContainerEngine backend (Distrobox only)
- CLI interfaces subject to change before v1.0
- Limited Windows testing (alpha phase)

---

## Future Releases

### [0.2.0] - Planned
- Additional ContainerEngine backends (Docker Compose, Podman Compose)
- GUI/TUI for configuration and container lifecycle management
- Policy/lock layers for team and org-level configuration enforcement
- Improved error messages and diagnostics

### [0.3.0] - Planned
- Built-in telemetry (opt-in) for usage analytics
- Shell completion scripts (bash, zsh, fish, PowerShell)
- Integration with popular dev tools (Orbstack, Multipass)
- Automated backup/restore of scratchpad state

### [1.0.0] - Stable Release
- Stable CLI interface
- Complete test coverage (unit + integration)
- Production-ready performance
- Official support policy

---

## Release Notes Template

When creating a release, use this template:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- Feature description

### Fixed
- Bug fix description

### Changed
- Breaking change description

### Removed
- Deprecated feature removal

### Security
- Security fix description
```

---

**Contributing**: See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute changes.
