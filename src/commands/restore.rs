use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct RestoreCommand<'a> {
    pub commit: Option<&'a str>,
    pub interactive: bool,
    pub force: bool,
    pub apply: RestoreApply,
    pub mode: RestoreMode,
    pub paths: &'a [String],
}

#[derive(Clone, Copy, Debug)]
pub enum RestoreApply {
    Apply,
    DryRun,
}

#[derive(Clone, Copy, Debug)]
pub enum RestoreMode {
    Overlay,
    Full,
}

impl Command for RestoreCommand<'_> {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        let dry_run = matches!(self.apply, RestoreApply::DryRun);
        let full = matches!(self.mode, RestoreMode::Full);
        crate::core::git::restore(
            &ctx.repo_root,
            self.commit,
            self.interactive,
            self.force,
            dry_run,
            full,
            self.paths,
        )
    }
}
