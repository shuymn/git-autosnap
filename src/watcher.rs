use std::path::Path;
use anyhow::{bail, Result};
use crate::config::AutosnapConfig;

/// Start the foreground watcher loop.
/// Placeholder implementation; real implementation will use `watchexec` with gitignore support.
pub fn start_foreground(_repo_root: &Path, _cfg: &AutosnapConfig) -> Result<()> {
    bail!("watcher (foreground) not implemented yet")
}

