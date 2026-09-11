# Architecture

This document explains how dev-box's codebase is organized, why it's organized that
way, and how to extend it (new platform, new container backend, new command) without
fighting the grain of the design.

If you only read one section, read [The two-axis design](#the-two-axis-design) — it's
the single idea that makes the rest of the codebase predictable.

---

## Goals that shaped the design

- **IDE-independent**: nothing here should assume a specific editor. The only
  "protocol" dev-box exposes to the outside world is SSH.
- **Cross-platform without `#[cfg]` soup**: platform differences (Linux native, WSL2
  on Windows, Podman Machine/Lima on macOS) are isolated behind one trait, not
  scattered through `if cfg!(windows)` checks in business logic.
- **No lock-in to one container tool**: Distrobox is the only backend today, but
  nothing above the `ContainerEngine` trait should need to change if a second backend
  (e.g. Docker Compose) shows up later.
- **Small, single binary**: no daemon, no background service. Every command is a
  short-lived process; the only long-lived process is `ssh-proxy`, and only for the
  duration of one SSH session.

---

## Module map

```
src/
  main.rs        Entry point: parses CLI, builds AppContext, dispatches commands
  cli.rs         clap argument/subcommand definitions (no logic)
  context.rs     AppContext: merged config + detected host + selected engine
  util.rs        Small stateless helpers (quote-stripping, path resolution)

  config/        Layered INI parsing & merging (global -> project -> local)
  engine/        *What* container tool to drive
    mod.rs         ContainerEngine trait
    distrobox.rs   The only implementation today
  host/          *How* to run a command on this OS
    mod.rs         HostTransport trait + shared default methods
    linux.rs       Native Linux adapter
    windows.rs     WSL2 adapter
    macos.rs       Podman Machine / Lima adapter
    pty.rs         Cross-platform pseudo-terminal bridge (portable_pty <-> tokio)
    scratchpad.rs  Windows/macOS native-filesystem sync (rsync-based)
  sshd/          Embedded SSH server (keyless, no TCP listener)
    mod.rs         Server bootstrap + ~/.ssh/config management
    handler.rs     Per-session protocol handling (shell/exec/pty/data/...)

tests/
  cli.rs                     Hermetic black-box CLI tests (no container runtime needed)
  distrobox_integration.rs   Opt-in (#[ignore]) tests against a real Distrobox install
```

---

## The two-axis design

Every command dev-box runs has to answer two *independent* questions:

1. **What tool manages the container?** (today: Distrobox)
2. **How do I even run a shell command on this machine?** (native shell / WSL2 /
   a VM)

These are modeled as two separate traits so that neither axis knows the other exists:

```mermaid
flowchart TB
    subgraph engine_axis["ContainerEngine -- WHAT to drive"]
        CE[ContainerEngine trait]
        Distrobox[distrobox.rs]
        FutureEngine[future: docker-compose.rs]
        CE --> Distrobox
        CE --> FutureEngine
    end
    subgraph host_axis["HostTransport -- HOW to run a command"]
        HT[HostTransport trait]
        Linux[linux.rs]
        Windows[windows.rs]
        Macos[macos.rs]
        HT --> Linux
        HT --> Windows
        HT --> Macos
    end
    AppContext --> engine_axis
    AppContext --> host_axis
```

`AppContext` (`context.rs`) picks one implementation of each trait at startup and
hands both to every command. A `ContainerEngine` never spawns a process directly —
it builds a *script string* (e.g. `"distrobox enter my-box"`) and asks whichever
`HostTransport` is active to actually run it:

```rust
// engine/distrobox.rs
fn enter(&self, host: &dyn HostTransport, box_name: &str, ...) -> Result<()> {
    let script = self.enter_script(box_name, None, forwarded_env, work_dir);
    host.run(&script, None)?;   // <- engine doesn't know or care if this is
    Ok(())                       //    sh -c, wsl.exe -e sh -c, or podman machine ssh
}
```

Because of this split, **any engine works on any host for free**. Adding a Docker
Compose backend wouldn't touch `host/` at all; adding a new platform wouldn't touch
`engine/` at all.

### `HostTransport`: one required method, four free ones

Every platform adapter used to duplicate `run`/`capture`/`spawn_piped` almost
verbatim, with only the concrete command differing. That's now collapsed: the trait
has exactly **one** thing each platform must define, and everything else is a default
method built on top of it.

```mermaid
flowchart TD
    CP["command_parts(script) -> (program, args)\n(the only platform-specific method)"]
    CP --> Run["run() -- inherits stdio, waits for exit"]
    CP --> Capture["capture() -- trims stdout, for tool-existence checks"]
    CP --> SpawnPiped["spawn_piped() -- plain OS pipes, for non-interactive SSH sessions"]
    CP --> SpawnPty["spawn_pty() -- real pseudo-terminal, for interactive SSH sessions"]
```

| Platform | `command_parts("distrobox list")` returns |
|---|---|
| Linux (native) | `("sh", ["-c", "distrobox list"])` |
| Windows (WSL2) | `("wsl.exe", ["-e", "sh", "-c", "distrobox list"])` |
| macOS (Podman Machine) | `("podman", ["machine", "ssh", "distrobox list"])` |
| macOS (Lima) | `("limactl", ["shell", "default", "sh", "-c", "distrobox list"])` |

`macos.rs` is the one adapter that's an `enum` instead of a unit struct, because it's
the only platform that has to pick *between* two backends at runtime
(`MacHost::detect()` probes for `podman`, falls back to `limactl`). That's inherent to
the platform, not an inconsistency in the design.

---

## Configuration layering

`devbox.ini` is deliberately INI, not YAML/TOML/JSON, because it merges predictably
and diffs cleanly in code review. Layers are merged in ascending priority — later
layers win on scalar keys, and specific keys (`additional_packages`, `init_hooks`,
`exported_apps`, `forward_env`) are *additive* (space-joined) instead of overwritten:

```mermaid
flowchart TD
    Global["~/.config/dev-box/global.ini\n(user defaults, machine-wide)"]
    Project["./devbox.ini\n(committed to the repo)"]
    Local["./devbox.local.ini\n(gitignored, personal overrides)"]
    Merged["merged Ini\n(config::merge_layers)"]
    Global --> Merged
    Project --> Merged
    Local --> Merged
    Merged --> Payload["distrobox assemble create --file /dev/stdin"]
```

`--config <path>` (repeatable, in `cli.rs`) replaces this default cascade entirely,
which is also how `sshd::install_client_config` pins the exact layer set into the
generated `ProxyCommand` — so `ssh <box>` run by an IDE from an arbitrary working
directory always resolves the same configuration `dev-box up` used.

---

## Command flow: `dev-box up`

```mermaid
sequenceDiagram
    participant User
    participant Main as main.rs
    participant Cfg as config::merge_layers
    participant Eng as ContainerEngine
    participant Host as HostTransport
    participant Sshd as sshd::install_client_config

    User->>Main: dev-box up
    Main->>Cfg: merge_layers(layers)
    Cfg-->>Main: merged Ini
    Main->>Eng: assemble(host, payload)
    Eng->>Host: run("distrobox assemble create --file /dev/stdin", payload)
    Host-->>Eng: exit status
    Main->>Sshd: install_client_config(box_name, layers)
    Sshd-->>Main: ~/.ssh/config + ~/.ssh/dev-box_config updated
    Main-->>User: "connect with: ssh box_name"
```

`install_client_config` acquires an OS file lock (`sshd::with_ssh_config_lock`) before
touching either file, so two `dev-box up`/`dev-box rm` invocations for different boxes
never interleave their read-modify-write of the same config files.

---

## Command flow: interactive `ssh <box>`

This is the part of the system with the least visible control flow, since it's driven
by whatever the SSH client decides to send. `ssh-proxy` never opens a TCP listener —
it's spawned locally as an SSH `ProxyCommand` and speaks the protocol over its own
stdin/stdout.

```mermaid
sequenceDiagram
    participant Client as SSH client (e.g. IDE)
    participant Proxy as dev-box ssh-proxy
    participant Host as HostTransport
    participant Box as distrobox container

    Client->>Proxy: spawned via ProxyCommand (stdio pipe, no network)
    Proxy->>Client: auth_none -> Accept (see sshd/handler.rs docs for why)
    Client->>Proxy: pty_request(cols, rows) [only for interactive sessions]
    Proxy->>Proxy: pending_pty.insert(channel, size)
    Client->>Proxy: shell_request (or exec_request)
    alt pty was requested
        Proxy->>Host: spawn_pty(enter_script, size)
        Host->>Box: distrobox enter box_name  (attached to a real pty)
    else no pty (plain scripted `ssh box cmd`)
        Proxy->>Host: spawn_piped(enter_script)
        Host->>Box: distrobox enter box_name -- sh -c cmd  (plain pipes)
    end
    Box-->>Proxy: stdout/stderr
    Proxy-->>Client: channel data
    Client->>Proxy: data (keystrokes / stdin)
    Proxy->>Box: forwarded to the child's stdin (pipe or pty)
    Box-->>Proxy: exit status
    Proxy-->>Client: exit_status_request, eof, close
```

The pty-vs-piped choice happens once per channel and mirrors real OpenSSH semantics:

```mermaid
flowchart TD
    Start["shell_request / exec_request"] --> Check{"pty_request seen\nfor this channel?"}
    Check -- yes --> Pty["spawn_child_pty\n(host::pty, real pseudo-terminal)"]
    Check -- no --> Piped["spawn_child\n(plain OS pipes)"]
```

A pty is only allocated when the client actually asks for one — an interactive
`ssh <box>` or `ssh -t <box> cmd` gets `vim`/`htop`/job control working correctly; a
plain scripted `ssh <box> cmd` (no `-t`) stays on the lighter, non-interactive pipe
path, exactly like talking to a real `sshd`.

---

## Extending dev-box

### Add a new platform (e.g. FreeBSD, or a remote-SSH-host transport)

1. Create `src/host/<platform>.rs` implementing `HostTransport`:

   ```rust
   use super::HostTransport;
   use anyhow::Result;

   pub struct MyPlatformHost;

   impl HostTransport for MyPlatformHost {
       fn name(&self) -> &'static str {
           "my-platform"
       }

       fn command_parts(&self, script: &str) -> Result<(String, Vec<String>)> {
           Ok(("sh".to_string(), vec!["-c".to_string(), script.to_string()]))
       }
   }
   ```

   That's it — `run`, `capture`, `spawn_piped`, and `spawn_pty` (real PTY support) all
   come for free from the default methods in `host/mod.rs`. Only override them if your
   platform genuinely can't support one (e.g. no pty concept at all).

2. Register it in `host::detect()` behind the right `#[cfg(target_os = "...")]`.
3. Add a unit test for `command_parts` (see `host/linux.rs`'s neighbors for the
   pattern) and, if the platform is available in CI, let `tests/cli.rs` exercise it
   for free (those tests are already platform-agnostic).

### Add a new container backend (e.g. Docker Compose)

1. Create `src/engine/<backend>.rs` implementing `ContainerEngine`
   (`name`, `is_available`, `assemble`, `enter`, `enter_script`, `list`, `stop`, `rm`).
   Use `engine/distrobox.rs` as the template — note it never touches `std::process`
   directly, only ever calls `host.run(...)`/`host.capture(...)` with a script string.
2. Pick the engine in `AppContext::new` (`context.rs`) — today it's hardcoded to
   `DistroboxEngine`; a multi-backend future would read a config key here instead.
3. No changes needed anywhere in `host/` or `sshd/` — both only depend on the
   `ContainerEngine` trait, never on `DistroboxEngine` specifically.

### Add a new CLI command

1. Add a variant to `Command` in `cli.rs` (clap derives the parser from the enum).
2. Add a matching arm in `main.rs`'s `match cli.command`. Reach for `ctx.host`,
   `ctx.engine`, `ctx.config` — `AppContext` already has everything most commands need.

---

## Testing strategy

| Layer | Where | What it covers | Needs a container runtime? |
|---|---|---|---|
| Unit tests | `#[cfg(test)]` modules next to the code (`config/mod.rs`, `util.rs`, `engine/distrobox.rs`, `sshd/mod.rs`, `host/pty.rs`) | Pure logic: INI merging, shell quoting, SSH-config block insert/remove, pty size clamping | No |
| CLI black-box tests | `tests/cli.rs` | `dev-box config`, `dev-box up --dry-run`, argument validation -- spawns the real binary, never touches Distrobox | No |
| Integration tests | `tests/distrobox_integration.rs` (`#[ignore]`d by default) | Real `up`/`list`/`rm` round-trip, including the `~/.ssh/*` side effects | Yes |

Run everything hermetic with `cargo test`. Run the real thing with
`cargo test --test distrobox_integration -- --ignored --test-threads=1` once
Distrobox + Podman/Docker (or WSL2/Podman Machine/Lima) are installed. CI's
`integration` job (`.github/workflows/ci.yml`) does exactly that on a dedicated
Ubuntu runner, separately from the platform matrix that gates merges.

---

## Non-goals (for now)

- **No daemon.** Every command is a fresh process. If dev-box ever needs persistent
  state beyond `~/.ssh/config`, that's a deliberate architecture change, not an
  incremental one.
- **No plugin system.** Both `HostTransport` and `ContainerEngine` are closed sets of
  Rust types chosen at compile time via `#[cfg]`/hardcoding, not a dynamically loaded
  plugin API. That's intentional for a single ~2MB static binary with no runtime
  dependency resolution.
