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

use crate::engine::ContainerEngine;
use crate::host::HostTransport;
use crate::util;
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
///
/// Both `host` and `engine` are `Arc`s so they can be shared with the
/// async handler without an extra allocation.
pub fn run(
    host: Arc<dyn HostTransport>,
    box_name: String,
    forwarded_env: Vec<(String, String)>,
    engine: Arc<dyn ContainerEngine>,
    work_dir: Option<String>,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to start the async runtime for ssh-proxy")?;
    rt.block_on(serve(host, box_name, forwarded_env, engine, work_dir))
}

async fn serve(
    host: Arc<dyn HostTransport>,
    box_name: String,
    forwarded_env: Vec<(String, String)>,
    engine: Arc<dyn ContainerEngine>,
    work_dir: Option<String>,
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
        host,
        engine,
        box_name: Arc::from(box_name.as_str()),
        forwarded_env,
        work_dir,
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

/// Resolves a slice of (potentially relative) config-layer paths to their
/// absolute counterparts using `util::to_absolute`.
pub fn absolute_layers(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    paths
        .iter()
        .map(|p| util::to_absolute(p))
        .collect::<Result<Vec<_>>>()
        .context("failed to resolve configuration layer paths")
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

/// Removes the managed SSH client config block for `box_name` from
/// `~/.ssh/dev-box_config` when the box is deleted.
pub fn remove_client_config(box_name: &str) -> Result<()> {
    let ssh_dir = match dirs::home_dir() {
        Some(h) => h.join(".ssh"),
        None => return Ok(()),
    };
    let config_path = ssh_dir.join("dev-box_config");
    if !config_path.exists() {
        return Ok(());
    }
    remove_host_block(&config_path, box_name)?;
    Ok(())
}

/// Removes the marker-delimited `Host` block for `box_name` from the config file,
/// leaving all other host entries intact.
fn remove_host_block(path: &Path, box_name: &str) -> Result<()> {
    let begin = format!("{BEGIN_MARKER_PREFIX} {box_name} >>>");
    let end = format!("{END_MARKER_PREFIX} {box_name} <<<");

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

    if let Some((start, end_idx)) = existing_block {
        let mut new_content = String::new();
        if start > 0 {
            new_content.push_str(&lines[..start].join("\n"));
            new_content.push('\n');
        }
        let rest = &lines[end_idx + 1..];
        if !rest.is_empty() {
            new_content.push_str(&rest.join("\n"));
            new_content.push('\n');
        }
        fs::write(path, new_content)
            .with_context(|| format!("failed to update {}", path.display()))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "dev-box-sshd-test-{name}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    #[test]
    fn ensure_include_adds_line_once() {
        let path = temp_path("ssh-config");
        let _ = fs::remove_file(&path);

        ensure_include(&path).expect("first insert succeeds");
        let first = fs::read_to_string(&path).expect("read");
        assert_eq!(
            first.lines().filter(|l| l.trim() == INCLUDE_LINE).count(),
            1
        );

        // Running it again must not duplicate the line.
        ensure_include(&path).expect("second call is a no-op");
        let second = fs::read_to_string(&path).expect("read");
        assert_eq!(
            second.lines().filter(|l| l.trim() == INCLUDE_LINE).count(),
            1
        );

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn ensure_include_preserves_existing_content() {
        let path = temp_path("ssh-config-existing");
        fs::write(&path, "Host example\n    HostName example.com\n").expect("seed file");

        ensure_include(&path).expect("insert succeeds");
        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("Host example"));
        assert!(content.lines().next() == Some(INCLUDE_LINE));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn upsert_host_block_inserts_then_updates_in_place() {
        let path = temp_path("dev-box-config");
        let _ = fs::remove_file(&path);
        let layers = vec![PathBuf::from("/tmp/devbox.ini")];

        upsert_host_block(&path, "my-box", &layers).expect("first insert");
        let first = fs::read_to_string(&path).expect("read");
        assert!(first.contains("Host my-box"));
        assert!(first.contains("ssh-proxy my-box"));

        // Add a second, unrelated box.
        upsert_host_block(&path, "other-box", &layers).expect("second insert");
        let with_two = fs::read_to_string(&path).expect("read");
        assert!(with_two.contains("Host my-box"));
        assert!(with_two.contains("Host other-box"));

        // Re-running for `my-box` must replace only its own block, not
        // duplicate it or disturb `other-box`.
        upsert_host_block(&path, "my-box", &layers).expect("update");
        let updated = fs::read_to_string(&path).expect("read");
        assert_eq!(updated.matches("Host my-box").count(), 1);
        assert_eq!(updated.matches("Host other-box").count(), 1);

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn remove_host_block_deletes_only_the_named_block() {
        let path = temp_path("dev-box-config-remove");
        let _ = fs::remove_file(&path);
        let layers = vec![PathBuf::from("/tmp/devbox.ini")];

        upsert_host_block(&path, "keep-box", &layers).expect("insert keep-box");
        upsert_host_block(&path, "drop-box", &layers).expect("insert drop-box");

        remove_host_block(&path, "drop-box").expect("remove drop-box");
        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("Host keep-box"));
        assert!(!content.contains("Host drop-box"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn remove_host_block_is_a_noop_when_block_absent() {
        let path = temp_path("dev-box-config-noop");
        fs::write(&path, "Host untouched\n    HostName example.com\n").expect("seed");

        remove_host_block(&path, "nonexistent-box").expect("no-op succeeds");
        let content = fs::read_to_string(&path).expect("read");
        assert!(content.contains("Host untouched"));

        let _ = fs::remove_file(&path);
    }

    #[test]
    fn absolute_layers_resolves_relative_paths() {
        let layers = vec![PathBuf::from("devbox.ini")];
        let resolved = absolute_layers(&layers).expect("resolves");
        assert!(resolved[0].is_absolute());
        assert!(resolved[0].ends_with("devbox.ini"));
    }
}
