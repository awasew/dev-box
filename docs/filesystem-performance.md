# Filesystem Performance & Cross-Boundary I/O (Windows / WSL2)

When developing inside Linux containers on Windows, filesystem performance is one of the most critical factors affecting developer experience. This document explains the root cause of cross-filesystem performance bottlenecks, how Microsoft's **VS Code Dev Containers** addresses the issue, and best practices and architectural solutions for **dev-box**.

---

## 1. The Root Cause: The Cross-Filesystem Penalty

On Windows, `dev-box` uses WSL2 (Windows Subsystem for Linux) to run Distrobox and Podman/Docker.

### The Slow Path (`/mnt/c/...`)

When your project files reside on the Windows NTFS filesystem (e.g. `C:\Users\<user>\projects\my-app`), WSL2 mounts this drive into the Linux environment under `/mnt/c/...`.

```
Windows NTFS (C:\...)
        ↕  9P Protocol / VirtioFS (network-like translation)
WSL2 Linux VM (/mnt/c/...)
        ↕  Bind mount
Distrobox Container
```

Every file operation (read, write, stat, chmod) across the `/mnt/c/` boundary must be translated between Linux filesystem semantics and the Windows NTFS kernel driver via the 9P or VirtioFS protocol.

- **Impact**: File I/O across `/mnt/c/` is **5x to 20x slower** than native Linux ext4.
- **Symptoms**:
  - `git status` taking several seconds on medium/large repos.
  - `npm install`, `cargo build`, or `pip install` taking orders of magnitude longer due to writing thousands of tiny files.
  - Language servers (rust-analyzer, tsserver, gopls) experiencing high CPU and slow indexing.

### The Fast Path (`/home/<user>/...`)

When files reside natively inside the WSL2 virtual disk (`ext4.vhdx`):

```
WSL2 Linux Virtual Disk (ext4)
        ↕  Native Linux VFS / Ext4
Distrobox Container
```

File operations run at **100% native Linux speed**.

---

## 2. How VS Code Dev Containers Solves This

VS Code's Dev Containers specification and CLI handle this issue through several complementary strategies:

### A. "Clone Repository in Container Volume"
Instead of bind-mounting an existing folder from the host:
1. VS Code creates an isolated, named Docker volume (e.g. `vsc-myrepo-12345-volume`).
2. Docker volumes live inside WSL2's native ext4 virtual disk image.
3. The repository is cloned directly inside that volume via a helper container.
4. VS Code attaches via remote RPC/SSH directly into the container. The Windows NTFS filesystem is never touched.

### B. "Clone Repository in WSL" Pattern
For developers who want host-side file access without Docker volume opacity, VS Code encourages storing projects inside WSL2:
- **Slow**: `C:\Users\<user>\projects\repo`
- **Fast**: `\\wsl$\Ubuntu\home\<user>\projects\repo`

When Docker runs inside WSL2, mounting `/home/<user>/projects/repo` into a container is a native Linux-to-Linux bind mount with zero translation overhead.

### C. Automatic Slow-Path Detection & UI Warnings
When opening a folder located on a Windows drive (e.g., `/mnt/c/`), Dev Containers detects the path and prompts the user:
> *"You are opening a container on a Windows filesystem which is known to be slow. We recommend cloning into a volume or moving it to WSL."*

### D. Selective Volume Mounts for Build Caches
In `devcontainer.json`, developers can isolate heavy write directories (`node_modules`, `target`, `.gradle`) onto fast, native named volumes while keeping source code mounted to the host:

```json
{
  "mounts": [
    "source=cargo-cache,target=${containerWorkspaceFolder}/target,type=volume",
    "source=node-cache,target=${containerWorkspaceFolder}/node_modules,type=volume"
  ]
}
```

---

---

## 3. The dev-box Solution: Native WSL2 Ext4 / RAM Scratchpad Sync

To give developers the best of both worlds—**editing natively on Windows with Windows tools** while compiling at **100% native Linux ext4 speed**—dev-box provides a built-in **Scratchpad Sync** layer.

```
┌─────────────────────────────────────────────────────────────┐
│                    WINDOWS HOST (C:\)                       │
│  Project Source Files (Edited natively on Windows)          │
└──────────────────────────────┬──────────────────────────────┘
                               │
            Differential Sync (native WSL rsync)
            Sub-millisecond latency across WSL boundary
                               │
┌──────────────────────────────▼──────────────────────────────┐
│                NATIVE WSL EXT4 RAM/RAMDISK                  │
│  `/tmp/dev-box/scratchpads/<box_name>/`                     │
│  • Primary workspace for Distrobox container                │
│  • Compiles at NATIVE 100% Linux ext4 speed                 │
└─────────────────────────────────────────────────────────────┘
```

### How It Works

1. **Host-to-Scratchpad Sync**:
   When you run `dev-box enter --scratchpad` (or declare `scratchpad = true` in `devbox.ini`), dev-box uses WSL's native `rsync` to mirror your project files into `/tmp/dev-box/scratchpads/<box_name>` on native Linux ext4/tmpfs.
