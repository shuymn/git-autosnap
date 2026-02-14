use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct CompactCommand {
    pub days: Option<u32>,
}

impl Command for CompactCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        let days = self.days.unwrap_or(ctx.cfg.compact_days);
        let result = crate::core::git::compact(&ctx.repo_root, days)?;

        if result.rewritten {
            println!(
                "compacted snapshots: {} -> {} commits",
                result.before_commits, result.after_commits
            );
        } else {
            println!("no rewrite needed ({} commits)", result.before_commits);
        }

        Ok(())
    }
}
