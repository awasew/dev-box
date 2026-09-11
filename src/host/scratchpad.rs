use super::HostTransport;
use anyhow::{bail, Context, Result};
use std::path::Path;

/// Escapes a string for safe embedding in a POSIX shell command line.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// Returns the scratchpad path inside WSL2 for `box_name`.
/// Stored in `/tmp/dev-box/scratchpads/<box_name>` on native Linux ext4/tmpfs
/// to achieve 100% native Linux I/O performance.
pub fn scratchpad_path(box_name: &str) -> String {
    format!("/tmp/dev-box/scratchpads/{box_name}")
}

/// Translates a Windows path (e.g. `C:\Users\foo\project`) to a WSL path
/// using `wslpath -u` or fallback mount translation.
pub fn to_wsl_path(host: &dyn HostTransport, path: &Path) -> Result<String> {
    let path_str = path.to_string_lossy();
    let script = format!("wslpath -u {}", shell_quote(&path_str));
    if let Ok(converted) = host.capture(&script) {
        if !converted.is_empty() {
            return Ok(converted);
        }
    }

    // Fallback translation: C:\path -> /mnt/c/path
    let normalized = path_str.replace('\\', "/");
    if let Some((drive, rest)) = normalized.split_once(':') {
        let drive_letter = drive.to_lowercase();
        Ok(format!("/mnt/{drive_letter}{rest}"))
    } else {
        Ok(normalized)
    }
}

/// Checks whether `rsync` is installed in WSL.
fn ensure_rsync(host: &dyn HostTransport) -> Result<()> {
    let check = host.capture("command -v rsync")?;
    if check.is_empty() {
        bail!(
            "rsync was not found in WSL. Install it with `sudo apt install rsync` (or equivalent) to use scratchpad sync."
        );
    }
    Ok(())
}

/// Synchronizes project files from the Windows host directory into the WSL2
/// ext4 scratchpad. Honors `.gitignore` and keeps heavy build artifacts
/// (`target/`, `node_modules/`) isolated.
pub fn sync_to_scratchpad(host: &dyn HostTransport, host_dir: &Path, box_name: &str) -> Result<()> {
    ensure_rsync(host)?;
    let wsl_src = to_wsl_path(host, host_dir)?;
    let target = scratchpad_path(box_name);

    eprintln!("==> [scratchpad] syncing host ({wsl_src}) -> WSL ext4 ({target})...");
    let script = format!(
        "mkdir -p {target_q} && rsync -au --delete --filter=':- .gitignore' --exclude='target/' --exclude='node_modules/' --exclude='.git/' {src_q}/ {target_q}/",
        target_q = shell_quote(&target),
        src_q = shell_quote(&wsl_src),
    );

    let status = host
        .run(&script, None)
        .context("failed to execute scratchpad sync to WSL")?;
    if !status.success() {
        bail!("rsync to scratchpad failed with status: {status}");
    }
    Ok(())
}

/// Synchronizes source changes back from the WSL2 ext4 scratchpad to the
/// Windows host directory (e.g. updated Cargo.lock, code generators).
/// Uses `-u` to preserve any newer files on the host and excludes build artifacts.
pub fn sync_from_scratchpad(
    host: &dyn HostTransport,
    host_dir: &Path,
    box_name: &str,
) -> Result<()> {
    ensure_rsync(host)?;
    let wsl_dest = to_wsl_path(host, host_dir)?;
    let target = scratchpad_path(box_name);

    eprintln!("==> [scratchpad] reverse-syncing WSL ext4 ({target}) -> host ({wsl_dest})...");
    let script = format!(
        "rsync -au --filter=':- .gitignore' --exclude='target/' --exclude='node_modules/' --exclude='.git/' {target_q}/ {dest_q}/",
        target_q = shell_quote(&target),
        dest_q = shell_quote(&wsl_dest),
    );

    let status = host
        .run(&script, None)
        .context("failed to execute reverse scratchpad sync to host")?;
    if !status.success() {
        bail!("rsync from scratchpad failed with status: {status}");
    }
    Ok(())
}

/// Cleans up the scratchpad directory in WSL2 ext4 storage.
pub fn clean_scratchpad(host: &dyn HostTransport, box_name: &str) -> Result<()> {
    let target = scratchpad_path(box_name);
    let script = format!("rm -rf {}", shell_quote(&target));
    let _ = host.run(&script, None);
    Ok(())
}
