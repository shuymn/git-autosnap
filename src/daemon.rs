use anyhow::{bail, Context, Result};
use std::path::Path;
use std::fs;
use std::process::Command;
use crate::process::pid_file;
use crate::config::AutosnapConfig;

/// Start the watcher in background (daemonize). Placeholder.
pub fn start_daemon(_repo_root: &Path, _cfg: &AutosnapConfig) -> Result<()> {
    bail!("daemon mode not implemented yet")
}

/// Stop the running daemon via pidfile and signal. Placeholder.
pub fn stop(repo_root: &Path) -> Result<()> {
    let pid_path = pid_file(repo_root);
    if !pid_path.exists() {
        // already stopped
        println!("stopped");
        return Ok(());
    }
    let pid = fs::read_to_string(&pid_path)
        .with_context(|| format!("failed to read {}", pid_path.display()))?;
    let pid = pid.trim();
    // Best-effort send SIGTERM using system kill; Unix-only behavior assumed
    let status = Command::new("/bin/kill").arg("-TERM").arg(pid).status();
    match status {
        Ok(s) if s.success() => {
            println!("sent SIGTERM to {}", pid);
            Ok(())
        }
        Ok(s) => bail!("kill exited with status: {}", s),
        Err(e) => Err(e).context("failed to execute kill")
    }
}
