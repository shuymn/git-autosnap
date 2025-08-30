use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, bail};
use git2::{ObjectType, Repository, Tree, TreeWalkMode, TreeWalkResult};
use skim::{
    Skim,
    prelude::{SkimItemReader, SkimOptionsBuilder},
};

use super::repo::autosnap_dir;

/// Open a snapshot in a subshell for exploration.
pub fn snapshot_shell(repo_root: &Path, commit: Option<&str>, interactive: bool) -> Result<()> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        bail!(".autosnap is missing; run `git autosnap init` first")
    }

    // If interactive mode, select commit using skim
    let commit_to_use = if interactive {
        select_commit_interactive(&autosnap)?
    } else {
        commit.map(String::from)
    };

    // Create a temporary directory
    let temp_dir = tempfile::TempDir::new().context("failed to create temporary directory")?;
    let temp_path = temp_dir.path();

    // Open the autosnap bare repository
    let repo = Repository::open(&autosnap)
        .with_context(|| format!("failed to open autosnap repo at {}", autosnap.display()))?;

    // Parse the commit reference
    let commit_ref = commit_to_use.as_deref().unwrap_or("HEAD");
    let object = repo
        .revparse_single(commit_ref)
        .with_context(|| format!("failed to parse commit reference: {}", commit_ref))?;

    let commit = object
        .peel_to_commit()
        .with_context(|| format!("failed to resolve {} to a commit", commit_ref))?;

    let tree = commit.tree().context("failed to get tree from commit")?;

    // Extract files from the tree to the temporary directory
    extract_tree_to_path(&repo, &tree, temp_path)?;

    // Format commit info for display
    let short_id = commit
        .as_object()
        .short_id()
        .context("failed to get short commit id")?;
    let short_id_str = short_id.as_str().unwrap_or("unknown");
    let message = commit.message().unwrap_or("no message");
    let first_line = message.lines().next().unwrap_or(message);

    println!("Opening snapshot in subshell:");
    println!("  Commit: {} {}", short_id_str, first_line);
    println!("  Location: {}", temp_path.display());
    println!("  Type 'exit' to return and cleanup");
    println!();

    // Determine which shell to use
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());

    // Launch subshell with modified prompt
    let ps1 = format!("[autosnap:{}] $ ", short_id_str);

    let status = Command::new(&shell)
        .current_dir(temp_path)
        .env("PS1", ps1)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("failed to launch subshell: {}", shell))?;

    // The temp_dir will be cleaned up automatically when it goes out of scope
    if status.success() {
        println!("\nSnapshot exploration completed, temporary directory cleaned up.");
    } else {
        println!("\nSubshell exited with status: {}", status);
    }

    Ok(())
}

/// Helper function to extract a git tree to a filesystem path
fn extract_tree_to_path(repo: &Repository, tree: &Tree, base_path: &Path) -> Result<()> {
    tree.walk(TreeWalkMode::PreOrder, |root, entry| {
        let entry_path = if root.is_empty() {
            PathBuf::from(entry.name().unwrap_or(""))
        } else {
            PathBuf::from(root).join(entry.name().unwrap_or(""))
        };

        let full_path = base_path.join(&entry_path);

        match entry.kind() {
            Some(ObjectType::Tree) => {
                // Create directory
                if let Err(e) = fs::create_dir_all(&full_path) {
                    eprintln!("Failed to create directory {}: {}", full_path.display(), e);
                }
            }
            Some(ObjectType::Blob) => {
                // Extract file
                if let Ok(object) = entry.to_object(repo)
                    && let Some(blob) = object.as_blob()
                {
                    // Ensure parent directory exists
                    if let Some(parent) = full_path.parent() {
                        let _ = fs::create_dir_all(parent);
                    }

                    if let Err(e) = fs::write(&full_path, blob.content()) {
                        eprintln!("Failed to write file {}: {}", full_path.display(), e);
                    }

                    // Try to preserve executable permissions
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let filemode = entry.filemode();
                        // Git stores executable as 0100755 (33261 in decimal)
                        if filemode == 33261 {
                            let permissions = fs::Permissions::from_mode(0o755);
                            let _ = fs::set_permissions(&full_path, permissions);
                        }
                    }
                }
            }
            _ => {}
        }

        TreeWalkResult::Ok
    })?;

    Ok(())
}

/// Interactive commit selection using skim fuzzy finder.
pub(crate) fn select_commit_interactive(autosnap_dir: &Path) -> Result<Option<String>> {
    // Open the autosnap repository
    let repo = Repository::open(autosnap_dir)
        .with_context(|| format!("failed to open autosnap repo at {}", autosnap_dir.display()))?;

    // Collect commits
    let commits = list_commits(&repo, 100)?;

    if commits.is_empty() {
        bail!("No snapshots found in .autosnap repository");
    }

    // Prepare items for skim
    let items: Vec<String> = commits
        .iter()
        .map(|c| format!("{}\t{}", c.0, c.1))
        .collect();

    let items_str = items.join("\n");

    // Configure skim options
    let options = SkimOptionsBuilder::default()
        .height("50%".to_string())
        .multi(false)
        .preview(Some("".to_string()))
        .preview_window("down:3:wrap".to_string())
        .prompt("Select snapshot> ".to_string())
        .build()
        .context("failed to build skim options")?;

    // Create item reader
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(std::io::Cursor::new(items_str));

    // Run skim
    let skim_output = Skim::run_with(&options, Some(items)).context("skim UI failed")?;

    if skim_output.is_abort {
        return Ok(None);
    }

    if let Some(item) = skim_output.selected_items.first() {
        let output = item.output();
        if let Some((sha, _)) = output.split_once('\t') {
            return Ok(Some(sha.to_string()));
        }
    }

    Ok(None)
}

/// List commits from the repository.
fn list_commits(repo: &Repository, limit: usize) -> Result<Vec<(String, String)>> {
    let mut commits = Vec::new();
    let mut revwalk = repo.revwalk()?;

    // Start from HEAD
    revwalk.push_head()?;

    // Collect commits with their short SHA and message
    for (i, oid) in revwalk.enumerate() {
        if i >= limit {
            break;
        }

        let oid = oid?;
        let commit = repo.find_commit(oid)?;

        let short_id = repo.find_object(oid, None)?.short_id()?;
        let short_id_str = short_id.as_str().unwrap_or("unknown").to_string();

        let message = commit.message().unwrap_or("no message");
        let first_line = message.lines().next().unwrap_or(message).to_string();

        commits.push((short_id_str, first_line));
    }

    Ok(commits)
}
