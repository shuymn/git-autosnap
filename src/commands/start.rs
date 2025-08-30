use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct StartCommand {
    pub daemon: bool,
}

impl Command for StartCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        if self.daemon {
            crate::core::runtime::daemon::start_daemon(&ctx.repo_root, &ctx.cfg)
        } else {
            crate::core::runtime::watcher::start_foreground(&ctx.repo_root, &ctx.cfg)
        }
    }
}
