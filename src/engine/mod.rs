use crate::host::HostTransport;
use anyhow::Result;

pub mod distrobox;

/// Abstracts the container backend dev-box drives. Distrobox is the only
/// implementation today, but this trait keeps the door open for
/// alternative backends without touching the config or host layers.
pub trait ContainerEngine {
    /// Human-readable engine name, for diagnostics.
    fn name(&self) -> &'static str;

    /// Whether the engine's binary is reachable on the given host.
    fn is_available(&self, host: &dyn HostTransport) -> bool;

    /// Streams the merged configuration payload into the engine to
    /// create (or update) the dev environment.
    fn assemble(&self, host: &dyn HostTransport, payload: &str) -> Result<()>;

    /// Enters an already-assembled environment interactively, forwarding
    /// `forwarded_env` (name, value) pairs into the box's session (e.g.
    /// AI agent API keys read from the host shell -- see
    /// `config::forwarded_env_from`), optionally changing into `work_dir`.
    fn enter(
        &self,
        host: &dyn HostTransport,
        box_name: &str,
        forwarded_env: &[(String, String)],
        work_dir: Option<&str>,
    ) -> Result<()>;

    /// Builds the shell script used to enter the box for a piped SSH
    /// session: an interactive shell if `command` is `None`, otherwise
    /// exactly that command. Used by the embedded SSH server in
    /// `crate::sshd`, which spawns this via `HostTransport::spawn_piped`.
    ///
    /// `forwarded_env` values must not contain literal spaces (they are
    /// passed through as `distrobox enter --additional-flags "--env
    /// K=V ..."`, which distrobox splits on whitespace).
    fn enter_script(
        &self,
        box_name: &str,
        command: Option<&str>,
        forwarded_env: &[(String, String)],
        work_dir: Option<&str>,
    ) -> String;

    /// Lists all existing container environments.
    fn list(&self, host: &dyn HostTransport) -> Result<()>;

    /// Stops a running container environment.
    fn stop(&self, host: &dyn HostTransport, box_name: &str) -> Result<()>;

    /// Removes a container environment.
    fn rm(&self, host: &dyn HostTransport, box_name: &str, force: bool) -> Result<()>;
}
