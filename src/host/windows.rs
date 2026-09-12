use super::HostTransport;
use anyhow::{bail, Result};

/// Windows can't run Linux containers natively, so dbx bridges into
/// WSL2, where distrobox and its backend (Podman/Docker) actually run.
#[allow(dead_code)]
pub struct WindowsHost;

/// Fails fast with a clear message if `wsl.exe` is not on the PATH,
/// rather than letting the spawned command produce a confusing error.
#[allow(dead_code)]
fn require_wsl() -> Result<()> {
    if which::which("wsl.exe").is_err() {
        bail!("dbx requires WSL2 on Windows. Install it with `wsl --install` and try again.");
    }
    Ok(())
}

impl HostTransport for WindowsHost {
    fn name(&self) -> &'static str {
        "windows (WSL2)"
    }

    fn command_parts(&self, script: &str) -> Result<(String, Vec<String>)> {
        require_wsl()?;
        Ok((
            "wsl.exe".to_string(),
            vec![
                "-e".to_string(),
                "sh".to_string(),
                "-c".to_string(),
                script.to_string(),
            ],
        ))
    }
}
