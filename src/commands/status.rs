use anyhow::Result;

use super::Command;
use crate::app::context::AppContext;

pub struct StatusCommand;

impl Command for StatusCommand {
    fn run(&self, ctx: &AppContext) -> Result<()> {
        let running = crate::core::runtime::process::status(&ctx.repo_root)?;
        if running {
            println!("running");
            std::process::exit(0);
        } else {
            println!("stopped");
            std::process::exit(1);
        }
    }
}
