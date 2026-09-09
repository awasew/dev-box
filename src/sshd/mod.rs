//! dev-box's "keyless" SSH bridge.
//!
//! Instead of installing and configuring a real `sshd` inside every box
//! (which would mean touching the container's filesystem and package
//! set), dev-box embeds a minimal SSH server directly in its own binary.
//! `dev-box ssh-proxy <box>` speaks the SSH protocol over its own
//! stdin/stdout -- exactly what an SSH `ProxyCommand` expects -- and
//! never opens a TCP listener at all.
//!
//! The only thing dev-box ever changes on the host is a managed block in
//! `~/.ssh/config` (see `install_client_config`); nothing inside the box
//! is touched.

mod handler;

use crate::engine::distrobox::DistroboxEngine;
use crate::host::HostTransport;
use anyhow::{Context, Result};
use handler::DevBoxHandler;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{self, Config};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const BEGIN_MARKER_PREFIX: &str = "# >>> dev-box:";
const END_MARKER_PREFIX: &str = "# <<< dev-box:";
const INCLUDE_LINE: &str = "Include dev-box_config";

/// Runs the embedded SSH server for `box_name`, blocking until the
/// session ends. Intended to be invoked as an SSH `ProxyCommand` target.
/// `forwarded_env` is forwarded into every shell/exec session started
/// inside the box (see `config::forwarded_env_from`).
pub fn run(
    host: Box<dyn HostTransport>,
    box_name: String,
    forwarded_env: Vec<(String, String)>,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start the async runtime for ssh-proxy")?;
    rt.block_on(serve(host, box_name, forwarded_env))
}

async fn serve(
    host: Box<dyn HostTransport>,
    box_name: String,
    forwarded_env: Vec<(String, String)>,
) -> Result<()> {
    // A fresh, in-memory-only host key. It's never written to disk and
    // never needs to be trusted by the client (`StrictHostKeyChecking
    // no` is safe here since this "connection" is a local pipe, not a
    // network socket -- there is no MITM to protect against).
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .context("failed to generate an ephemeral SSH host key")?;

    let config = Arc::new(Config {
        keys: vec![key],
        ..Default::default()
    });

    let handler = DevBoxHandler {
        host: Arc::from(host),
        engine: DistroboxEngine,
        box_name: Arc::from(box_name.as_str()),
        forwarded_env,
        children: HashMap::new(),
    };

    let stream = tokio::io::join(tokio::io::stdin(), tokio::io::stdout());

    let running = server::run_stream(config, stream, handler)
        .await
        .map_err(|e| anyhow::anyhow!("failed to start ssh session: {e}"))?;
    running
        .await
        .map_err(|e| anyhow::anyhow!("ssh session ended with an error: {e}"))?;
    Ok(())
}

/// Writes/updates the local SSH client config so `ssh <box_name>` (and
/// any Remote-SSH-capable IDE) just works, with no key or password ever
/// requested.
///
/// `layers` are the *absolute* paths to the config layers that produced
/// the box's current configuration (see `config::default_layers` /
/// `Cli::config`). They're baked into the generated `ProxyCommand` as
/// explicit `-c` flags so that `ssh <box_name>` -- which may be invoked
/// by an IDE from an arbitrary working directory -- always resolves the
/// exact same configuration (and therefore the same `forward_env` list)
/// that `dev-box up` used, regardless of its own current directory.
pub fn install_client_config(box_name: &str, layers: &[PathBuf]) -> Result<()> {
    let ssh_dir = dirs::home_dir()
        .context("could not determine the home directory")?
        .join(".ssh");
    fs::create_dir_all(&ssh_dir)
        .with_context(|| format!("failed to create {}", ssh_dir.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&ssh_dir, fs::Permissions::from_mode(0o700))
            .with_context(|| format!("failed to set permissions on {}", ssh_dir.display()))?;
    }

    ensure_include(&ssh_dir.join("config"))?;
    upsert_host_block(&ssh_dir.join("dev-box_config"), box_name, layers)?;
    Ok(())
}

/// Resolves `path` to an absolute path relative to the current working
/// directory, without requiring it to exist (config layers are commonly
/// optional).
pub fn to_absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("could not determine the current directory")?
            .join(path))
    }
}

/// Adds an `Include dev-box_config` line to the top of `~/.ssh/config` if
/// it isn't already present. Never touches any other line the user owns.
fn ensure_include(config_path: &Path) -> Result<()> {
    let existing = fs::read_to_string(config_path).unwrap_or_default();
    if existing.lines().any(|l| l.trim() == INCLUDE_LINE) {
        return Ok(());
    }
    let mut updated = String::new();
    updated.push_str(INCLUDE_LINE);
    updated.push('\n');
    updated.push_str(&existing);
    fs::write(config_path, updated)
        .with_context(|| format!("failed to update {}", config_path.display()))?;
    Ok(())
}

/// Inserts or replaces the marker-delimited `Host` block for `box_name`
/// inside dev-box's own managed config file, leaving every other box's
/// block untouched.
fn upsert_host_block(path: &Path, box_name: &str, layers: &[PathBuf]) -> Result<()> {
    let exe =
        std::env::current_exe().context("could not determine dev-box's own executable path")?;
    let begin = format!("{BEGIN_MARKER_PREFIX} {box_name} >>>");
    let end = format!("{END_MARKER_PREFIX} {box_name} <<<");

    let mut config_flags = String::new();
    for layer in layers {
        config_flags.push_str(&format!(" -c \"{}\"", layer.display()));
    }

    let block = format!(
        "{begin}\nHost {box_name}\n    ProxyCommand \"{}\"{config_flags} ssh-proxy {box_name}\n    StrictHostKeyChecking no\n    UserKnownHostsFile /dev/null\n    BatchMode yes\n    LogLevel ERROR\n{end}\n",
        exe.display(),
    );

    let existing = fs::read_to_string(path).unwrap_or_default();
    let lines: Vec<&str> = existing.lines().collect();

    let existing_block = lines
        .iter()
        .position(|l| l.trim() == begin)
        .and_then(|start| {
            lines[start..]
                .iter()
                .position(|l| l.trim() == end)
                .map(|offset| (start, start + offset))
        });

    let mut new_content = String::new();
    match existing_block {
        Some((start, end_idx)) => {
            if start > 0 {
                new_content.push_str(&lines[..start].join("\n"));
                new_content.push('\n');
            }
            new_content.push_str(&block);
            let rest = &lines[end_idx + 1..];
            if !rest.is_empty() {
                new_content.push_str(&rest.join("\n"));
                new_content.push('\n');
            }
        }
        None => {
            new_content.push_str(&existing);
            if !new_content.is_empty() && !new_content.ends_with('\n') {
                new_content.push('\n');
            }
            new_content.push_str(&block);
        }
    }

    fs::write(path, new_content).with_context(|| format!("failed to update {}", path.display()))?;
    Ok(())
}
