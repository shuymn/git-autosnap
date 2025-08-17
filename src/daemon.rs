use anyhow::{bail, Result};
use std::path::Path;
use crate::config::AutosnapConfig;

/// Start the watcher in background (daemonize). Placeholder.
pub fn start_daemon(_repo_root: &Path, _cfg: &AutosnapConfig) -> Result<()> {
    bail!("daemon mode not implemented yet")
}

/// Stop the running daemon via pidfile and signal. Placeholder.
pub fn stop() -> Result<()> {
    bail!("stop not implemented yet")
}

