use anyhow::Result;
use clap::Parser;
use git_autosnap::cli::{Cli, Commands};
use git_autosnap::core::git;
use git_autosnap::logging::init::{init_tracing, init_tracing_with_file};
use git_autosnap::run;

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing based on the command
    // Start command needs file logging for the watcher
    match &cli.command {
        Commands::Start { daemon } => {
            let root = git::repo_root()?;
            init_tracing_with_file(&root, cli.verbose, *daemon)?;
        }
        _ => {
            init_tracing(cli.verbose)?;
        }
    }

    run(cli)
}
