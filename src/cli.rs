use clap::{ArgAction, Parser, Subcommand};

/// git-autosnap command-line interface
#[derive(Parser, Debug, Clone)]
#[command(name = "git-autosnap", version, about = "Record working tree snapshots in a local bare repo", long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv). `RUST_LOG` overrides this.
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
    Once {
        /// Optional message to include in the snapshot commit
        #[arg(value_name = "MESSAGE")]
        message: Option<String>,
    },

    /// Compact old snapshot history into a single baseline commit
    Compact {
        /// Compact snapshots older than N days (defaults to autosnap.compact.days)
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

    /// Restore files from a snapshot to the working tree
    Restore {
        /// Commit SHA or ref to restore from (defaults to HEAD/latest)
        #[arg(value_name = "COMMIT")]
        commit: Option<String>,

        /// Interactive mode: select commit from list using skim
        #[arg(short, long)]
        interactive: bool,

        /// Force restore even if there are uncommitted changes
        #[arg(long)]
        force: bool,

        /// Preview changes without actually restoring
        #[arg(long)]
        dry_run: bool,

        /// Full restore: remove files not present in snapshot
        #[arg(long)]
        full: bool,

        /// Specific paths to restore (if empty, restores all)
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },

    /// Show diff between snapshots or working tree
    Diff {
        /// First commit SHA or ref (defaults to working tree if only one commit provided)
        #[arg(value_name = "COMMIT1")]
        commit1: Option<String>,

        /// Second commit SHA or ref (defaults to HEAD if not provided)
        #[arg(value_name = "COMMIT2")]
        commit2: Option<String>,

        /// Interactive mode: select commits from list using skim
        #[arg(short, long)]
        interactive: bool,

        /// Show only stats (files changed, insertions, deletions)
        #[arg(long, conflicts_with = "name_only", conflicts_with = "name_status")]
        stat: bool,

        /// Show only names of changed files
        #[arg(long, conflicts_with = "stat", conflicts_with = "name_status")]
        name_only: bool,

        /// Show names and status of changed files
        #[arg(long, conflicts_with = "stat", conflicts_with = "name_only")]
        name_status: bool,

        /// Specific paths to diff (if empty, diffs all)
        #[arg(value_name = "PATH")]
        paths: Vec<String>,
    },

    /// View watcher logs
    Logs {
        /// Follow log output (like tail -f)
        #[arg(short, long)]
        follow: bool,

        /// Number of lines to show (default: 100)
        #[arg(short = 'n', long, default_value = "100")]
        lines: usize,
    },
}
