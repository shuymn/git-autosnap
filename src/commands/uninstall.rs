use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct UninstallCommand;

impl Command for UninstallCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        let _ = crate::core::runtime::daemon::stop(&ctx.repo_root);
        crate::core::runtime::process::uninstall(&ctx.repo_root)
    }
}
