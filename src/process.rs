use std::fs;
use std::path::{Path, PathBuf};
use anyhow::{bail, Result};

/// Path to the PID file inside `.autosnap`.
pub fn pid_file(repo_root: &Path) -> PathBuf {
    repo_root.join(".autosnap").join("autosnap.pid")
}

/// Print running status. Placeholder implementation.
pub fn status() -> Result<()> {
    // TODO: Implement real status using pidfile + liveness probe.
    println!("git-autosnap status: not implemented");
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

