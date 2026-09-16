use super::HostTransport;
use anyhow::{bail, Result};
use std::path::Path;

/// Windows can't run Linux containers natively, so dbx bridges into
/// WSL2, where distrobox and its backend (Podman/Docker) actually run.
pub struct WindowsHost;

/// Fails fast with a clear message if `wsl.exe` is not on the PATH,
/// rather than letting the spawned command produce a confusing error.
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

    fn sync_to_scratchpad(&self, host_dir: &Path, box_name: &str) -> Result<()> {
        super::scratchpad::sync_to_scratchpad(self, host_dir, box_name)
    }

    fn sync_from_scratchpad(&self, host_dir: &Path, box_name: &str) -> Result<()> {
        super::scratchpad::sync_from_scratchpad(self, host_dir, box_name)
    }

    fn scratchpad_work_dir(&self, box_name: &str) -> Option<String> {
        Some(super::scratchpad::scratchpad_path(box_name))
    }

    fn clean_scratchpad(&self, box_name: &str) -> Result<()> {
        super::scratchpad::clean_scratchpad(self, box_name)
    }
}
