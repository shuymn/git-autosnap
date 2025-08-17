pub mod cli;
pub mod config;
pub mod gitlayer;
pub mod watcher;
pub mod daemon;
pub mod process;

use anyhow::{Context, Result};
use tracing::warn;

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
        Commands::Once => {
            let root = gitlayer::repo_root()?;
            gitlayer::snapshot_once(&root)?;
        }
        Commands::Gc { days } => {
            let root = gitlayer::repo_root()?;
            let mut cfg = config::AutosnapConfig::load(&root)?;
            if let Some(d) = days { cfg.prune_days = d; }
            gitlayer::gc(&root, cfg.prune_days)?;
        }
        Commands::Uninstall => {
            let root = gitlayer::repo_root()?;
            let _ = daemon::stop(&root);
            process::uninstall(&root)?;
        }
    }
    Ok(())
}
