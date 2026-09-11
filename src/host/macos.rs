use super::HostTransport;
use anyhow::{bail, Result};

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
            // Default to Podman Machine; `command_parts` will surface the
            // install error if the user proceeds without fixing it.
            MacHost::PodmanMachine
        }
    }

    fn binary(&self) -> &'static str {
        match self {
            MacHost::PodmanMachine => "podman",
            MacHost::Lima => "limactl",
        }
    }

    /// Returns the `(binary, args)` pair needed to execute `script`
    /// inside the VM.
    fn cmd_parts<'a>(&self, script: &'a str) -> (&'static str, Vec<&'a str>) {
        match self {
            MacHost::PodmanMachine => ("podman", vec!["machine", "ssh", script]),
            MacHost::Lima => ("limactl", vec!["shell", "default", "sh", "-c", script]),
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

    fn command_parts(&self, script: &str) -> Result<(String, Vec<String>)> {
        if which::which(self.binary()).is_err() {
            bail!(
                "macOS requires Podman Machine or Lima to run distrobox. Install one of them and try again."
            );
        }
        let (bin, args) = self.cmd_parts(script);
        Ok((
            bin.to_string(),
            args.into_iter().map(String::from).collect(),
        ))
    }
}
