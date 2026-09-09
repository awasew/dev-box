use super::{run_with_stdin, HostTransport};
use anyhow::{bail, Context, Result};
use std::process::{Command, ExitStatus, Stdio};

/// macOS can't run Linux containers natively, so dev-box bridges into a
/// lightweight Linux VM: Podman Machine (preferred) or Lima as a fallback.
pub enum MacHost {
    PodmanMachine,
    Lima,
}

impl MacHost {
    pub fn detect() -> Self {
        if which::which("podman").is_ok() {
            MacHost::PodmanMachine
        } else if which::which("limactl").is_ok() {
            MacHost::Lima
        } else {
            // Default to Podman Machine; `run` surfaces a clear install
            // error if neither binary is actually present.
            MacHost::PodmanMachine
        }
    }

    fn wrap(&self, script: &str) -> Command {
        match self {
            MacHost::PodmanMachine => {
                let mut cmd = Command::new("podman");
                cmd.args(["machine", "ssh", script]);
                cmd
            }
            MacHost::Lima => {
                let mut cmd = Command::new("limactl");
                cmd.args(["shell", "default", "sh", "-c", script]);
                cmd
            }
        }
    }

    fn binary(&self) -> &'static str {
        match self {
            MacHost::PodmanMachine => "podman",
            MacHost::Lima => "limactl",
        }
    }
}

impl HostTransport for MacHost {
    fn name(&self) -> &'static str {
        match self {
            MacHost::PodmanMachine => "macOS (podman machine)",
            MacHost::Lima => "macOS (lima)",
        }
    }

    fn run(&self, script: &str, stdin_payload: Option<&str>) -> Result<ExitStatus> {
        if which::which(self.binary()).is_err() {
            bail!(
                "macOS requires Podman Machine or Lima to run distrobox. Install one of them and try again."
            );
        }
        run_with_stdin(self.wrap(script), stdin_payload)
    }

    fn capture(&self, script: &str) -> Result<String> {
        let output = self.wrap(script).output()?;
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn spawn_piped(&self, script: &str) -> Result<tokio::process::Child> {
        if which::which(self.binary()).is_err() {
            bail!(
                "macOS requires Podman Machine or Lima to run distrobox. Install one of them and try again."
            );
        }
        let mut cmd = match self {
            MacHost::PodmanMachine => {
                let mut c = tokio::process::Command::new("podman");
                c.args(["machine", "ssh", script]);
                c
            }
            MacHost::Lima => {
                let mut c = tokio::process::Command::new("limactl");
                c.args(["shell", "default", "sh", "-c", script]);
                c
            }
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.spawn().context("failed to spawn host process")
    }
}
