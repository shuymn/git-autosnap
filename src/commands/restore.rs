use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct RestoreCommand<'a> {
    pub commit: Option<&'a str>,
    pub interactive: bool,
    pub force: bool,
    pub dry_run: bool,
    pub full: bool,
    pub paths: &'a [String],
}

impl<'a> Command for RestoreCommand<'a> {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::git::restore(
            &ctx.repo_root,
            self.commit,
            self.interactive,
            self.force,
            self.dry_run,
            self.full,
            self.paths,
        )
    }
}
