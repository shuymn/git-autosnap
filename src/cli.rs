use clap::{ArgAction, Parser, Subcommand};

/// git-autosnap command-line interface
#[derive(Parser, Debug, Clone)]
#[command(name = "git-autosnap", version, about = "Record working tree snapshots in a local bare repo", long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv). RUST_LOG overrides this.
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Initialize .autosnap bare repository in the current Git repo
    Init,

    /// Launch watcher (foreground by default)
    Start {
        /// Detach and run as a background daemon
        #[arg(long)]
        daemon: bool,
    },

    /// Stop background watcher (reads PID from .autosnap/autosnap.pid)
    Stop,

    /// Exit 0 if running, non-zero otherwise
    Status,

    /// Take one snapshot and exit
    Once,

    /// Prune snapshots older than N days (default: 60)
    Gc {
        /// Retention in days
        #[arg(long, value_name = "DAYS")]
        days: Option<u32>,
    },

    /// Stop watcher (if running) and remove .autosnap directory
    Uninstall,

    /// Open a snapshot in a subshell for exploration
    Shell {
        /// Commit SHA or ref to explore (defaults to HEAD/latest)
        #[arg(value_name = "COMMIT")]
        commit: Option<String>,

        /// Interactive mode: select commit from list using skim
        #[arg(short, long)]
        interactive: bool,
    },
}
