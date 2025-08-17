use anyhow::Result;
use clap::Parser;
use git_autosnap::cli::Cli;
use git_autosnap::{init_tracing, run};

fn main() -> Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose)?;
    run(cli)
}
