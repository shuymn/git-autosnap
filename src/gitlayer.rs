use std::path::{Path, PathBuf};
use anyhow::{bail, Context, Result};
use git2::{Commit, IndexAddOption, Oid, Repository, Signature, Tree};

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
pub fn snapshot_once(repo_root: &Path) -> Result<()> {
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
    let mut index = repo.index().context("failed to open index for autosnap repo")?;
    index.add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("index add_all failed")?;
    // Best-effort remove .autosnap and .git if they got picked up
    let _ = index.remove_all([".autosnap", ".git"], None);
    index.write().context("failed to write index")?;
    let tree_id = index.write_tree().context("failed to write tree from index")?;
    let tree = repo.find_tree(tree_id).context("failed to find written tree")?;

    // Check if identical to HEAD to avoid duplicate commits
    if let Some(prev_tree) = head_tree(&repo)? {
        if prev_tree.id() == tree.id() {
            // No changes; do not create a new commit
            return Ok(());
        }
    }

    // Create author/committer signature from main repo config
    let sig = signature_from_main(repo_root)?;

    // Commit message
    let branch = current_branch_name(repo_root).unwrap_or_else(|| "DETACHED".to_string());
    let ts = unix_timestamp_iso8601_like();
    let msg = format!("AUTOSNAP[{branch}] {ts}");

    // Determine parents (if any)
    let parents: Vec<Commit> = match repo.head() {
        Ok(head) => {
            if let Some(oid) = head.target() {
                vec![repo.find_commit(oid).context("failed to peel HEAD to commit")?]
            } else {
                Vec::new()
            }
        }
        Err(_) => Vec::new(),
    };

    let parent_refs: Vec<&Commit> = parents.iter().collect();

    let _commit = repo
        .commit(Some("HEAD"), &sig, &sig, &msg, &tree, &parent_refs)
        .context("failed to create autosnap commit")?;

    Ok(())
}

/// Garbage collect (prune) snapshots older than the given number of days.
/// Not implemented yet.
pub fn gc(_repo_root: &Path, _prune_days: u32) -> Result<()> {
    // Placeholder: Will be implemented using libgit2 or delegated to `git` CLI.
    Ok(())
}

fn head_tree(repo: &Repository) -> Result<Option<Tree>> {
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
    let name = cfg.get_string("user.name").unwrap_or_else(|_| "git-autosnap".to_string());
    let email = cfg.get_string("user.email").unwrap_or_else(|_| "git-autosnap@local".to_string());
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

fn unix_timestamp_iso8601_like() -> String {
    // Without external time crates, use a simple UTC-like format "YYYY-MM-DDTHH:MM:SSZ" if possible,
    // otherwise fall back to epoch seconds. We'll approximate using libc gmtime if available in std.
    // For portability within current dependencies, emit epoch seconds.
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(dur) => format!("{}", dur.as_secs()),
        Err(_) => "0".to_string(),
    }
}
