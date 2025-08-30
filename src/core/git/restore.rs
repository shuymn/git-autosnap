use anyhow::{Context, Result, bail};
use git2::Repository;
use std::fs;
use std::path::Path;

use super::repo::autosnap_dir;
use super::shell::select_commit_interactive;

/// Restore files from a snapshot to the working tree.
pub fn restore(
    repo_root: &Path,
    commit: Option<&str>,
    interactive: bool,
    force: bool,
    dry_run: bool,
    full: bool,
    paths: &[String],
) -> Result<()> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        bail!(".autosnap is missing; run `git autosnap init` first")
    }

    // Check for uncommitted changes unless forced
    if !force && !dry_run {
        // Open the main repository to check for changes
        let main_repo =
            Repository::discover(repo_root).context("failed to open main repository")?;

        let statuses = main_repo
            .statuses(None)
            .context("failed to get repository status")?;

        if !statuses.is_empty() {
            let mut has_changes = false;
            for status in statuses.iter() {
                let flags = status.status();
                if flags.contains(git2::Status::WT_MODIFIED)
                    || flags.contains(git2::Status::WT_NEW)
                    || flags.contains(git2::Status::WT_DELETED)
                    || flags.contains(git2::Status::INDEX_MODIFIED)
                    || flags.contains(git2::Status::INDEX_NEW)
                    || flags.contains(git2::Status::INDEX_DELETED)
                {
                    has_changes = true;
                    break;
                }
            }

            if has_changes {
                bail!(
                    "Working tree has uncommitted changes. Use --force to override or commit/stash your changes first."
                );
            }
        }
    }

    // If interactive mode, select commit using skim
    let commit_to_use = if interactive {
        select_commit_interactive(&autosnap)?
    } else {
        commit.map(String::from)
    };

    // Open the autosnap bare repository
    let repo = Repository::open(&autosnap)
        .with_context(|| format!("failed to open autosnap repo at {}", autosnap.display()))?;

    // Set the working directory to the main repo root
    repo.set_workdir(repo_root, false)
        .with_context(|| format!("failed to set workdir to {}", repo_root.display()))?;

    // Parse the commit reference
    let commit_ref = commit_to_use.as_deref().unwrap_or("HEAD");
    let object = repo
        .revparse_single(commit_ref)
        .with_context(|| format!("failed to parse commit reference: {}", commit_ref))?;

    let commit = object
        .peel_to_commit()
        .with_context(|| format!("failed to resolve {} to a commit", commit_ref))?;

    let tree = commit.tree().context("failed to get tree from commit")?;

    // Get commit info for display
    let short_id = commit
        .as_object()
        .short_id()
        .context("failed to get short commit id")?;
    let short_id_str = short_id.as_str().unwrap_or("unknown");
    let message = commit.message().unwrap_or("no message");
    let first_line = message.lines().next().unwrap_or(message);

    if dry_run {
        println!("DRY RUN: Would restore from snapshot:");
    } else {
        println!("Restoring from snapshot:");
    }
    println!("  Commit: {} {}", short_id_str, first_line);
    if !paths.is_empty() {
        println!("  Paths: {}", paths.join(", "));
    }
    if full {
        println!("  Mode: Full restore (will remove files not in snapshot)");
    } else {
        println!("  Mode: Overlay (preserves files not in snapshot)");
    }
    println!();

    if !dry_run {
        println!("Processing files...");
    }

    // Build checkout options
    let mut checkout_builder = git2::build::CheckoutBuilder::new();

    if dry_run {
        checkout_builder.dry_run();
    } else if force {
        checkout_builder.force();
    } else {
        checkout_builder.safe();
    }

    checkout_builder.recreate_missing(true);
    checkout_builder.update_index(false);

    if !paths.is_empty() {
        for path in paths {
            checkout_builder.path(path);
        }
    }

    checkout_builder.progress(|path, _cur, _total| {
        if let Some(p) = path {
            println!("  {}", p.display());
        }
    });

    repo.checkout_tree(tree.as_object(), Some(&mut checkout_builder))
        .context("failed to restore snapshot")?;

    if full && !dry_run {
        let entries = fs::read_dir(repo_root).context("failed to read repository directory")?;
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if file_name == ".git" || file_name == ".autosnap" {
                continue;
            }
            let relative_path = path.strip_prefix(repo_root).unwrap_or(&path);
            if tree.get_path(relative_path).is_err() {
                if path.is_dir() {
                    fs::remove_dir_all(&path).with_context(|| {
                        format!("failed to remove directory: {}", path.display())
                    })?;
                    println!("  Removed: {}", relative_path.display());
                } else {
                    fs::remove_file(&path)
                        .with_context(|| format!("failed to remove file: {}", path.display()))?;
                    println!("  Removed: {}", relative_path.display());
                }
            }
        }
    }

    if !dry_run {
        let main_repo =
            Repository::discover(repo_root).context("failed to open main repository")?;
        let mut index = main_repo
            .index()
            .context("failed to open main repository index")?;
        index
            .update_all(["*"], None)
            .context("failed to update main repository index")?;
        index
            .write()
            .context("failed to write main repository index")?;

        println!("\n✓ Restore completed successfully");
        if full {
            println!("Note: Files not in the snapshot have been removed.");
        }
        println!("Note: Main repository index has been updated to match the restored state.");
    } else {
        println!("\nDRY RUN completed. No files were modified.");
    }

    Ok(())
}
