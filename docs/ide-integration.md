# IDE Integration & Keyless SSH Guide

**dev-box** automates the container developer experience without requiring proprietary extensions (like Microsoft's Dev Containers extension), Docker Desktop, or running a bloated SSH daemon inside your container.

Any editor or IDE that supports remote development over SSH connects seamlessly.

---

## 1. How It Works: The Keyless SSH Proxy

When you run `dev-box up`:
1. Distrobox builds/updates your container from your layered INI configuration.
2. `dev-box` automatically adds an `Include dev-box_config` directive to `~/.ssh/config` and manages an isolated block in `~/.ssh/dev-box_config`:

```sshconfig
# >>> dev-box: my-project-dev >>>
Host my-project-dev
    ProxyCommand "path/to/dev-box" -c "path/to/devbox.ini" ssh-proxy my-project-dev
    StrictHostKeyChecking no
    UserKnownHostsFile /dev/null
    BatchMode yes
    LogLevel ERROR
# <<< dev-box: my-project-dev <<<
```

### Why There Are Zero Credentials or Keys
When your IDE or command line runs `ssh my-project-dev`:
- OpenSSH invokes `dev-box ssh-proxy my-project-dev` as a subprocess (`ProxyCommand`).
- `dev-box` embeds a minimal SSH server (`russh`) that speaks the SSH protocol directly over standard input/output (`stdio`).
- OpenSSH automatically probes with the standard SSH `"none"` authentication method. `dev-box` accepts `"none"` unconditionally.
- **Safety guarantee**: There is no TCP port, no open network socket, and no remote access. The proxy can only be executed by your local user account.
- **Container hygiene**: Nothing is installed inside your container (`sshd` is not installed, no keys are generated or stored on disk). The proxy simply spawns `distrobox enter <box>` and pipes stdio into the SSH channel.

---

## 2. Connecting Your IDE

### Visual Studio Code

1. Ensure the **Remote - SSH** extension (`ms-vscode-remote.remote-ssh`) is installed (standard open SSH extension, NOT the Dev Containers extension).
2. Press `F1` (or `Ctrl+Shift+P` / `Cmd+Shift+P`), type:
   ```text
   Remote-SSH: Connect to Host...
   ```
3. Select your container name (e.g. `my-project-dev`).
4. VS Code connects immediately with zero password prompts and bootstraps its server inside the container.
5. Open your workspace folder (`File -> Open Folder`):
   - **Linux**: `/home/<user>/projects/<repo>`
   - **macOS**: `/Users/<user>/projects/<repo>`
   - **Windows (standard)**: `/mnt/c/Users/<user>/...`
   - **Windows (scratchpad)**: `/tmp/dev-box/scratchpads/<box_name>`

#### Command Line Shortcut
You can open VS Code directly attached to your box from your terminal:
```sh
code --remote ssh-remote+my-project-dev /path/to/workspace
```

---

### Cursor

Cursor uses the standard Remote-SSH architecture:
1. Press `Ctrl+Shift+P` / `Cmd+Shift+P` -> `Remote-SSH: Connect to Host...`.
2. Select `<box_name>`.
3. Or launch directly via CLI:
   ```sh
   cursor --remote ssh-remote+my-project-dev /path/to/workspace
   ```

---

### JetBrains Gateway (IntelliJ IDEA, PyCharm, CLion, GoLand)

1. Open **JetBrains Gateway**.
2. Select **SSH Connection** -> **New Connection**.
3. In **Host**, enter your box name (e.g. `my-project-dev`).
4. Set **Port** to `22` (ignored by OpenSSH due to `ProxyCommand`).
5. Set **Authentication type** to **None** or **OpenSSH config and authentication agent**.
6. Click **Check Connection and Continue**. JetBrains will automatically deploy its backend inside the Distrobox container.

---

### Zed

Zed supports native SSH remoting:
```sh
zed ssh://my-project-dev/path/to/workspace
```

---

### Neovim / Terminal / Plain SSH

Connect directly with your native terminal shell:
```sh
ssh my-project-dev
```
Or run specific commands directly inside the box:
```sh
ssh my-project-dev cargo build --release
ssh my-project-dev git status
```

---

## 3. Port Forwarding

Any tool or web server running inside your container (e.g., a web server on `http://localhost:3000` or `http://localhost:8080`) can be reached from your host browser.

- In **VS Code / Cursor / JetBrains**: Remote-SSH automatically detects listening TCP sockets inside the box and forwards them to `http://localhost:<port>` on your host machine.
- Manually via plain SSH:
  ```sh
  ssh -L 3000:localhost:3000 my-project-dev
  ```

---

## 4. Lifecycle Cleanup

When you delete a container with `dev-box rm`:
```sh
dev-box rm
```
`dev-box` automatically:
1. Stops and removes the Distrobox container.
2. Removes the `Host <box_name>` block from `~/.ssh/dev-box_config`.
3. Cleans up any ext4 scratchpad directories in WSL2.
No dangling SSH config entries or orphaned volumes are left behind.
