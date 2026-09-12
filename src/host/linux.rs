use super::HostTransport;
use anyhow::Result;

/// Native Linux transport: runs commands directly through a shell, no
/// virtualization layer needed since distrobox runs natively here.
#[allow(dead_code)]
pub struct LinuxHost;

impl HostTransport for LinuxHost {
    fn name(&self) -> &'static str {
        "linux (native)"
    }

    fn command_parts(&self, script: &str) -> Result<(String, Vec<String>)> {
        Ok(("sh".to_string(), vec!["-c".to_string(), script.to_string()]))
    }
}
