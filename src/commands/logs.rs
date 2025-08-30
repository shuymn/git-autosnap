use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct LogsCommand {
    pub follow: bool,
    pub lines: usize,
}

impl Command for LogsCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        // Use tokio runtime for async file operations
        let rt = tokio::runtime::Runtime::new()?;
        rt.block_on(crate::logging::view::show_logs(
            &ctx.repo_root,
            self.follow,
            self.lines,
        ))
    }
}
