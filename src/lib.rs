pub mod cli;
pub mod config;
pub mod daemon;
pub mod gitlayer;
pub mod logs;
pub mod process;
pub mod watcher;

use anyhow::{Context, Result};
use std::path::Path;
use std::sync::OnceLock;

// Global guard to keep the file appender alive
static FILE_APPENDER_GUARD: OnceLock<tracing_appender::non_blocking::WorkerGuard> = OnceLock::new();

/// Initialize tracing. RUST_LOG (if set) takes precedence.
/// Otherwise, -v/-vv map to "debug"/"trace".
pub fn init_tracing(verbosity: u8) -> Result<()> {
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let base = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| base.to_string());

    let fmt_layer = fmt::layer().with_target(false);
    let filter_layer = EnvFilter::try_new(filter).context("invalid RUST_LOG / filter")?;

    // Allow re-init to be a no-op in tests
    let _ = tracing_subscriber::registry()
        .with(filter_layer)
        .with(fmt_layer)
        .try_init();

    Ok(())
}

/// Initialize tracing with file logging. RUST_LOG (if set) takes precedence.
/// Otherwise, -v/-vv map to "debug"/"trace".
pub fn init_tracing_with_file(repo_root: &Path, verbosity: u8, is_daemon: bool) -> Result<()> {
    use tracing_appender::rolling;
    use tracing_subscriber::{EnvFilter, fmt, prelude::*};

    let base = match verbosity {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| base.to_string());
    let filter_layer = EnvFilter::try_new(filter).context("invalid RUST_LOG / filter")?;

    let log_dir = repo_root.join(".autosnap");
    let file_appender = rolling::daily(log_dir, "autosnap.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    // Store the guard globally to keep it alive for the program duration
    FILE_APPENDER_GUARD.set(guard).ok();

    let file_layer = fmt::layer()
        .with_ansi(false)
        .with_target(false)
        .with_writer(non_blocking);

    if is_daemon {
        // File-only logging for daemon mode
        let _ = tracing_subscriber::registry()
            .with(filter_layer)
            .with(file_layer)
            .try_init();
    } else {
        // Dual logging (file + console) for foreground mode
        let console_layer = fmt::layer().with_target(false);

        let _ = tracing_subscriber::registry()
            .with(filter_layer)
            .with(file_layer)
            .with(console_layer)
            .try_init();
    }

    Ok(())
}

/// Entry point for CLI subcommands.
pub fn run(cli: cli::Cli) -> Result<()> {
    use cli::Commands;

    match cli.command {
        Commands::Init => {
            let root = gitlayer::repo_root()?;
            gitlayer::init_autosnap(&root)?;
            println!("Initialized .autosnap in {}", root.display());
        }
        Commands::Start { daemon } => {
            let root = gitlayer::repo_root()?;
            let cfg = config::AutosnapConfig::load(&root)?;
            if daemon {
                daemon::start_daemon(&root, &cfg)?;
            } else {
                watcher::start_foreground(&root, &cfg)?;
            }
        }
        Commands::Stop => {
            let root = gitlayer::repo_root()?;
            daemon::stop(&root)?;
        }
        Commands::Status => {
            let root = gitlayer::repo_root()?;
            let running = process::status(&root)?;
            if running {
                println!("running");
                std::process::exit(0);
            } else {
                println!("stopped");
                std::process::exit(1);
            }
        }
        Commands::Once { message } => {
            let root = gitlayer::repo_root()?;
            gitlayer::snapshot_once(&root, message.as_deref())?;
        }
        Commands::Gc { days, prune } => {
            let root = gitlayer::repo_root()?;
            if prune {
                // Pruning mode: remove old snapshots
                let mut cfg = config::AutosnapConfig::load(&root)?;
                if let Some(d) = days {
                    cfg.prune_days = d;
                }
                gitlayer::gc(&root, true, Some(cfg.prune_days))?;
            } else {
                // Compression only mode: just pack objects
                if days.is_some() {
                    eprintln!("Warning: --days is ignored without --prune");
                }
                gitlayer::gc(&root, false, None)?;
            }
        }
        Commands::Uninstall => {
            let root = gitlayer::repo_root()?;
            let _ = daemon::stop(&root);
            process::uninstall(&root)?;
        }
        Commands::Shell {
            commit,
            interactive,
        } => {
            let root = gitlayer::repo_root()?;
            gitlayer::snapshot_shell(&root, commit.as_deref(), interactive)?;
        }
        Commands::Restore {
            commit,
            interactive,
            force,
            dry_run,
            full,
            paths,
        } => {
            let root = gitlayer::repo_root()?;
            gitlayer::restore(
                &root,
                commit.as_deref(),
                interactive,
                force,
                dry_run,
                full,
                &paths,
            )?;
        }
        Commands::Diff {
            commit1,
            commit2,
            interactive,
            stat,
            name_only,
            name_status,
            paths,
        } => {
            let root = gitlayer::repo_root()?;

            // Determine output format
            let format = if stat {
                gitlayer::DiffFormat::Stat
            } else if name_only {
                gitlayer::DiffFormat::NameOnly
            } else if name_status {
                gitlayer::DiffFormat::NameStatus
            } else {
                gitlayer::DiffFormat::Unified
            };

            gitlayer::diff(
                &root,
                commit1.as_deref(),
                commit2.as_deref(),
                interactive,
                format,
                &paths,
            )?;
        }
        Commands::Logs { follow, lines } => {
            let repo_root = gitlayer::repo_root()?;

            // Use tokio runtime for async file operations
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(logs::show_logs(&repo_root, follow, lines))?;
        }
    }
    Ok(())
}
