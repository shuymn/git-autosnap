pub mod cli;

use anyhow::{Context, Result};
use tracing::info;

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
    match cli.command {
        cli::Commands::Greet { name, upper } => {
            let msg = format!("Hello, {name}!");
            let out = if upper { msg.to_uppercase() } else { msg };
            println!("{out}");
            info!(event = "greet", len = out.len());
        }
        cli::Commands::Sum { values } => {
            let sum: i64 = values.iter().copied().sum();
            println!("{sum}");
            info!(event = "sum", count = values.len(), sum);
        }
    }
    Ok(())
}
