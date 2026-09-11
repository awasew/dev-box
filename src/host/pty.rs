//! Cross-platform pseudo-terminal support for the embedded SSH server.
//!
//! `portable_pty` (from the wezterm project) provides a pty implementation
//! on every platform dbx runs on -- including Windows via ConPTY --
//! but its API is blocking/thread-based. This module bridges it onto
//! `tokio` channels so `crate::sshd::handler` can drive it from the async
//! SSH server without blocking the runtime: a couple of dedicated OS
//! threads do the actual blocking reads/writes/waits, and hand data
//! across via channels that are safe to touch from async code.
//!
//! Without this, every session spawned by `dbx ssh-proxy` talks to
//! the box over plain OS pipes (see `HostTransport::spawn_piped`), which
//! is fine for one-shot commands but means interactive full-screen
//! programs (vim, htop, ...) and shell job control don't work, since the
//! remote shell never sees a real controlling terminal.

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, ExitStatus, MasterPty};
use std::io::{Read, Write};
use tokio::sync::mpsc;

pub use portable_pty::PtySize;

/// A live pty-backed child process, already bridged onto async-friendly
/// channels by [`spawn`].
///
/// * Read from `output_rx` to get bytes the child wrote (stdout+stderr
///   merged, as is inherent to a pty).
/// * Send into `input_tx` (a plain, non-blocking `std::sync::mpsc`
///   sender -- safe to call from an async fn) to write bytes to the
///   child's stdin.
/// * Call `master.resize(...)` whenever the client's terminal window
///   changes size.
/// * Await `wait_handle` to learn the exit status; call `killer.kill()`
///   to terminate the child early.
pub struct PtyChild {
    /// Kept around so callers can resize the pty later (via the
    /// `MasterPty::resize` trait method) and so `Drop` closes the
    /// master side once the caller is done with it; reading and writing
    /// happen through the independent handles below.
    pub master: Box<dyn MasterPty + Send>,
    /// Split out from the `Child` *before* it's moved into the blocking
    /// wait task, so it can still be signalled independently.
    pub killer: Box<dyn ChildKiller + Send + Sync>,
    pub output_rx: mpsc::UnboundedReceiver<Vec<u8>>,
    pub input_tx: std::sync::mpsc::Sender<Vec<u8>>,
    pub wait_handle: tokio::task::JoinHandle<std::io::Result<ExitStatus>>,
}

/// Clamps a client-supplied `u32` terminal dimension into `PtySize`'s
/// `u16` fields instead of truncating, which could otherwise wrap a
/// bogus/huge value into something small and confusing.
pub fn clamp_u16(value: u32) -> u16 {
    value.min(u32::from(u16::MAX)) as u16
}

/// Spawns `program args...` attached to a freshly-allocated pty of the
/// given `size` and immediately starts the background threads/tasks that
/// bridge it onto async channels.
pub fn spawn(program: &str, args: &[String], size: PtySize) -> Result<PtyChild> {
    let pty_system = native_pty_system();
    let pair = pty_system
        .openpty(size)
        .context("failed to allocate a pseudo-terminal")?;

    let mut cmd = CommandBuilder::new(program);
    cmd.args(args);

    let mut child = pair
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("failed to spawn `{program}` in a pseudo-terminal"))?;

    // Drop our copy of the slave side once the child holds it, so the
    // child ends up the sole owner and EOF/hangup propagate correctly
    // when it exits (mirrors the portable_pty examples).
    drop(pair.slave);

    let killer = child.clone_killer();

    let mut reader = pair
        .master
        .try_clone_reader()
        .context("failed to clone pty reader")?;
    let mut writer = pair
        .master
        .take_writer()
        .context("failed to take pty writer")?;

    let (output_tx, output_rx) = mpsc::unbounded_channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buf = [0u8; 8192];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if output_tx.send(buf[..n].to_vec()).is_err() {
                        break;
                    }
                }
            }
        }
    });

    let (input_tx, input_rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        while let Ok(chunk) = input_rx.recv() {
            if writer.write_all(&chunk).is_err() {
                break;
            }
            let _ = writer.flush();
        }
    });

    // `Child::wait` is blocking, so it can't run directly on the tokio
    // runtime; `spawn_blocking` gives it a dedicated thread and lets the
    // caller `.await` the result like any other async task.
    let wait_handle = tokio::task::spawn_blocking(move || child.wait());

    Ok(PtyChild {
        master: pair.master,
        killer,
        output_rx,
        input_tx,
        wait_handle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamp_u16_passes_through_small_values() {
        assert_eq!(clamp_u16(80), 80);
        assert_eq!(clamp_u16(0), 0);
    }

    #[test]
    fn clamp_u16_saturates_oversized_values() {
        assert_eq!(clamp_u16(u32::MAX), u16::MAX);
        assert_eq!(clamp_u16(u32::from(u16::MAX) + 1), u16::MAX);
    }

    #[test]
    fn spawn_runs_a_trivial_command_and_reports_exit_status() {
        // `sh` ships on every Unix dbx actually targets; on Windows
        // CI this test is skipped since there's no native pty-friendly
        // shell guaranteed on PATH without WSL.
        if cfg!(windows) {
            return;
        }

        // `spawn` uses `tokio::task::spawn_blocking` internally, which
        // needs a live runtime context -- build one by hand rather than
        // depending on the `macros` feature just for `#[tokio::test]`.
        let rt = tokio::runtime::Runtime::new().expect("build a tokio runtime for the test");
        rt.block_on(async {
            let mut child = spawn(
                "sh",
                &["-c".to_string(), "echo pty-smoke-test".to_string()],
                PtySize {
                    rows: 24,
                    cols: 80,
                    pixel_width: 0,
                    pixel_height: 0,
                },
            )
            .expect("spawn a trivial command in a pty");

            let mut collected = Vec::new();
            while let Some(chunk) = child.output_rx.recv().await {
                collected.extend_from_slice(&chunk);
            }
            let output = String::from_utf8_lossy(&collected);
            assert!(
                output.contains("pty-smoke-test"),
                "unexpected pty output: {output:?}"
            );

            let status = child
                .wait_handle
                .await
                .expect("join the wait task")
                .expect("wait for the child");
            assert!(status.success());
        });
    }
}
