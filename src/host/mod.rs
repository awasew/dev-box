pub mod linux;
pub mod macos;
pub mod windows;

use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, ExitStatus, Stdio};

/// Abstracts *how* dev-box reaches the Linux layer that actually runs
/// distrobox: natively on Linux, via WSL2 on Windows, and via Podman
/// Machine/Lima on macOS.
///
/// Requires `Send + Sync` so implementations can be shared (via `Arc`)
/// with the async SSH server task in `crate::sshd`.
pub trait HostTransport: Send + Sync {
    /// Human-readable description of the active transport, for diagnostics.
    fn name(&self) -> &'static str;

    /// Runs `script` on the host, optionally piping `stdin_payload` into it.
    /// stdout/stderr are inherited so output streams live to the user.
    fn run(&self, script: &str, stdin_payload: Option<&str>) -> Result<ExitStatus>;

    /// Runs `script` on the host and captures trimmed stdout as a String.
    /// Used for cheap existence checks (e.g. `command -v distrobox`).
    fn capture(&self, script: &str) -> Result<String>;

    /// Spawns `script` with fully piped stdio (stdin/stdout/stderr), for
    /// programmatically pumping data between an SSH channel and the box's
    /// shell. Used exclusively by the embedded SSH server in `crate::sshd`.
    fn spawn_piped(&self, script: &str) -> Result<tokio::process::Child>;
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
        compile_error!("dev-box does not support this platform");
    }
}

/// Shared helper: spawns `cmd`, optionally writes `stdin_payload` into its
/// stdin, inherits stdout/stderr, and waits for completion.
pub(crate) fn run_with_stdin(mut cmd: Command, stdin_payload: Option<&str>) -> Result<ExitStatus> {
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
