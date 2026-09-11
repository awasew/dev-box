//! Platform transports: *how* dbx reaches the Linux layer that
//! actually runs distrobox.
//!
//! This module is the "ports and adapters" seam of the whole codebase.
//! `HostTransport` is the port (shared core); `linux`, `windows`, and
//! `macos` are the adapters (platform-specific edges). Every adapter
//! only has to implement two things:
//!
//! * `name()` -- a human-readable label, for diagnostics.
//! * `command_parts(script)` -- how to turn a POSIX shell script into a
//!   concrete `(program, args)` invocation on this host (e.g. `("sh",
//!   ["-c", script])` natively, `("wsl.exe", ["-e", "sh", "-c",
//!   script])` on Windows, `("podman", ["machine", "ssh", script])` on
//!   macOS).
//!
//! Everything else -- `run`, `capture`, `spawn_piped`, `spawn_pty` -- is
//! a *default* trait method implemented once, here, on top of
//! `command_parts`. Adding a new platform (or a wholly different
//! transport, e.g. a remote SSH host) never means re-implementing "how
//! do I run a script and stream its output" -- just describing the one
//! command that does it.

pub mod linux;
pub mod macos;
pub mod pty;
pub mod scratchpad;
pub mod windows;

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, ExitStatus, Stdio};

/// Abstracts *how* dbx reaches the Linux layer that actually runs
/// distrobox: natively on Linux, via WSL2 on Windows, and via Podman
/// Machine/Lima on macOS.
///
/// Requires `Send + Sync` so implementations can be shared (via `Arc`)
/// with the async SSH server task in `crate::sshd`.
pub trait HostTransport: Send + Sync {
    /// Human-readable description of the active transport, for diagnostics.
    fn name(&self) -> &'static str;

    /// Returns the `(program, args)` pair that executes `script` on this
    /// host. This is the *only* thing a platform adapter has to define;
    /// every other method on this trait is a default built on top of it.
    fn command_parts(&self, script: &str) -> Result<(String, Vec<String>)>;

    /// Runs `script` on the host, optionally piping `stdin_payload` into
    /// it. stdout/stderr are inherited so output streams live to the user.
    fn run(&self, script: &str, stdin_payload: Option<&str>) -> Result<ExitStatus> {
        let (program, args) = self.command_parts(script)?;
        let mut cmd = Command::new(program);
        cmd.args(args);
        run_with_stdin(cmd, stdin_payload)
    }

    /// Runs `script` on the host and captures trimmed stdout as a String.
    /// Used for cheap existence checks (e.g. `command -v distrobox`).
    fn capture(&self, script: &str) -> Result<String> {
        let (program, args) = self.command_parts(script)?;
        let output = Command::new(program)
            .args(args)
            .output()
            .with_context(|| format!("failed to run: {script}"))?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// Spawns `script` with fully piped stdio (stdin/stdout/stderr), for
    /// programmatically pumping data between an SSH channel and the box's
    /// shell. Used exclusively by the embedded SSH server in `crate::sshd`
    /// for sessions that didn't request a pty.
    fn spawn_piped(&self, script: &str) -> Result<tokio::process::Child> {
        let (program, args) = self.command_parts(script)?;
        tokio::process::Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn host process")
    }

    /// Spawns `script` attached to a freshly-allocated pseudo-terminal of
    /// the given `size` instead of plain pipes, so interactive
    /// full-screen programs (vim, htop, ...) and shell job control work
    /// correctly over the embedded SSH server. Used for sessions that
    /// requested a pty (the normal case for interactive `ssh <box>`).
    fn spawn_pty(&self, script: &str, size: pty::PtySize) -> Result<pty::PtyChild> {
        let (program, args) = self.command_parts(script)?;
        pty::spawn(&program, &args, size)
    }
}

/// Detects the current platform and returns the appropriate transport.
pub fn detect() -> Box<dyn HostTransport> {
    #[cfg(target_os = "linux")]
    {
        return Box::new(linux::LinuxHost);
    }
    #[cfg(target_os = "windows")]
    {
        return Box::new(windows::WindowsHost);
    }
    #[cfg(target_os = "macos")]
    {
        return Box::new(macos::MacHost::detect());
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        compile_error!("dbx does not support this platform");
    }
}

/// Shared helper: spawns `cmd`, optionally writes `stdin_payload` into its
/// stdin, inherits stdout/stderr, and waits for completion.
fn run_with_stdin(mut cmd: Command, stdin_payload: Option<&str>) -> Result<ExitStatus> {
    if stdin_payload.is_some() {
        cmd.stdin(Stdio::piped());
    }
    cmd.stdout(Stdio::inherit()).stderr(Stdio::inherit());

    let mut child = cmd.spawn().context("failed to spawn host process")?;
    if let Some(payload) = stdin_payload {
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(payload.as_bytes())
                .context("failed to write to child stdin")?;
        }
    }
    Ok(child.wait()?)
}
