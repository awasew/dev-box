# dbx vs. Jetify Devbox

["Devbox" by Jetify](https://github.com/jetify-com/devbox) is a mature, popular (~12k stars)
open-source tool that solves a problem adjacent to `dbx`'s: reproducible developer
environments. Because both target developer environment orchestration, this document exists
to keep the comparison honest and in one place rather than scattered across READMEs and issues.

This is **not** a "which one is better" pitch. They use fundamentally different mechanisms
and make different tradeoffs; which one you want depends on what you're actually trying to
solve.

## Side-by-side

| | **Jetify Devbox** | **dbx** |
|---|---|---|
| **Isolation mechanism** | [Nix](https://nixos.org/) — symlinks packages from the immutable `/nix/store` into a local `.devbox` directory. It's a *virtual shell*: same kernel, same filesystem, just a curated `PATH`/environment. | Real Linux containers via [Distrobox](https://distrobox.it) (Podman/Docker). You get an actual separate root filesystem running a full distro userspace (Ubuntu, Fedora, Arch, ...), not just a curated shell. |
| **Package source** | The Nix Package Registry (400k+ pinned versions, extremely reproducible) | Whatever your chosen distro's own package manager provides (`apt`/`dnf`/`pacman`/`apk`) — less precisely pinned, but you get that distro's *entire* real ecosystem, including anything installable via a normal install script. |
| **Config format** | `devbox.json`, with an `include` field to compose from other files/plugins | Layered `.ini` cascade (system → user global → project → local override), auto-merged by file location; additive-list semantics for things like packages/hooks |
| **Reproducibility guarantee** | `devbox.lock` pins exact resolved Nix store paths | None yet — `image = "ubuntu:24.04"` is a floating tag, not a pinned digest (see [Ideas borrowed](#ideas-borrowed-from-devbox) below) |
| **Cross-platform story** | Runs wherever Nix runs (Linux, macOS natively; Nix on WSL2 for Windows) | Bridges each OS to a Linux layer where Distrobox actually runs: native on Linux, Podman Machine/Lima on macOS, WSL2 on Windows |
| **Missing-dependency handling** | Auto-installs Nix if it's not already present | Fails with an install link; no auto-install yet |
| **IDE / remote connectivity** | None built in. `devbox generate` can emit a `devcontainer.json`/Dockerfile, but then you're back to needing VS Code's own Dev Containers extension. Jetify's actual "remote dev in the cloud" is a separate, hosted, paid product — not part of the open-source CLI. | Embeds its own tiny SSH server directly in the binary, so any Remote-SSH-capable IDE (VS Code, JetBrains Gateway, Neovim, plain `ssh`) can attach with zero extension and zero manual key setup. |
| **Language / maturity** | Go, ~12k stars, backed by a company, production-used | Rust, fast and lightweight, single static binary |
| **License** | Apache-2.0 | MIT |

## The honest summary

Devbox solves "give me the exact same tool *versions* everywhere" via Nix's package-level
reproducibility — it's mature and excellent at that.

`dbx` solves a different problem: "give me a full, real, isolated Linux *container* I can
just SSH into from any IDE, with zero container-side setup and zero SSH keys."

The container-vs-Nix-shell distinction and the embedded keyless-SSH bridge are genuinely
different mechanisms, not a rename of the same idea. There's a real reason to pick one over
the other — e.g. you need an entirely different distro's toolchain intact (pick `dbx`), or
you want bit-for-bit pinned tool versions without touching containers at all (pick Devbox).

## Ideas borrowed from Devbox

Reading Devbox's design surfaced a few genuinely good ideas worth adapting into `dbx`,
even though the underlying mechanism is different. None of these are implemented yet; they're
tracked here so they don't get lost, and cross-referenced from the "What's Next?" roadmap
section in the main [README](../README.md).

1. **Pin the resolved image, not just a floating tag.**
   Devbox's `devbox.lock` guarantees the *exact* environment you tested is the one you get
   later. `dbx`'s `image = "ubuntu:24.04"` is a moving target. A `dbx.lock`
   (or a `[dev-environment] image_digest=` field, written automatically after a successful
   `dbx up`) would let `dbx up` reproduce the exact same image on a teammate's
   machine, while still letting `dbx up --pull`/similar intentionally refresh it.

2. **Auto-detect and offer to install missing dependencies.**
   Devbox bootstraps Nix automatically if it's missing. `dbx` currently just prints an
   install link and exits. A `dbx doctor` (or automatic prompt on first `up`) that
   detects the missing piece per-platform (Distrobox, Podman/Docker, WSL2, Podman
   Machine/Lima) and either offers to run the platform's install command or prints the
   *exact* one to run, would meaningfully lower the "day one" friction Devbox is clearly
   optimized for.

3. **A literal, declarative `env` section, distinct from `forward_env`.**
   `dbx`'s `forward_env` intentionally only ever carries variable *names* (the values
   come live from the host shell, for secrets). Devbox's `devbox.json` also has a plain
   `"env": {...}` map for *non-secret* values you want baked directly into the environment
   (e.g. `EDITOR=vim`, `NODE_ENV=development`). `dbx` has no equivalent today short of an
   `init_hooks` shell command; a small additive `env = "KEY=VALUE"` list (merged the same way
   as `additional_packages`) would cover this cleanly without overloading `forward_env`'s
   security model.

4. **An explicit `include=` composition option, alongside the automatic file-cascade.**
   `dbx`'s N-layer cascade is automatic and file-location-based (global → project →
   local). Devbox's `devbox.json` additionally supports an explicit `include` list, letting
   one config pull in a *named* preset regardless of file location. A `[dev-environment]
   include = "rust-base"` style option (resolving against a small library of shipped
   presets, e.g. `presets/rust.ini`, `presets/python.ini`) would make sharing common stacks
   across unrelated projects easier than copy-pasting `dbx.ini` fragments.

5. **An export path back to plain Dev Containers / Dockerfile, for teams that need it.**
   `devbox generate` can emit a `devcontainer.json` or `Dockerfile` from the same declarative
   source, so teams aren't fully locked into Devbox's own runtime. A `dbx export
   --devcontainer` / `dbx export --dockerfile`, translating the merged INI into those
   formats, would give `dbx` users the same escape hatch — useful for onboarding a
   teammate who's stuck on plain VS Code Dev Containers for now.

6. **Community/contribution scaffolding.**
   Devbox ships a `CODE_OF_CONDUCT.md`, `CONTRIBUTING.md`, and a "Related Work" section
   crediting Nix. `dbx` should do the same as it grows past a single-maintainer
   prototype: a `CONTRIBUTING.md`, a `CODE_OF_CONDUCT.md`, and this document doubling as the
   "Related Work" / prior-art section.

Items 1–3 are the most self-contained and highest-value; 4–6 are more about long-term project
health and can wait until there's real usage to justify them.
