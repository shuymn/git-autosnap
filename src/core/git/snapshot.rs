use anyhow::{Context, Result, bail};
use git2::{Commit, Repository, Signature, Tree};
use std::path::Path;

use super::index::write_tree_with_retries;
use super::repo::autosnap_dir;

/// Take a single snapshot of the working tree and commit it into `.autosnap`.
/// Returns the short hash of the created commit, or None if no changes were made.
pub fn snapshot_once(repo_root: &Path, message: Option<&str>) -> Result<Option<String>> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        bail!(".autosnap is missing; run `git autosnap init` first")
    }

    // Open autosnap bare repo and attach the main working directory
    let repo = Repository::open(&autosnap)
        .with_context(|| format!("failed to open autosnap repo at {}", autosnap.display()))?;
    // Associate with the main working directory so libgit2 can read files and ignore rules
    repo.set_workdir(repo_root, false)
        .with_context(|| format!("failed to set workdir to {}", repo_root.display()))?;

    // Build index from the working directory, respecting .gitignore (libgit2)
    // with retries to tolerate transient file modifications during read.
    let tree_id = write_tree_with_retries(&repo, 5, 50)
        .context("failed to write tree from index (after retries)")?;
    let tree = repo
        .find_tree(tree_id)
        .context("failed to find written tree")?;

    // Check if identical to HEAD to avoid duplicate commits
    if let Some(prev_tree) = head_tree(&repo)?
        && prev_tree.id() == tree.id()
    {
        // No changes; do not create a new commit
        return Ok(None);
    }

    // Create author/committer signature from main repo config
    let sig = signature_from_main(repo_root)?;

    // Commit message
    let branch = current_branch_name(repo_root).unwrap_or_else(|| "DETACHED".to_string());
    let ts = iso8601_now_with_offset();
    let msg = if let Some(custom_msg) = message {
        format!("AUTOSNAP[{branch}] {ts}: {custom_msg}")
    } else {
        format!("AUTOSNAP[{branch}] {ts}")
    };

    // Determine parents (if any)
    let parents: Vec<Commit> = match repo.head() {
        Ok(head) => {
            if let Some(oid) = head.target() {
                vec![
                    repo.find_commit(oid)
                        .context("failed to peel HEAD to commit")?,
                ]
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    let parent_refs: Vec<&Commit> = parents.iter().collect();

    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parent_refs)
        .context("failed to create autosnap commit")?;

    // Return short id for script-friendliness per implementation plan
    if let Ok(short) = repo.find_object(oid, None).and_then(|o| o.short_id())
        && let Some(s) = short.as_str()
    {
        Ok(Some(s.to_string()))
    } else {
        // Fallback to full oid if short id fails
        Ok(Some(oid.to_string()))
    }
}

fn head_tree(repo: &Repository) -> Result<Option<Tree<'_>>> {
    match repo.head() {
        Ok(head) => {
            if let Some(oid) = head.target() {
                let commit = repo.find_commit(oid)?;
                Ok(Some(commit.tree()?))
            } else {
                Ok(None)
            }
        }
        Err(_) => Ok(None),
    }
}

fn signature_from_main(repo_root: &Path) -> Result<Signature<'static>> {
    let main_repo = Repository::discover(repo_root)?;
    let cfg = main_repo.config()?;
    let name = cfg
        .get_string("user.name")
        .unwrap_or_else(|_| "git-autosnap".to_string());
    let email = cfg
        .get_string("user.email")
        .unwrap_or_else(|_| "git-autosnap@local".to_string());
    let sig = Signature::now(&name, &email).context("failed to create signature")?;
    Ok(sig)
}

fn current_branch_name(repo_root: &Path) -> Option<String> {
    let main_repo = Repository::discover(repo_root).ok()?;
    let head = main_repo.head().ok()?;
    if head.is_branch() {
        head.shorthand().map(|s| s.to_string())
    } else {
        None
    }
}

fn iso8601_now_with_offset() -> String {
    use time::{OffsetDateTime, format_description::well_known::Rfc3339};
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    now.format(&Rfc3339).unwrap_or_else(|_| now.to_string())
}
