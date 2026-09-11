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
/// warning rather than failing the whole command. Values that would break
/// the downstream `distrobox enter --additional-flags "--env K=V ..."`
/// encoding (see `is_forwardable_value`) are also skipped with a warning
/// instead of being silently truncated, corrupted, or -- worst case --
/// used to smuggle extra shell tokens into the assembled command line.
pub fn forwarded_env_from(ini: &Ini) -> Vec<(String, String)> {
    let names = ini
        .get_from(Some("dev-environment"), "forward_env")
        .map(util::unquote)
        .unwrap_or("")
        .split_whitespace();

    let mut forwarded = Vec::new();
    for name in names {
        match std::env::var(name) {
            Ok(value) => {
                if is_forwardable_value(&value) {
                    forwarded.push((name.to_string(), value));
                } else {
                    eprintln!(
                        "==> warning: forward_env lists `{name}`, but its value contains \
                         whitespace or shell-special characters that can't be safely passed \
                         through `distrobox enter --additional-flags` -- skipping"
                    );
                }
            }
            Err(_) => {
                eprintln!(
                    "==> warning: forward_env lists `{name}`, but it is not set in this shell -- skipping"
                );
            }
        }
    }
    forwarded
}

/// Returns `true` if `value` can be safely embedded as one whitespace-split
/// token (`K=V`) inside the `--additional-flags "--env K=V ..."` string
/// that `DistroboxEngine::enter_script` builds. distrobox splits that
/// string on whitespace itself, so any value containing whitespace would
/// silently fragment into extra, unintended flags; quote/backtick/dollar
/// characters are rejected defensively in case the string is ever
/// re-interpreted by a shell further downstream (e.g. inside distrobox or
/// the container runtime it drives).
fn is_forwardable_value(value: &str) -> bool {
    !value.is_empty()
        && !value
            .chars()
            .any(|c| c.is_whitespace() || matches!(c, '\'' | '"' | '`' | '$' | '\\'))
}

