use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "dbx",
    author,
    version,
    about = "A fast, IDE-independent developer environment orchestrator built on Distrobox",
    long_about = None
)]
pub struct Cli {
    /// Explicit configuration layers to merge, in order (lowest to highest priority).
    /// Defaults to: ~/.config/dbx/global.ini -> ./dbx.ini -> ./dbx.local.ini
    #[arg(short = 'c', long = "config", global = true)]
    pub config: Vec<PathBuf>,

    /// Enable verbose debug logging (sets RUST_LOG=debug).
    #[arg(short = 'v', long = "verbose", global = true)]
    pub verbose: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Merge configuration layers and create/update the dbx environment.
    #[command(alias = "create")]
    Up {
        #[arg(long)]
        dry_run: bool,
        /// On Windows, prepare a high-speed WSL2 ext4 scratchpad workspace.
        #[arg(long)]
        scratchpad: bool,
    },
    /// Enter an already-created dbx environment.
    Enter {
        name: Option<String>,
        /// On Windows, enter inside the high-speed WSL2 ext4 scratchpad workspace.
        #[arg(long)]
        scratchpad: bool,
    },
    /// Synchronize project files between Windows host and WSL2 ext4 scratchpad.
    Sync {
        /// Name of the box (defaults to [dev-environment] name from config).
        name: Option<String>,
        /// Reverse sync: pull changes from WSL scratchpad back to Windows host.
        #[arg(short, long)]
        reverse: bool,
    },
    /// List all existing dbx / distrobox containers.
    #[command(alias = "ls")]
    List,
    /// Stop a running dbx environment.
    Stop {
        /// Name of the box to stop (defaults to [dev-environment] name from config).
        name: Option<String>,
    },
    /// Remove a dbx environment.
    #[command(alias = "delete")]
    Rm {
        /// Name of the box to remove (defaults to [dev-environment] name from config).
        name: Option<String>,
        /// Force removal without confirmation prompt.
        #[arg(short, long)]
        force: bool,
    },
    /// Print the merged configuration without doing anything else.
    Config,
    /// Internal: runs sshd in inetd mode inside a box, speaking raw SSH
    /// protocol over stdin/stdout. This is meant to be used as an SSH
    /// `ProxyCommand` target (dbx wires it up automatically) -- you
    /// shouldn't need to run this directly.
    #[command(hide = true)]
    SshProxy { name: String },
}
