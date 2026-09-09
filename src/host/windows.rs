use super::{run_with_stdin, HostTransport};
use anyhow::{bail, Context, Result};
use std::process::{Command, ExitStatus, Stdio};

/// Windows can't run Linux containers natively, so dev-box bridges into
/// WSL2, where distrobox and its backend (Podman/Docker) actually run.
pub struct WindowsHost;

impl HostTransport for WindowsHost {
    fn name(&self) -> &'static str {
        "windows (WSL2)"
    }

    fn run(&self, script: &str, stdin_payload: Option<&str>) -> Result<ExitStatus> {
        if which::which("wsl.exe").is_err() {
            bail!(
                "dev-box requires WSL2 on Windows. Install it with `wsl --install` and try again."
            );
        }
        let mut cmd = Command::new("wsl.exe");
        cmd.args(["-e", "sh", "-c", script]);
        run_with_stdin(cmd, stdin_payload)
    }

    fn capture(&self, script: &str) -> Result<String> {
        let output = Command::new("wsl.exe")
            .args(["-e", "sh", "-c", script])
            .output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn spawn_piped(&self, script: &str) -> Result<tokio::process::Child> {
        if which::which("wsl.exe").is_err() {
            bail!(
                "dev-box requires WSL2 on Windows. Install it with `wsl --install` and try again."
            );
        }
        tokio::process::Command::new("wsl.exe")
            .args(["-e", "sh", "-c", script])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn host process")
    }
}