/// Returns `true` if `[dev-environment] scratchpad = true` is declared in the config.
pub fn is_scratchpad_enabled(ini: &Ini) -> bool {
    ini.get_from(Some("dev-environment"), "scratchpad")
        .map(util::unquote)
        .map(|v| v.eq_ignore_ascii_case("true") || v == "1")
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Writes `contents` to a uniquely-named file under the OS temp dir and
    /// returns its path. Using the temp dir (rather than a fixtures/
    /// directory) keeps these tests self-contained and safe to run in
    /// parallel without a `tempfile` dependency.
    fn write_temp_ini(name: &str, contents: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "dev-box-test-{name}-{}-{:?}.ini",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut f = std::fs::File::create(&path).expect("create temp ini");
        f.write_all(contents.as_bytes()).expect("write temp ini");
        path
    }

    #[test]
    fn merge_layers_skips_missing_files() {
        let missing = PathBuf::from("/does/not/exist/devbox.ini");
        let merged = merge_layers(&[missing]).expect("missing layers are skipped, not an error");
        assert!(merged.get_from(Some("dev-environment"), "name").is_none());
    }

    #[test]
    fn merge_layers_overwrites_scalar_keys() {
        let base = write_temp_ini(
            "base",
            "[dev-environment]\nname = \"base-name\"\nimage = \"ubuntu:22.04\"\n",
        );
        let override_ = write_temp_ini("override", "[dev-environment]\nname = \"override-name\"\n");

        let merged = merge_layers(&[base.clone(), override_.clone()]).expect("merge succeeds");
        assert_eq!(
            merged.get_from(Some("dev-environment"), "name"),
            Some("override-name")
        );
        assert_eq!(
            merged.get_from(Some("dev-environment"), "image"),
            Some("ubuntu:22.04")
        );

        let _ = std::fs::remove_file(base);
        let _ = std::fs::remove_file(override_);
    }

    #[test]
    fn merge_layers_space_joins_additive_keys() {
        let base = write_temp_ini(
            "additive-base",
            "[dev-environment]\nadditional_packages = \"git curl\"\n",
        );
        let extra = write_temp_ini(
            "additive-extra",
            "[dev-environment]\nadditional_packages = \"build-essential\"\n",
        );

        let merged = merge_layers(&[base.clone(), extra.clone()]).expect("merge succeeds");
        assert_eq!(
            merged.get_from(Some("dev-environment"), "additional_packages"),
            Some("git curl build-essential")
        );

        let _ = std::fs::remove_file(base);
        let _ = std::fs::remove_file(extra);
    }

    #[test]
    fn merge_layers_rejects_malformed_ini() {
        let bad = write_temp_ini("malformed", "this is not [valid ini\n===\n");
        let result = merge_layers(&[bad.clone()]);
        assert!(result.is_err());
        let _ = std::fs::remove_file(bad);
    }

    #[test]
    fn is_forwardable_value_accepts_typical_tokens() {
        assert!(is_forwardable_value("sk-abc123XYZ"));
        assert!(is_forwardable_value("ghp_1234567890"));
    }

    #[test]
    fn is_forwardable_value_rejects_dangerous_or_empty_values() {
        assert!(!is_forwardable_value(""));
        assert!(!is_forwardable_value("has space"));
        assert!(!is_forwardable_value("has\ttab"));
        assert!(!is_forwardable_value("quote'here"));
        assert!(!is_forwardable_value("double\"quote"));
        assert!(!is_forwardable_value("back`tick"));
        assert!(!is_forwardable_value("dollar$sign"));
        assert!(!is_forwardable_value("back\\slash"));
    }

    #[test]
    fn forwarded_env_from_skips_unset_and_unsafe_values() {
        let ini = write_temp_ini(
            "forward-env",
            "[dev-environment]\nforward_env = \"DEV_BOX_TEST_SAFE DEV_BOX_TEST_UNSAFE DEV_BOX_TEST_UNSET\"\n",
        );
        let loaded = Ini::load_from_file(&ini).expect("load temp ini");
        let _ = std::fs::remove_file(ini);

        std::env::set_var("DEV_BOX_TEST_SAFE", "safe-value-123");
        std::env::set_var("DEV_BOX_TEST_UNSAFE", "unsafe value");
        std::env::remove_var("DEV_BOX_TEST_UNSET");

        let forwarded = forwarded_env_from(&loaded);

        std::env::remove_var("DEV_BOX_TEST_SAFE");
        std::env::remove_var("DEV_BOX_TEST_UNSAFE");

        assert_eq!(
            forwarded,
            vec![(
                "DEV_BOX_TEST_SAFE".to_string(),
                "safe-value-123".to_string()
            )]
        );
    }

    #[test]
    fn is_scratchpad_enabled_accepts_true_and_one() {
        let mut ini = Ini::new();
        ini.with_section(Some("dev-environment"))
            .set("scratchpad", "true");
        assert!(is_scratchpad_enabled(&ini));

        let mut ini2 = Ini::new();
        ini2.with_section(Some("dev-environment"))
            .set("scratchpad", "1");
        assert!(is_scratchpad_enabled(&ini2));

        let mut ini3 = Ini::new();
        ini3.with_section(Some("dev-environment"))
            .set("scratchpad", "false");
        assert!(!is_scratchpad_enabled(&ini3));

        let empty = Ini::new();
        assert!(!is_scratchpad_enabled(&empty));
    }

    #[test]
    fn to_ini_string_round_trips() {
        let mut ini = Ini::new();
        ini.with_section(Some("dev-environment"))
            .set("name", "demo");
        let rendered = to_ini_string(&ini).expect("render ini");
        assert!(rendered.contains("[dev-environment]"));
        assert!(rendered.contains("name=demo") || rendered.contains("name = demo"));
    }
}
