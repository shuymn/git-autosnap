use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct DiffCommand {
    pub commit1: Option<String>,
    pub commit2: Option<String>,
    pub interactive: bool,
    pub stat: bool,
    pub name_only: bool,
    pub name_status: bool,
    pub paths: Vec<String>,
}

impl Command for DiffCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        let format = if self.stat {
            crate::core::git::DiffFormat::Stat
        } else if self.name_only {
            crate::core::git::DiffFormat::NameOnly
        } else if self.name_status {
            crate::core::git::DiffFormat::NameStatus
        } else {
            crate::core::git::DiffFormat::Unified
        };

        crate::core::git::diff(
            &ctx.repo_root,
            self.commit1.as_deref(),
            self.commit2.as_deref(),
            self.interactive,
            format,
            &self.paths,
        )
    }
}
