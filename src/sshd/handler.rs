//! The `russh` server `Handler` implementation that backs `dbx
//! ssh-proxy`.
//!
//! Authentication is handled entirely by accepting the SSH `"none"`
//! method (`Handler::auth_none`). OpenSSH clients always probe with
//! `"none"` automatically before trying any key or password -- by
//! accepting it unconditionally, no key or password is ever requested at
//! all. This is safe here specifically because the transport is never a
//! real network socket: `dbx ssh-proxy` is only ever reachable by
//! spawning it locally (via `ProxyCommand`), which already requires the
//! same OS-level access needed to run `distrobox` directly.
//!
//! Each shell/exec request spawns `distrobox enter <box>` (or a specific
//! command inside it). If the client requested a pty first (the normal
//! case for an interactive `ssh <box>` session), it's spawned attached to
//! a real pseudo-terminal via `HostTransport::spawn_pty` so full-screen
//! programs and shell job control work correctly; otherwise (e.g. a
//! scripted `ssh <box> cmd` with no `-t`) it's spawned with plain pipes
//! via `HostTransport::spawn_piped`, matching normal OpenSSH behavior.

use crate::engine::ContainerEngine;
use crate::host::pty::{self, PtyChild, PtySize};
use crate::host::HostTransport;
use anyhow::Context;
use portable_pty::{ChildKiller, MasterPty};
use russh::server::{Auth, ChannelOpenHandle, Handler, Session};
use russh::{Channel, ChannelId, Pty};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::ChildStdin;
use tokio::sync::oneshot;

/// Tracks the live state of a child process spawned for an SSH channel.
enum ChildState {
    /// Spawned via plain OS pipes (`HostTransport::spawn_piped`) -- the
    /// non-interactive path used when the client didn't request a pty.
    Piped {
        /// The child's stdin pipe. Set to `None` once `channel_eof` is
        /// received, which closes the pipe and signals EOF downstream.
        stdin: Option<ChildStdin>,
        /// Sending on this channel triggers `child.kill()` in the
        /// background exit-watcher task, so closing an SSH channel
        /// always terminates the underlying process immediately.
        kill_tx: oneshot::Sender<()>,
    },
    /// Spawned attached to a real pseudo-terminal
    /// (`HostTransport::spawn_pty`) -- the interactive path used when
    /// the client sent a `pty_request` before `shell_request`/`exec_request`.
    Pty {
        /// Kept only for `window_change_request` to call `resize` on.
        master: Box<dyn MasterPty + Send>,
        /// Set to `None` on `channel_eof`, which drops the last sender
        /// and lets the pty's writer thread exit, delivering a hangup
        /// to the child -- the pty equivalent of closing a pipe's stdin.
        input_tx: Option<std::sync::mpsc::Sender<Vec<u8>>>,
        /// Same purpose as `Piped::kill_tx`.
        kill_tx: oneshot::Sender<()>,
    },
}

pub struct DbxHandler {
    pub host: Arc<dyn HostTransport>,
    pub engine: Arc<dyn ContainerEngine>,
    pub box_name: Arc<str>,
    pub forwarded_env: Vec<(String, String)>,
    pub work_dir: Option<String>,
    children: HashMap<ChannelId, ChildState>,
    pending_pty: HashMap<ChannelId, PtySize>,
}

