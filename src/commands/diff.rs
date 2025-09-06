use anyhow::Result;

use super::Command;
use crate::{app::context::AppContext, core::git::DiffFormat};

pub struct DiffCommand<'a> {
    pub commit1: Option<&'a str>,
    pub commit2: Option<&'a str>,
    pub interactive: bool,
    pub format: DiffFormat,
    pub paths: &'a [String],
}

impl Command for DiffCommand<'_> {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::git::diff(
            &ctx.repo_root,
            self.commit1,
            self.commit2,
            self.interactive,
            self.format,
            self.paths,
        )
    }
}
