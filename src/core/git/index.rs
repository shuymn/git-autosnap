use std::path::Path;

use anyhow::{Context, Result};
use git2::{ErrorClass, ErrorCode, Oid, Repository};
use tracing::warn;

// Determine if a git2 error is likely a transient filesystem race
// where a file changed between stat and read during index population.
fn is_transient_fs_change(err: &git2::Error) -> bool {
    matches!(err.class(), ErrorClass::Filesystem)
        || matches!(err.code(), ErrorCode::Modified)
        || err
            .message()
            .to_lowercase()
            .contains("file changed before we could read it")
}

// Build the repository index from the working tree
pub(crate) fn build_index(repo: &Repository) -> Result<()> {
    let work_tree = repo
        .workdir()
        .context("repository has no working directory")?;

    // Try optimized path first, fall back to standard approach
    if let Some(discovered) = discover_files(repo, work_tree)? {
        update_index_from_discovery(repo, discovered)
    } else {
        update_index_standard(repo)
    }
}

// Structure to hold discovered files and optimization hints
struct DiscoveredFiles {
    files: Vec<String>,
    // Track which paths are in the index for optimized stale detection
    indexed_paths: std::collections::HashSet<String>,
}

// Discover files in the working tree (equivalent to: git ls-files -z --cached --others --exclude-standard)
fn discover_files(repo: &Repository, work_tree: &Path) -> Result<Option<DiscoveredFiles>> {
    // Verify that the repository exists
    if !work_tree.join(".git").exists() {
        return Ok(None);
    }

    // This function requires a working tree to discover files
    // - For the main repo: always has a working tree
    // - For .autosnap repo: temporarily gets a workdir set during diff/restore operations
    if repo.is_bare() {
        return Ok(None);
    }

    // Use BTreeSet to maintain lexicographic order and avoid duplicates
    let mut all_paths = std::collections::BTreeSet::new();
    let mut indexed_paths = std::collections::HashSet::new();

    // --cached: list all paths that are in the index
    let index = repo.index().context("failed to get index")?;
    for i in 0..index.len() {
        if let Some(entry) = index.get(i) {
            // Convert path bytes to string
            let path_str =
                std::str::from_utf8(&entry.path).context("invalid UTF-8 in index entry path")?;

            // Skip internal git directories
            if !should_skip_path(path_str) {
                all_paths.insert(path_str.to_string());
                indexed_paths.insert(path_str.to_string());
            }
        }
    }

    // --others --exclude-standard: list untracked files respecting .gitignore
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(true)
        .no_refresh(true);

    let statuses = repo
        .statuses(Some(&mut opts))
        .context("failed to get repository status")?;

    for status_entry in statuses.iter() {
        if status_entry.status().contains(git2::Status::WT_NEW) {
            let path_str = if let Some(path) = status_entry.path() {
                path
            } else {
                let path_bytes = status_entry.path_bytes();
                std::str::from_utf8(path_bytes).context("invalid UTF-8 in status entry path")?
            };

            if !should_skip_path(path_str) {
                all_paths.insert(path_str.to_string());
            }
        }
    }

    Ok(Some(DiscoveredFiles {
        files: all_paths.into_iter().collect(),
        indexed_paths,
    }))
}

// Update index using pre-discovered file list
fn update_index_from_discovery(repo: &Repository, discovered: DiscoveredFiles) -> Result<()> {
    let mut index = repo.index().context("failed to get index")?;

    // Update tracked files first (uses stat cache)
    index
        .update_all(["."].iter(), None)
        .context("failed to update tracked files")?;

    // Remove stale entries - only check if we have indexed files
    if !discovered.indexed_paths.is_empty() {
        remove_stale_entries(&mut index, &discovered.files, &discovered.indexed_paths)?;
    }

    // Add new files
    for file_path in discovered.files {
        let _ = index.add_path(Path::new(&file_path));
    }

    index.write().context("failed to write index")?;
    Ok(())
}

// Remove stale entries using pre-collected index information
fn remove_stale_entries(
    index: &mut git2::Index,
    current_files: &[String],
    indexed_paths: &std::collections::HashSet<String>,
) -> Result<()> {
    let current_set: std::collections::HashSet<&str> =
        current_files.iter().map(|s| s.as_str()).collect();

    // Only check the paths we know are in the index
    let mut to_remove = Vec::new();
    for indexed_path in indexed_paths {
        if !current_set.contains(indexed_path.as_str()) {
            to_remove.push(indexed_path.clone());
        }
    }

    for path in to_remove {
        index.remove_path(Path::new(&path))?;
    }

    Ok(())
}

// Standard index update using libgit2
fn update_index_standard(repo: &Repository) -> Result<()> {
    let mut index = repo.index()?;

    index
        .update_all(["."].iter(), None)
        .context("failed to update tracked files")?;

    index
        .add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)
        .context("failed to add new files")?;

    let _ = index.remove_all([".autosnap", ".git"], None);

    index.write().context("failed to write index")?;
    Ok(())
}

// Check if a path should be excluded from indexing
fn should_skip_path(path: &str) -> bool {
    path.starts_with(".git/")
        || path.starts_with(".autosnap/")
        || path == ".git"
        || path == ".autosnap"
}

// Try to build the index and write out the tree once.
// Returns the new tree id or the underlying git2 error.
fn try_write_tree(repo: &Repository) -> std::result::Result<Oid, git2::Error> {
    // Build the index from working tree
    if let Err(e) = build_index(repo) {
        return Err(git2::Error::from_str(&e.to_string()));
    }

    // Get the index and write the tree
    let mut index = repo.index()?;
    index.write_tree()
}

// Retry wrapper with exponential backoff for transient FS-change errors.
pub(crate) fn write_tree_with_retries(
    repo: &Repository,
    max_attempts: u32,
    initial_backoff_ms: u64,
) -> Result<Oid> {
    let mut backoff_ms = initial_backoff_ms;
    let mut attempt = 1u32;
    loop {
        match try_write_tree(repo) {
            Ok(oid) => return Ok(oid),
            Err(e) if is_transient_fs_change(&e) && attempt < max_attempts => {
                warn!(
                    attempt,
                    backoff_ms,
                    "transient index build error, retrying after backoff: {}",
                    e.message()
                );
            }
            Err(e) => return Err(anyhow::Error::from(e).context("index build failed")),
        }

        std::thread::sleep(std::time::Duration::from_millis(backoff_ms));
        attempt += 1;
        backoff_ms = (backoff_ms * 2).min(800);
    }
}
