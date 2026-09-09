use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "dev-box",
    author,
    version,
    about = "A fast, IDE-independent developer environment orchestrator built on Distrobox",
    long_about = None
)]
pub struct Cli {
    /// Explicit configuration layers to merge, in order (lowest to highest priority).
    /// Defaults to: ~/.config/dev-box/global.ini -> ./devbox.ini -> ./devbox.local.ini
    #[arg(short = 'c', long = "config", global = true)]
    pub config: Vec<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Merge configuration layers and create/update the dev-box environment.
    Up {
        #[arg(long)]
        dry_run: bool,
    },
    /// Enter an already-created dev-box environment.
    Enter { name: Option<String> },
    /// Print the merged configuration without doing anything else.
    Config,
    /// Internal: runs sshd in inetd mode inside a box, speaking raw SSH
    /// protocol over stdin/stdout. This is meant to be used as an SSH
    /// `ProxyCommand` target (dev-box wires it up automatically) -- you
    /// shouldn't need to run this directly.
    #[command(hide = true)]
    SshProxy { name: String },
}
