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
            // Neither tool found — warn now rather than waiting for the
            // first `run()` call, so the user gets a clear message early.
            eprintln!(
                "==> warning: neither `podman` nor `limactl` was found. \
                 Install Podman Machine (https://podman.io) or Lima (https://lima-vm.io) \
                 before running `dev-box up`."
            );
            // Default to Podman Machine; `run` will surface the install
            // error if the user proceeds without fixing it.
            MacHost::PodmanMachine
        }
    }

    fn binary(&self) -> &'static str {
        match self {
            MacHost::PodmanMachine => "podman",
            MacHost::Lima => "limactl",
        }
    }

    /// Returns the `(binary, args)` pair needed to execute `script` inside
    /// the VM, shared by both the `std::process` and `tokio::process` paths
    /// so the match logic only exists in one place.
    fn cmd_parts<'a>(&self, script: &'a str) -> (&'static str, Vec<&'a str>) {
        match self {
            MacHost::PodmanMachine => ("podman", vec!["machine", "ssh", script]),
            MacHost::Lima => ("limactl", vec!["shell", "default", "sh", "-c", script]),
        }
    }

    fn wrap(&self, script: &str) -> Command {
        let (bin, args) = self.cmd_parts(script);
        let mut cmd = Command::new(bin);
        cmd.args(args);
        cmd
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
        let (bin, args) = self.cmd_parts(script);
        let mut cmd = tokio::process::Command::new(bin);
        cmd.args(args);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.spawn().context("failed to spawn host process")
    }
}
