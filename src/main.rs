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
        std::env::set_var(
            "RUST_LOG",
            if cli.verbose { "debug" } else { "warn" },
        );
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

        Command::Up { dry_run, scratchpad } => {
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
                .map_err(|e| e.context("failed to assemble the dev-box environment"))?;

            let use_scratchpad = scratchpad || config::is_scratchpad_enabled(&ctx.config);
            if use_scratchpad && cfg!(target_os = "windows") {
                if let Ok(cwd) = std::env::current_dir() {
                    let _ = host::scratchpad::sync_to_scratchpad(ctx.host.as_ref(), &cwd, &box_name);
                }
            }

            eprintln!("==> wiring up keyless SSH access for {box_name}");
            let abs_layers = sshd::absolute_layers(&layers)?;
            sshd::install_client_config(&box_name, &abs_layers)
                .map_err(|e| e.context("failed to update the local SSH client configuration"))?;

            eprintln!("==> dev-box environment is ready");
            eprintln!("==> connect with: ssh {box_name}");
        }

        Command::Enter { name, scratchpad } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name()
                    .map_err(|e| e.context("no box name given and no [dev-environment] name= found in config"))?,
            };
            let forwarded_env = ctx.forwarded_env();
            let use_scratchpad = scratchpad || config::is_scratchpad_enabled(&ctx.config);

            if use_scratchpad && cfg!(target_os = "windows") {
                let cwd = std::env::current_dir()?;
                host::scratchpad::sync_to_scratchpad(ctx.host.as_ref(), &cwd, &box_name)?;
                let work_dir = host::scratchpad::scratchpad_path(&box_name);

                let enter_res = ctx.engine.enter(
                    ctx.host.as_ref(),
                    &box_name,
                    &forwarded_env,
                    Some(&work_dir),
                );

                // On session exit, sync modified source files back to Windows host.
                let _ = host::scratchpad::sync_from_scratchpad(ctx.host.as_ref(), &cwd, &box_name);
                enter_res?;
            } else {
                ctx.engine.enter(ctx.host.as_ref(), &box_name, &forwarded_env, None)?;
            }
        }

        Command::Sync { name, reverse } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name()
                    .map_err(|e| e.context("no box name given and no [dev-environment] name= found in config"))?,
            };
            let cwd = std::env::current_dir()?;
            if reverse {
                host::scratchpad::sync_from_scratchpad(ctx.host.as_ref(), &cwd, &box_name)?;
            } else {
                host::scratchpad::sync_to_scratchpad(ctx.host.as_ref(), &cwd, &box_name)?;
            }
        }

        Command::List => {
            ctx.engine
                .list(ctx.host.as_ref())
                .map_err(|e| e.context("failed to list dev-box environments"))?;
        }

        Command::Stop { name } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name()
                    .map_err(|e| e.context("no box name given and no [dev-environment] name= found in config"))?,
            };
            eprintln!("==> stopping dev-box environment: {box_name}");
            ctx.engine
                .stop(ctx.host.as_ref(), &box_name)
                .map_err(|e| e.context(format!("failed to stop {box_name}")))?;
        }

        Command::Rm { name, force } => {
            let box_name = match name {
                Some(n) => n,
                None => ctx.box_name()
                    .map_err(|e| e.context("no box name given and no [dev-environment] name= found in config"))?,
            };
            eprintln!("==> removing dev-box environment: {box_name}");
            ctx.engine
                .rm(ctx.host.as_ref(), &box_name, force)
                .map_err(|e| e.context(format!("failed to remove {box_name}")))?;
            let _ = sshd::remove_client_config(&box_name);
            let _ = host::scratchpad::clean_scratchpad(ctx.host.as_ref(), &box_name);
        }

        Command::SshProxy { name } => {
            let forwarded_env = ctx.forwarded_env();
            let work_dir = if config::is_scratchpad_enabled(&ctx.config) && cfg!(target_os = "windows") {
                Some(host::scratchpad::scratchpad_path(&name))
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