impl DbxHandler {
    pub fn new(
        host: Arc<dyn HostTransport>,
        engine: Arc<dyn ContainerEngine>,
        box_name: Arc<str>,
        forwarded_env: Vec<(String, String)>,
        work_dir: Option<String>,
    ) -> Self {
        Self {
            host,
            engine,
            box_name,
            forwarded_env,
            work_dir,
            children: HashMap::new(),
            pending_pty: HashMap::new(),
        }
    }
    /// Spawns `script` via the host transport (plain pipes) and wires its
    /// stdio to `channel`: stdout/stderr are streamed back to the client
    /// as they arrive, and the channel is closed with the child's exit
    /// status once it terminates. The child's stdin is retained so
    /// `data()` can forward further client input to it.
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
            ChildState::Piped {
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

    /// Spawns `script` attached to a real pseudo-terminal of the given
    /// `size` and wires it to `channel`, mirroring `spawn_child` but
    /// using `HostTransport::spawn_pty` instead of `spawn_piped` so
    /// interactive full-screen programs and job control work.
    fn spawn_child_pty(
        &mut self,
        channel: ChannelId,
        script: &str,
        size: PtySize,
        session: &mut Session,
    ) -> anyhow::Result<()> {
        let PtyChild {
            master,
            killer,
            output_rx,
            input_tx,
            wait_handle,
        } = self
            .host
            .spawn_pty(script, size)
            .context("failed to spawn distrobox in a pseudo-terminal")?;

        let (kill_tx, kill_rx) = oneshot::channel::<()>();
        self.children.insert(
            channel,
            ChildState::Pty {
                master,
                input_tx: Some(input_tx),
                kill_tx,
            },
        );

        let out_handle = session.handle();
        let mut output_rx = output_rx;
        tokio::spawn(async move {
            while let Some(chunk) = output_rx.recv().await {
                if out_handle.data(channel, chunk).await.is_err() {
                    break;
                }
            }
        });

        let exit_handle = session.handle();
        let mut killer: Box<dyn ChildKiller + Send + Sync> = killer;
        tokio::spawn(async move {
            tokio::select! {
                // Normal path: child exits on its own.
                status = wait_handle => {
                    let code = status
                        .ok()
                        .and_then(|inner| inner.ok())
                        .map(|s| s.exit_code())
                        .unwrap_or(1);
                    let _ = exit_handle.exit_status_request(channel, code).await;
                    let _ = exit_handle.eof(channel).await;
                    let _ = exit_handle.close(channel).await;
                }
                // Kill path: channel_close sent the kill signal before the
                // child exited; terminate it immediately.
                _ = kill_rx => {
                    let _ = killer.kill();
                    let _ = exit_handle.exit_status_request(channel, 1).await;
                    let _ = exit_handle.eof(channel).await;
                    let _ = exit_handle.close(channel).await;
                }
            }
        });

        Ok(())
    }
}

impl Handler for DbxHandler {
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
        match self.pending_pty.remove(&channel) {
            Some(size) => self.spawn_child_pty(channel, &script, size, session)?,
            None => self.spawn_child(channel, &script, session)?,
        }
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
        match self.pending_pty.remove(&channel) {
            Some(size) => self.spawn_child_pty(channel, &script, size, session)?,
            None => self.spawn_child(channel, &script, session)?,
        }
        session.channel_success(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        match self.children.get_mut(&channel) {
            Some(ChildState::Piped {
                stdin: Some(stdin), ..
            }) => {
                let _ = stdin.write_all(data).await;
                let _ = stdin.flush().await;
            }
            Some(ChildState::Pty {
                input_tx: Some(tx), ..
            }) => {
                let _ = tx.send(data.to_vec());
            }
            _ => {}
        }
        Ok(())
    }

    async fn channel_eof(
        &mut self,
        channel: ChannelId,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        // Drop the child's input handle to signal EOF/hangup to the
        // process inside the box. The ChildState itself (and its
        // kill_tx) stays alive so channel_close can still send the kill
        // signal if the process doesn't exit on its own afterward.
        match self.children.get_mut(&channel) {
            Some(ChildState::Piped { stdin, .. }) => *stdin = None,
            Some(ChildState::Pty { input_tx, .. }) => *input_tx = None,
            None => {}
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
            let kill_tx = match state {
                ChildState::Piped { kill_tx, .. } => kill_tx,
                ChildState::Pty { kill_tx, .. } => kill_tx,
            };
            let _ = kill_tx.send(());
        }
        self.pending_pty.remove(&channel);
        Ok(())
    }

    /// Records the requested pty size for this channel; the following
    /// `shell_request`/`exec_request` consumes it and spawns via
    /// `spawn_child_pty` instead of the plain-pipe `spawn_child`. This
    /// matches real OpenSSH semantics: a pty is only allocated when the
    /// client explicitly asks for one (interactive `ssh <box>`, or `ssh
    /// -t <box> cmd`), never for a plain scripted `ssh <box> cmd`.
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        _modes: &[(Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.pending_pty.insert(
            channel,
            PtySize {
                rows: pty::clamp_u16(row_height),
                cols: pty::clamp_u16(col_width),
                pixel_width: pty::clamp_u16(pix_width),
                pixel_height: pty::clamp_u16(pix_height),
            },
        );
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
        col_width: u32,
        row_height: u32,
        pix_width: u32,
        pix_height: u32,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if let Some(ChildState::Pty { master, .. }) = self.children.get(&channel) {
            let _ = master.resize(PtySize {
                rows: pty::clamp_u16(row_height),
                cols: pty::clamp_u16(col_width),
                pixel_width: pty::clamp_u16(pix_width),
                pixel_height: pty::clamp_u16(pix_height),
            });
        }
        session.channel_success(channel)?;
        Ok(())
    }
}

/// Backward-compatible alias for `DbxHandler`.
#[allow(dead_code)]
pub type DevBoxHandler = DbxHandler;
