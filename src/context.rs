use crate::config;
use crate::engine::{distrobox::DistroboxEngine, ContainerEngine};
use crate::host::{self, HostTransport};
use anyhow::{Context, Result};
use ini::Ini;
use std::path::PathBuf;
use std::sync::Arc;

/// Bundles the three pieces every command needs — the merged configuration,
/// the platform host transport, and the container engine — so `main` builds
/// them exactly once before the `match` and passes a single `AppContext` to
/// each command handler, instead of duplicating construction in every arm.
pub struct AppContext {
    pub config: Ini,
    /// Arc so `sshd::run` can share the transport without a second detection.
    pub host: Arc<dyn HostTransport>,
    /// Arc so `sshd::run` can share the engine without a second allocation.
    pub engine: Arc<dyn ContainerEngine>,
}

impl AppContext {
    /// Merges `layers`, detects the host transport, and selects the engine.
    pub fn new(layers: &[PathBuf]) -> Result<Self> {
        let config = config::merge_layers(layers)?;
        // Arc::from(Box<dyn T>) is stable since Rust 1.21.
        let host: Arc<dyn HostTransport> = Arc::from(host::detect());
        let engine: Arc<dyn ContainerEngine> = Arc::new(DistroboxEngine);
        Ok(Self { config, host, engine })
    }

    /// Reads `[dev-environment] name=` from the merged configuration.
    pub fn box_name(&self) -> Result<String> {
        self.config
            .get_from(Some("dev-environment"), "name")
            .context("no [dev-environment] name= found in the merged configuration")
            .map(str::to_string)
    }

    /// Resolves `forward_env` variable names against the live process
    /// environment (see `config::forwarded_env_from`).
    pub fn forwarded_env(&self) -> Vec<(String, String)> {
        config::forwarded_env_from(&self.config)
    }
}
