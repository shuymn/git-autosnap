use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct RestoreCommand {
    pub commit: Option<String>,
    pub interactive: bool,
    pub force: bool,
    pub dry_run: bool,
    pub full: bool,
    pub paths: Vec<String>,
}

impl Command for RestoreCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::git::restore(
            &ctx.repo_root,
            self.commit.as_deref(),
            self.interactive,
            self.force,
            self.dry_run,
            self.full,
            &self.paths,
        )
    }
}
