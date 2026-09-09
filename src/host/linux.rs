use super::{run_with_stdin, HostTransport};
use anyhow::{Context, Result};
use std::process::{Command, ExitStatus, Stdio};

/// Native Linux transport: runs commands directly through a shell, no
/// virtualization layer needed since distrobox runs natively here.
pub struct LinuxHost;

impl HostTransport for LinuxHost {
    fn name(&self) -> &'static str {
        "linux (native)"
    }

    fn run(&self, script: &str, stdin_payload: Option<&str>) -> Result<ExitStatus> {
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(script);
        run_with_stdin(cmd, stdin_payload)
    }

    fn capture(&self, script: &str) -> Result<String> {
        let output = Command::new("sh")
            .arg("-c")
            .arg(script)
            .output()
            .with_context(|| format!("failed to run: {script}"))?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn spawn_piped(&self, script: &str) -> Result<tokio::process::Child> {
        tokio::process::Command::new("sh")
            .arg("-c")
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("failed to spawn host process")
    }
}
