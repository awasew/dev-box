mod cli;
mod config;
mod engine;
mod host;
mod sshd;

use anyhow::{Context, Result};
use clap::Parser;
use cli::{Cli, Command};
use engine::{distrobox::DistroboxEngine, ContainerEngine};

fn main() -> Result<()> {
    let cli = Cli::parse();

    let layers = if cli.config.is_empty() {
        config::default_layers()
    } else {
        cli.config.clone()
    };

    match cli.command {
        Command::Config => {
            let merged = config::merge_layers(&layers)?;
            print!("{}", config::to_ini_string(&merged)?);
        }
        Command::Up { dry_run } => {
            let merged = config::merge_layers(&layers)?;
            let payload = config::to_ini_string(&merged)?;

            if dry_run {
                println!("=== merged configuration (dry run) ===");
                print!("{payload}");
                return Ok(());
            }

            let box_name = merged
                .get_from(Some("dev-environment"), "name")
                .context("no [dev-environment] name= found in the merged configuration")?
                .to_string();

            let host = host::detect();
            eprintln!("==> host: {}", host.name());

            let engine = DistroboxEngine;
            engine
                .assemble(host.as_ref(), &payload)
                .context("failed to assemble the dev-box environment")?;

            eprintln!("==> wiring up keyless SSH access for {box_name}");
            let absolute_layers = layers
                .iter()
                .map(|p| sshd::to_absolute(p))
                .collect::<Result<Vec<_>>>()
                .context("failed to resolve configuration layer paths")?;
            sshd::install_client_config(&box_name, &absolute_layers)
                .context("failed to update the local SSH client configuration")?;

            eprintln!("==> dev-box environment is ready");
            eprintln!("==> connect with: ssh {box_name}");
        }
        Command::Enter { name } => {
            let merged = config::merge_layers(&layers)?;
            let box_name = match name {
                Some(n) => n,
                None => merged
                    .get_from(Some("dev-environment"), "name")
                    .map(str::to_string)
                    .context("no box name given and no [dev-environment] name= found in config")?,
            };
            let forwarded_env = config::forwarded_env_from(&merged);

            let host = host::detect();
            let engine = DistroboxEngine;
            engine.enter(host.as_ref(), &box_name, &forwarded_env)?;
        }
        Command::SshProxy { name } => {
            let merged = config::merge_layers(&layers)?;
            let forwarded_env = config::forwarded_env_from(&merged);

            let host = host::detect();
            sshd::run(host, name, forwarded_env)?;
        }
    }

    Ok(())
}
