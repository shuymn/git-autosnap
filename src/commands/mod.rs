use anyhow::Result;

use crate::app::context::AppContext;
use crate::cli::{Cli, Commands as CliCommands};

pub mod diff;
pub mod gc;
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
    fn run(&self, ctx: &AppContext) -> Result<()>;
}

/// Central dispatcher: routes parsed CLI to subcommand handlers.
pub fn dispatch(cli: Cli) -> Result<()> {
    let ctx = AppContext::from_repo(cli.verbose)?;

    match &cli.command {
        CliCommands::Once { message } => {
            let cmd = once::OnceCommand {
                message: message.clone(),
            };
            cmd.run(&ctx)
        }
        CliCommands::Logs { follow, lines } => {
            let cmd = logs::LogsCommand {
                follow: *follow,
                lines: *lines,
            };
            cmd.run(&ctx)
        }
        CliCommands::Init => init::InitCommand.run(&ctx),
        CliCommands::Start { daemon } => {
            let cmd = start::StartCommand { daemon: *daemon };
            cmd.run(&ctx)
        }
        CliCommands::Stop => stop::StopCommand.run(&ctx),
        CliCommands::Status => status::StatusCommand.run(&ctx),
        CliCommands::Gc { days, prune } => {
            let cmd = gc::GcCommand {
                days: *days,
                prune: *prune,
            };
            cmd.run(&ctx)
        }
        CliCommands::Uninstall => uninstall::UninstallCommand.run(&ctx),
        CliCommands::Shell {
            commit,
            interactive,
        } => {
            let cmd = shell::ShellCommand {
                commit: commit.clone(),
                interactive: *interactive,
            };
            cmd.run(&ctx)
        }
        CliCommands::Restore {
            commit,
            interactive,
            force,
            dry_run,
            full,
            paths,
        } => {
            let cmd = restore::RestoreCommand {
                commit: commit.clone(),
                interactive: *interactive,
                force: *force,
                dry_run: *dry_run,
                full: *full,
                paths: paths.clone(),
            };
            cmd.run(&ctx)
        }
        CliCommands::Diff {
            commit1,
            commit2,
            interactive,
            stat,
            name_only,
            name_status,
            paths,
        } => {
            let cmd = diff::DiffCommand {
                commit1: commit1.clone(),
                commit2: commit2.clone(),
                interactive: *interactive,
                stat: *stat,
                name_only: *name_only,
                name_status: *name_status,
                paths: paths.clone(),
            };
            cmd.run(&ctx)
        } // All commands are handled explicitly above
    }
}
