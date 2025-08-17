use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{Context, Result};

/// Path to the PID file inside `.autosnap`.
pub fn pid_file(repo_root: &Path) -> PathBuf {
    repo_root.join(".autosnap").join("autosnap.pid")
}

/// Print running status. Placeholder implementation.
pub fn status(repo_root: &Path) -> Result<()> {
    let pid_path = pid_file(repo_root);
    if !pid_path.exists() {
        println!("stopped");
        return Ok(());
    }
    let content = fs::read_to_string(&pid_path)
        .with_context(|| format!("failed to read {}", pid_path.display()))?;
    let pid = content.trim();
    println!("running (pid={})", pid);
    Ok(())
}

/// Remove `.autosnap` directory after stopping the daemon.
/// Placeholder implementation.
pub fn uninstall(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".autosnap");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
        println!("Removed {}", dir.display());
    } else {
        println!("Nothing to remove at {}", dir.display());
    }
    Ok(())
}
