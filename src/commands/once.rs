use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct OnceCommand {
    pub message: Option<String>,
}

impl Command for OnceCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        if let Some(hash) =
            crate::core::git::snapshot_once(&ctx.repo_root, self.message.as_deref())?
        {
            println!("{}", hash);
        }
        Ok(())
    }
}
