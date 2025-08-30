use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct OnceCommand<'a> {
    pub message: Option<&'a str>,
}

impl<'a> Command for OnceCommand<'a> {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        if let Some(hash) = crate::core::git::snapshot_once(&ctx.repo_root, self.message)? {
            println!("{}", hash);
        }
        Ok(())
    }
}
