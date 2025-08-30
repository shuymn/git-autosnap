use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct ShellCommand {
    pub commit: Option<String>,
    pub interactive: bool,
}

impl Command for ShellCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::git::snapshot_shell(&ctx.repo_root, self.commit.as_deref(), self.interactive)
    }
}
