use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct StopCommand;

impl Command for StopCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::runtime::daemon::stop(&ctx.repo_root)
    }
}
