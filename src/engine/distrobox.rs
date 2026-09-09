use super::ContainerEngine;
use crate::host::HostTransport;
use anyhow::{bail, Result};

/// Drives Distrobox as the underlying container engine.
#[derive(Clone, Copy)]
pub struct DistroboxEngine;

/// Single-quotes `s` for safe embedding in a POSIX shell command line,
/// escaping any embedded single quotes.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

impl ContainerEngine for DistroboxEngine {
    fn name(&self) -> &'static str {
        "distrobox"
    }

    fn is_available(&self, host: &dyn HostTransport) -> bool {
        host.capture("command -v distrobox")
            .map(|out| !out.is_empty())
            .unwrap_or(false)
    }

    fn assemble(&self, host: &dyn HostTransport, payload: &str) -> Result<()> {
        if !self.is_available(host) {
            bail!(
                "distrobox was not found on {}. Install it and try again: https://distrobox.it",
                host.name()
            );
        }
        let status = host.run("distrobox assemble create --file /dev/stdin", Some(payload))?;
        if !status.success() {
            bail!("distrobox assemble failed with status: {status}");
        }
        Ok(())
    }

    fn enter(
        &self,
        host: &dyn HostTransport,
        box_name: &str,
        forwarded_env: &[(String, String)],
    ) -> Result<()> {
        let script = self.enter_script(box_name, None, forwarded_env);
        let status = host.run(&script, None)?;
        if !status.success() {
            bail!("distrobox enter failed with status: {status}");
        }
        Ok(())
    }

    fn enter_script(
        &self,
        box_name: &str,
        command: Option<&str>,
        forwarded_env: &[(String, String)],
    ) -> String {
        let mut script = format!("distrobox enter {}", shell_quote(box_name));

        if !forwarded_env.is_empty() {
            let mut flags = String::new();
            for (key, value) in forwarded_env {
                if !flags.is_empty() {
                    flags.push(' ');
                }
                flags.push_str("--env ");
                flags.push_str(key);
                flags.push('=');
                flags.push_str(value);
            }
            script.push_str(" --additional-flags ");
            script.push_str(&shell_quote(&flags));
        }

        if let Some(cmd) = command {
            script.push_str(" -- sh -c ");
            script.push_str(&shell_quote(cmd));
        }

        script
    }
}