2. **Automatic `.gitignore` & Artifact Isolation**:
   `rsync` automatically respects your `.gitignore` (`--filter=':- .gitignore'`) and excludes heavy build outputs (`target/`, `node_modules/`, `.git/`). Heavy compile artifacts stay 100% inside Linux ext4 and are never translated across 9P.
3. **Container Runs in Native Storage**:
   Distrobox enters directly into the scratchpad directory (`cd /tmp/dev-box/scratchpads/<box_name> && distrobox enter <box_name>`).
4. **Reverse Sync on Exit**:
   When you exit the session, dev-box automatically runs `rsync -au` to sync any updated source files (e.g. updated `Cargo.lock`, code generators, auto-fixes) back to your Windows `C:\` directory while preserving newer host edits.
5. **Manual / On-Demand Sync**:
   You can manually sync at any time:
   ```sh
   # Push Windows host changes to WSL scratchpad:
   dev-box sync

   # Pull WSL scratchpad changes back to Windows host:
   dev-box sync --reverse
   ```

### Enabling Scratchpad in Configuration

Add `scratchpad = true` to `devbox.ini`:

```ini
[dev-environment]
name = "my-project-dev"
image = "ubuntu:24.04"
scratchpad = true
```

---

## 4. Does macOS Also Suffer From This? (APFS vs VirtioFS vs Linux ext4)

**Yes, macOS suffers from the exact same cross-boundary architecture problem, though modern VirtioFS makes it less catastrophic than Windows 9P.**

### The Root Cause on macOS: The Hypervisor Boundary

Like Windows, **macOS cannot run Linux containers natively** because the host runs the Darwin/XNU kernel rather than the Linux kernel. Therefore, on macOS:
- Distrobox and Podman/Docker run inside a lightweight Linux virtual machine: **Podman Machine** or **Lima** (powered by Apple's `Virtualization.framework` or QEMU).
- Your project files reside on the Mac host's native APFS filesystem (`/Users/<user>/...`).
- The Linux VM accesses those host files across the hypervisor bridge via **VirtioFS**, **9P**, or reverse **sshfs**.

```
macOS Host APFS (/Users/...)
        ↕  VirtioFS / 9P (hypervisor bridge)
Podman Machine / Lima VM (/Users/...)
        ↕  Bind mount
Distrobox Container
```

### The macOS Performance Penalty

While VirtioFS on modern macOS (macOS 13+ with Apple Virtualization) is substantially faster than Windows 9P, writing and reading thousands of small files across the hypervisor still incurs a measurable penalty:

- **Heavy Write Workloads (`cargo build`, `npm install`)**: Writing intermediate compiler artifacts (`.o`, `.rlib`, `.js`) across VirtioFS is **2x to 5x slower** than compiling directly on native Linux ext4 inside the VM.
- **High-Metadata Operations (`git status`, IDE indexing)**: Repeated `stat` and `lstat` calls across VirtioFS incur hypervisor round-trip overhead.
- **File Locking Contention**: POSIX lock translation between Darwin APFS and Linux VFS can occasionally cause lock contention during parallel compiler passes.

### Cross-Platform Comparison Matrix

| Platform | Host Filesystem | VM Hypervisor Bridge | Heavy Compilation Penalty | Scratchpad Recommendation |
| :--- | :--- | :--- | :--- | :--- |
| **Linux** | Native ext4 / btrfs / zfs | *None (Native Linux Kernel)* | **0% (100% native speed)** | **Not needed** (bind mounts are already native speed) |
| **macOS** (Modern VirtioFS) | APFS | VirtioFS | **2x to 5x slower** | **Beneficial** (opt-in turbo boost for large Rust/C++/Node builds) |
| **macOS** (Older 9P / sshfs) | APFS | 9P / sshfs | **5x to 10x slower** | **Highly Recommended** |
| **Windows** | NTFS | 9P / VirtioFS (`/mnt/c/`) | **10x to 30x slower** | **Critical** (essential for usable compilation speeds) |

---

## 5. Comparison: dev-box Scratchpad vs Docker Named Volumes

| Feature | Docker Named Volume / Dev Container | dev-box WSL / VM Ext4 Scratchpad |
| :--- | :--- | :--- |
| **I/O Speed (`cargo build`, `npm`)** | Fast (Native ext4) | **Fast (Native ext4 / RAM)** |
| **Host Tooling Access** | **Restricted / Opaque** (Files locked inside Docker VHDX) | **Full Access** (Source code lives natively on host `C:\` or `/Users/`) |
| **Build Artifact Isolation** | Hard to inspect (buried in VM disk) | **Explicit & Clean** (`target/`, `node_modules/` stay isolated in VM) |
| **System Resource Usage** | **Heavy** (Docker Desktop VM daemon overhead) | **Lightweight** (Direct kernel syscalls, <2ms startup) |
| **Clean Up Lifecycle** | Manual (`docker volume prune`) | **Automatic on `dev-box rm`** |


