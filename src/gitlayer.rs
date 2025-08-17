use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use git2::Repository;

/// Discover the current repository root directory.
pub fn repo_root() -> Result<PathBuf> {
    let repo = Repository::discover(".").context("not inside a Git repository")?;
    let workdir = repo
        .workdir()
        .context("repository has no working directory")?;
    Ok(workdir.to_path_buf())
}

/// Return the `.autosnap` directory path under the given repo root.
pub fn autosnap_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".autosnap")
}

/// Initialize the `.autosnap` bare repository if absent.
pub fn init_autosnap(repo_root: &Path) -> Result<()> {
    let path = autosnap_dir(repo_root);
    if path.exists() {
        return Ok(());
    }
    let _ = Repository::init_bare(&path)
        .with_context(|| format!("failed to init bare repo at {}", path.display()))?;
    Ok(())
}

/// Take a single snapshot of the working tree and commit it into `.autosnap`.
/// Not implemented yet.
pub fn snapshot_once(_repo_root: &Path) -> Result<()> {
    bail!("snapshot functionality not implemented yet")
}

/// Garbage collect (prune) snapshots older than the given number of days.
/// Not implemented yet.
pub fn gc(_repo_root: &Path, _prune_days: u32) -> Result<()> {
    bail!("gc not implemented yet")
}

