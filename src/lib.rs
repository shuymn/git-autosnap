pub mod app;
pub mod cli;
pub mod commands;
pub mod config;
pub mod core;
pub mod logging;
use anyhow::Result;

/// Entry point for CLI subcommands: delegate to commands::dispatch
pub fn run(cli: cli::Cli) -> Result<()> {
    commands::dispatch(cli)
}
