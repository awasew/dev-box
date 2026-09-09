use crate::util;
use anyhow::{Context, Result};
use ini::Ini;
use std::path::PathBuf;

/// Returns `true` if `key` is accumulated additively across layers instead
/// of being overwritten by the highest-priority layer. Uses `matches!` for
/// an O(1) compile-time check rather than a runtime slice scan.
fn is_additive(key: &str) -> bool {
    matches!(
        key,
        "additional_packages" | "init_hooks" | "exported_apps" | "forward_env"
    )
}

/// The default N-layer cascade, in ascending priority order:
/// user global preferences -> project config -> local (gitignored) overrides.
pub fn default_layers() -> Vec<PathBuf> {
    let mut layers = Vec::new();
    if let Some(config_dir) = dirs::config_dir() {
        layers.push(config_dir.join("dev-box").join("global.ini"));
    }
    layers.push(PathBuf::from("./devbox.ini"));
    layers.push(PathBuf::from("./devbox.local.ini"));
    layers
}

/// Merges INI layers in memory, in order. Missing files are skipped
/// silently so users don't need every layer to exist. Scalar keys are
/// overwritten by later layers; additive keys are space-joined.
pub fn merge_layers(paths: &[PathBuf]) -> Result<Ini> {
    let mut merged = Ini::new();

    for path in paths {
        if !path.exists() {
            continue;
        }
        let layer = Ini::load_from_file(path)
            .with_context(|| format!("failed to parse INI layer: {}", path.display()))?;

        for (section, props) in layer.iter() {
            for (key, val) in props.iter() {
                if is_additive(key) {
                    let existing = merged
                        .get_from(section, key)
                        .map(util::unquote)
                        .unwrap_or("")
                        .to_string();
                    let new_val = util::unquote(val);
                    let combined = if existing.is_empty() {
                        new_val.to_string()
                    } else {
                        format!("{existing} {new_val}")
                    };
                    merged.with_section(section).set(key, combined);
                } else {
                    merged.with_section(section).set(key, val);
                }
            }
        }
    }

    Ok(merged)
}

/// Renders a merged `Ini` back into an INI-formatted string, ready to be
/// piped into `distrobox assemble create --file /dev/stdin`.
pub fn to_ini_string(ini: &Ini) -> Result<String> {
    let mut buf = Vec::new();
    ini.write_to(&mut buf)?;
    Ok(String::from_utf8(buf)?)
}

/// Reads `[dev-environment] forward_env` (a space-separated list of
/// environment variable *names*, never values -- so it's safe to commit
/// to `devbox.ini`) and resolves each name against the current
/// process's live environment. This is how host secrets (AI agent API
/// keys, tokens, ...) reach the box: dev-box never stores or persists
/// the values themselves, only forwards whatever is set in the shell
/// that invoked `dev-box up` / `dev-box enter` / `ssh <box>` at that
/// moment.
///
/// Names that aren't set in the current environment are skipped with a
/// warning rather than failing the whole command.
pub fn forwarded_env_from(ini: &Ini) -> Vec<(String, String)> {
    let names = ini
        .get_from(Some("dev-environment"), "forward_env")
        .map(util::unquote)
        .unwrap_or("")
        .split_whitespace();

    let mut forwarded = Vec::new();
    for name in names {
        match std::env::var(name) {
            Ok(value) => forwarded.push((name.to_string(), value)),
            Err(_) => {
                eprintln!(
                    "==> warning: forward_env lists `{name}`, but it is not set in this shell -- skipping"
                );
            }
        }
    }
    forwarded
}
