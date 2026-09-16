mod cli;
mod config;
mod context;
mod engine;
mod host;
mod sshd;
mod util;

use anyhow::Result;
use clap::Parser;
use cli::{Cli, Command};
use config::to_ini_string;
use context::AppContext;
use std::sync::Arc;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialise logging. Always respect RUST_LOG if already set; otherwise
    // default to "warn" (quiet) unless --verbose was passed (sets "debug").
    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", if cli.verbose { "debug" } else { "warn" });
    }
    env_logger::init();

    let layers = if cli.config.is_empty() {
        config::default_layers()
    } else {
        cli.config.clone()
    };

    // Build the shared context once; every command arm borrows or clones
    // from it rather than re-running detection and config merge themselves.
    let ctx = AppContext::new(&layers)?;

    match cli.command {
        Command::Config => {
            print!("{}", to_ini_string(&ctx.config)?);
        }

        Command::Up {
            dry_run,
            scratchpad,
        } => {
            let payload = to_ini_string(&ctx.config)?;

            if dry_run {
                println!("=== merged configuration (dry run) ===");
                print!("{payload}");
                return Ok(());
            }

            let box_name = ctx.box_name()?;

            eprintln!("==> host: {}", ctx.host.name());
            ctx.engine
                .assemble(ctx.host.as_ref(), &payload)
                .map_err(|e| e.context("failed to assemble the dbx environment"))?;

            let use_scratchpad = scratchpad || config::is_scratchpad_enabled(&ctx.config);
            if use_scratchpad {
                match std::env::current_dir() {
                    Ok(cwd) => {
                        if let Err(e) = ctx.host.sync_to_scratchpad(&cwd, &box_name) {
                            eprintln!("==> warning: scratchpad sync failed: {e:#}");
                        }
                    }
                    Err(e) => {
                        eprintln!("==> warning: could not determine current directory for scratchpad sync: {e:#}");
                    }
                }
            }

            eprintln!("==> wiring up keyless SSH access for {box_name}");
            let abs_layers = sshd::absolute_layers(&layers)?;
            sshd::install_client_config(&box_name, &abs_layers)
                .map_err(|e| e.context("failed to update the local SSH client configuration"))?;

            eprintln!("==> dbx environment is ready");
            eprintln!("==> connect with: ssh {box_name}");
        }

        Command::Enter { name, scratchpad } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name().map_err(|e| {
                    e.context("no box name given and no [dev-environment] name= found in config")
                })?,
            };
            let forwarded_env = ctx.forwarded_env();
            let use_scratchpad = scratchpad || config::is_scratchpad_enabled(&ctx.config);

            if use_scratchpad {
                let cwd = std::env::current_dir()?;
                ctx.host.sync_to_scratchpad(&cwd, &box_name)?;
                let work_dir = ctx.host.scratchpad_work_dir(&box_name);

                let enter_res = ctx.engine.enter(
                    ctx.host.as_ref(),
                    &box_name,
                    &forwarded_env,
                    work_dir.as_deref(),
                );

                // On session exit, sync modified source files back to host.
                if let Err(e) = ctx.host.sync_from_scratchpad(&cwd, &box_name) {
                    eprintln!("==> warning: failed to sync changes back from scratchpad: {e:#}");
                }
                enter_res?;
            } else {
                ctx.engine
                    .enter(ctx.host.as_ref(), &box_name, &forwarded_env, None)?;
            }
        }

        Command::Sync { name, reverse } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name().map_err(|e| {
                    e.context("no box name given and no [dev-environment] name= found in config")
                })?,
            };
            let cwd = std::env::current_dir()?;
            if reverse {
                ctx.host.sync_from_scratchpad(&cwd, &box_name)?;
            } else {
                ctx.host.sync_to_scratchpad(&cwd, &box_name)?;
            }
        }

        Command::List => {
            ctx.engine
                .list(ctx.host.as_ref())
                .map_err(|e| e.context("failed to list dbx environments"))?;
        }

        Command::Stop { name } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name().map_err(|e| {
                    e.context("no box name given and no [dev-environment] name= found in config")
                })?,
            };
            eprintln!("==> stopping dbx environment: {box_name}");
            ctx.engine
                .stop(ctx.host.as_ref(), &box_name)
                .map_err(|e| e.context(format!("failed to stop {box_name}")))?;
        }

        Command::Rm { name, force } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name().map_err(|e| {
                    e.context("no box name given and no [dev-environment] name= found in config")
                })?,
            };
            eprintln!("==> removing dbx environment: {box_name}");
            ctx.engine
                .rm(ctx.host.as_ref(), &box_name, force)
                .map_err(|e| e.context(format!("failed to remove {box_name}")))?;
            if let Err(e) = sshd::remove_client_config(&box_name) {
                eprintln!("==> warning: failed to remove SSH client config for {box_name}: {e:#}");
            }
            if let Err(e) = ctx.host.clean_scratchpad(&box_name) {
                eprintln!("==> warning: failed to clean scratchpad for {box_name}: {e:#}");
            }
        }

        Command::SshProxy { name } => {
            let forwarded_env = ctx.forwarded_env();
            let work_dir = if config::is_scratchpad_enabled(&ctx.config) {
                ctx.host.scratchpad_work_dir(&name)
            } else {
                None
            };
            sshd::run(
                Arc::clone(&ctx.host),
                name,
                forwarded_env,
                Arc::clone(&ctx.engine),
                work_dir,
            )?;
        }
    }

    Ok(())
}
