//! The `russh` server `Handler` implementation that backs `dev-box
//! ssh-proxy`.
//!
//! Authentication is handled entirely by accepting the SSH `"none"`
//! method (`Handler::auth_none`). OpenSSH clients always probe with
//! `"none"` automatically before trying any key or password -- by
//! accepting it unconditionally, no key or password is ever requested at
//! all. This is safe here specifically because the transport is never a
//! real network socket: `dev-box ssh-proxy` is only ever reachable by
//! spawning it locally (via `ProxyCommand`), which already requires the
//! same OS-level access needed to run `distrobox` directly.
//!
//! Each shell/exec request spawns `distrobox enter <box>` (or a specific
//! command inside it) via `HostTransport::spawn_piped`, and pumps bytes
//! between the SSH channel and the child process's stdio.

use crate::engine::ContainerEngine;
use crate::host::HostTransport;
use anyhow::Context;
use russh::server::{Auth, ChannelOpenHandle, Handler, Session};
use russh::{Channel, ChannelId, Pty};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::ChildStdin;
use tokio::sync::oneshot;

/// Tracks the live state of a child process spawned for an SSH channel.
struct ChildState {
    /// The child's stdin pipe. Set to `None` once `channel_eof` is received,
    /// which closes the pipe and signals EOF to the process inside the box.
    stdin: Option<ChildStdin>,
    /// Sending on this channel triggers `child.kill()` in the background
    /// exit-watcher task, so closing an SSH channel always terminates the
    /// underlying `distrobox enter` process immediately.
    kill_tx: oneshot::Sender<()>,
}

pub struct DevBoxHandler {
    pub host: Arc<dyn HostTransport>,
    /// Stored as `Arc<dyn ContainerEngine>` so the SSH handler is decoupled
    /// from `DistroboxEngine` specifically and works with any future backend.
    pub engine: Arc<dyn ContainerEngine>,
    pub box_name: Arc<str>,
    pub forwarded_env: Vec<(String, String)>,
    pub work_dir: Option<String>,
    pub children: HashMap<ChannelId, ChildState>,
}

impl DevBoxHandler {
    /// Spawns `script` via the host transport and wires its stdio to
    /// `channel`: stdout/stderr are streamed back to the client as they
    /// arrive, and the channel is closed with the child's exit status
    /// once it terminates. The child's stdin is retained so `data()` can
    /// forward further client input to it.
    ///
    /// A `oneshot` kill channel is wired into the exit-watcher task so
    /// that `channel_close` can immediately terminate the child rather
    /// than waiting for it to drain naturally.
    fn spawn_child(
        &mut self,
        channel: ChannelId,
        script: &str,
        session: &mut Session,
    ) -> anyhow::Result<()> {
        let mut child = self
            .host
            .spawn_piped(script)
            .context("failed to spawn distrobox")?;
        let stdin = child.stdin.take().context("child stdin missing")?;
        let mut stdout = child.stdout.take().context("child stdout missing")?;
        let mut stderr = child.stderr.take().context("child stderr missing")?;

        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        self.children.insert(
            channel,
            ChildState {
                stdin: Some(stdin),
                kill_tx,
            },
        );

        let out_handle = session.handle();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stdout.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if out_handle.data(channel, buf[..n].to_vec()).await.is_err() {
                            break;
                        }
                    }
                }
            }
        });

        let err_handle = session.handle();
        tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            loop {
                match stderr.read(&mut buf).await {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        if err_handle
                            .extended_data(channel, 1, buf[..n].to_vec())
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        let exit_handle = session.handle();
        tokio::spawn(async move {
            tokio::select! {
                // Normal path: child exits on its own.
                status = child.wait() => {
                    let code = status.ok().and_then(|s| s.code()).unwrap_or(1) as u32;
                    let _ = exit_handle.exit_status_request(channel, code).await;
                    let _ = exit_handle.eof(channel).await;
                    let _ = exit_handle.close(channel).await;
                }
                // Kill path: channel_close sent the kill signal before the
                // child exited; terminate it immediately.
                _ = kill_rx => {
                    child.kill().await.ok();
                    let _ = exit_handle.exit_status_request(channel, 1).await;
                    let _ = exit_handle.eof(channel).await;
                    let _ = exit_handle.close(channel).await;
                }
            }
        });

        Ok(())
    }
}

impl Handler for DevBoxHandler {
    type Error = anyhow::Error;

    async fn auth_none(&mut self, _user: &str) -> Result<Auth, Self::Error> {
        // Always accept: see module docs for why this is safe here.
        Ok(Auth::Accept)
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<russh::server::Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let script = self.engine.enter_script(
            &self.box_name,
            None,
            &self.forwarded_env,
            self.work_dir.as_deref(),
        );
        self.spawn_child(channel, &script, session)?;
        session.channel_success(channel)?;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let command = String::from_utf8_lossy(data).into_owned();
        let script = self.engine.enter_script(
            &self.box_name,
            Some(&command),
            &self.forwarded_env,
            self.work_dir.as_deref(),
        );
        self.spawn_child(channel, &script, session)?;
        session.channel_success(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(state) = self.children.get_mut(&channel) {
            if let Some(stdin) = &mut state.stdin {
                let _ = stdin.write_all(data).await;
                let _ = stdin.flush().await;
            }
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Drop the child's stdin handle to close the pipe, signalling EOF to
        // the process inside the box. The ChildState itself (and its kill_tx)
        // stays alive so channel_close can still send the kill signal if the
        // process doesn't exit on its own after receiving EOF.
        if let Some(state) = self.children.get_mut(&channel) {
            state.stdin = None;
        }
        Ok(())
    }

    async fn channel_close(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Trigger the kill signal so the background exit-watcher task
        // terminates the child immediately. If the child already exited
        // naturally, kill_rx was dropped and the send is a harmless no-op.
        if let Some(state) = self.children.remove(&channel) {
            let _ = state.kill_tx.send(());
        }
        Ok(())
    }

    // The following are acknowledged but otherwise no-ops: dev-box does
    // not allocate a real pseudo-terminal for the child process (yet),
    // so interactive full-screen tools (vim, htop, ...) won't render
    // correctly. Plain shells and most CLI tools work fine regardless.
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn env_request(
        &mut self,
        channel: ChannelId,
        _variable_name: &str,
        _variable_value: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn window_change_request(
        &mut self,
        channel: ChannelId,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }
}
