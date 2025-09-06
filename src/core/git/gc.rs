use std::{path::Path, process::Command};

use anyhow::{Context, Result};

use super::repo::autosnap_dir;

/// Garbage collect snapshots - compress objects and optionally prune old ones.
///
/// When `prune` is false, only compresses/packs objects without removing any snapshots.
/// When `prune` is true with `prune_days` set, removes snapshots older than the specified days.
///
/// # Errors
/// Returns an error if invoking `git` subcommands fails.
pub fn gc(repo_root: &Path, prune: bool, prune_days: Option<u32>) -> Result<()> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        return Ok(()); // nothing to do
    }

    let gitdir = autosnap.to_string_lossy().to_string();

    if prune {
        // Pruning mode: expire reflog and prune old objects
        let days = prune_days.unwrap_or(60);
        let expire = format!("{days}d");

        // First expire the reflog
        let status = Command::new("git")
            .args([
                format!("--git-dir={gitdir}").as_str(),
                "reflog",
                "expire",
                format!("--expire={expire}").as_str(),
                "--all",
            ])
            .status()
            .context("failed to run git reflog expire")?;
        if !status.success() {
            return Err(anyhow::anyhow!(
                "git reflog expire exited with status {status}"
            ));
        }

        // Then gc with pruning
        let status = Command::new("git")
            .args([
                format!("--git-dir={gitdir}").as_str(),
                "gc",
                format!("--prune={expire}").as_str(),
            ])
            .status()
            .context("failed to run git gc --prune")?;
        if !status.success() {
            return Err(anyhow::anyhow!(
                "git gc --prune exited with status {status}"
            ));
        }
    } else {
        // Compression-only mode: just pack objects without pruning
        let status = Command::new("git")
            .args([format!("--git-dir={gitdir}").as_str(), "gc"])
            .status()
            .context("failed to run git gc")?;
        if !status.success() {
            return Err(anyhow::anyhow!("git gc exited with status {status}"));
        }
    }

    Ok(())
}
