use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

/// Strips a single layer of surrounding double-quotes from a configuration
/// value, e.g. `"ubuntu:24.04"` → `ubuntu:24.04`.
///
/// This is the single canonical location for quote-stripping, replacing the
/// two separate `trim_matches('"')` call sites that previously existed in
/// `config::merge_layers` and `config::forwarded_env_from`.
pub fn unquote(s: &str) -> &str {
    s.trim_matches('"')
}

/// Resolves `path` to an absolute path relative to the current working
/// directory, without requiring the path to exist on disk (configuration
/// layers are commonly optional).
///
/// Previously lived in `sshd` — moved here because it has no SSH-specific
/// logic and is equally useful in the config layer.
pub fn to_absolute(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(std::env::current_dir()
            .context("could not determine the current directory")?
            .join(path))
    }
}
