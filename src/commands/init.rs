use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct InitCommand;

impl Command for InitCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        crate::core::git::init_autosnap(&ctx.repo_root)?;
        println!("Initialized .autosnap in {}", ctx.repo_root.display());
        Ok(())
    }
}
