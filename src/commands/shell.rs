use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct ShellCommand<'a> {
    pub commit: Option<&'a str>,
    pub interactive: bool,
}

impl Command for ShellCommand<'_> {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::git::snapshot_shell(&ctx.repo_root, self.commit, self.interactive)
    }
}
