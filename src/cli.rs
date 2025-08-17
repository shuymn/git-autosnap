use clap::{ArgAction, Parser, Subcommand};

/// A minimal, production-ready CLI template.
#[derive(Parser, Debug, Clone)]
#[command(name = "git-autosnap", version, about = "Example CLI", long_about = None)]
pub struct Cli {
    /// Increase verbosity (-v, -vv, -vvv). RUST_LOG overrides this.
    #[arg(short, long, action = ArgAction::Count)]
    pub verbose: u8,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Print a greeting
    Greet {
        /// Name to greet
        #[arg(value_name = "NAME")]
        name: String,

        /// Uppercase output
        #[arg(long)]
        upper: bool,
    },

    /// Sum integers and print the result
    Sum {
        /// One or more integers
        #[arg(value_name = "N", num_args = 1.., required = true)]
        values: Vec<i64>,
    },
}
