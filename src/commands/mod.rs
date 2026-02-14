use anyhow::Result;

use crate::{
    app::context::AppContext,
    cli::{Cli, Commands},
};

pub mod compact;
pub mod diff;
pub mod init;
pub mod logs;
pub mod once;
pub mod restore;
pub mod shell;
pub mod start;
pub mod status;
pub mod stop;
pub mod uninstall;

/// Unified interface implemented by each subcommand handler.
pub trait Command {
    /// Execute the subcommand.
    ///
    /// # Errors
    /// Returns an error if the command fails.
    fn run(&self, ctx: &AppContext) -> Result<()>;
}

/// Central dispatcher: routes parsed CLI to subcommand handlers.
///
/// # Errors
/// Returns an error if the invoked subcommand fails.
pub fn dispatch(cli: &Cli) -> Result<()> {
    let ctx = AppContext::from_repo(cli.verbose)?;

    match &cli.command {
        Commands::Once { message } => {
            let cmd = once::OnceCommand {
                message: message.as_deref(),
            };
            cmd.run(&ctx)
        }
        Commands::Logs { follow, lines } => {
            let cmd = logs::LogsCommand {
                follow: *follow,
                lines: *lines,
            };
            cmd.run(&ctx)
        }
        Commands::Init => init::InitCommand.run(&ctx),
        Commands::Start { daemon } => {
            let cmd = start::StartCommand { daemon: *daemon };
            cmd.run(&ctx)
        }
        Commands::Stop => stop::StopCommand.run(&ctx),
        Commands::Status => status::StatusCommand.run(&ctx),
        Commands::Compact { days } => {
            let cmd = compact::CompactCommand { days: *days };
            cmd.run(&ctx)
        }
        Commands::Uninstall => uninstall::UninstallCommand.run(&ctx),
        Commands::Shell {
            commit,
            interactive,
        } => {
            let cmd = shell::ShellCommand {
                commit: commit.as_deref(),
                interactive: *interactive,
            };
            cmd.run(&ctx)
        }
        Commands::Restore {
            commit,
            interactive,
            force,
            dry_run,
            full,
            paths,
        } => {
            let cmd = restore::RestoreCommand {
                commit: commit.as_deref(),
                interactive: *interactive,
                force: *force,
                apply: if *dry_run {
                    restore::RestoreApply::DryRun
                } else {
                    restore::RestoreApply::Apply
                },
                mode: if *full {
                    restore::RestoreMode::Full
                } else {
                    restore::RestoreMode::Overlay
                },
                paths,
            };
            cmd.run(&ctx)
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
            let format = if *stat {
                crate::core::git::DiffFormat::Stat
            } else if *name_only {
                crate::core::git::DiffFormat::NameOnly
            } else if *name_status {
                crate::core::git::DiffFormat::NameStatus
            } else {
                crate::core::git::DiffFormat::Unified
            };

            let cmd = diff::DiffCommand {
                commit1: commit1.as_deref(),
                commit2: commit2.as_deref(),
                interactive: *interactive,
                format,
                paths,
            };
            cmd.run(&ctx)
        } // All commands are handled explicitly above
    }
}
