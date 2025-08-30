use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct GcCommand {
    pub days: Option<u32>,
    pub prune: bool,
}

impl Command for GcCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        if self.prune {
            // Pruning mode: remove old snapshots
            let mut cfg = ctx.cfg;
            if let Some(d) = self.days {
                cfg.prune_days = d;
            }
            crate::core::git::gc(&ctx.repo_root, true, Some(cfg.prune_days))
        } else {
            // Compression only mode: just pack objects
            if self.days.is_some() {
                eprintln!("Warning: --days is ignored without --prune");
            }
            crate::core::git::gc(&ctx.repo_root, false, None)
        }
    }
}
