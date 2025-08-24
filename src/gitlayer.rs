use anyhow::{Context, Result, bail};
use console::Style;
use git2::{Commit, Oid, Repository, Signature, Tree};
use git2::{ErrorClass, ErrorCode};
use skim::Skim;
use skim::prelude::{SkimItemReader, SkimOptionsBuilder};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tracing::warn;

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

/// Initialize the `.autosnap` bare repository if absent and add it to `.git/info/exclude`.
pub fn init_autosnap(repo_root: &Path) -> Result<()> {
    let path = autosnap_dir(repo_root);
    let is_new = !path.exists();

    if is_new {
        let _ = Repository::init_bare(&path)
            .with_context(|| format!("failed to init bare repo at {}", path.display()))?;
    }

    // Add .autosnap to .git/info/exclude to prevent it from appearing in git status
    add_to_git_exclude(repo_root)?;

    Ok(())
}

/// Add `.autosnap` to `.git/info/exclude` if not already present.
fn add_to_git_exclude(repo_root: &Path) -> Result<()> {
    let git_dir = repo_root.join(".git");
    if !git_dir.exists() {
        // Not in a git repository, skip
        return Ok(());
    }

    let info_dir = git_dir.join("info");
    fs::create_dir_all(&info_dir).with_context(|| {
        format!(
            "failed to create .git/info directory at {}",
            info_dir.display()
        )
    })?;

    let exclude_path = info_dir.join("exclude");

    // Check if .autosnap is already in exclude file
    let pattern_exists = if exclude_path.exists() {
        let file = fs::File::open(&exclude_path)
            .with_context(|| format!("failed to open {}", exclude_path.display()))?;
        let reader = BufReader::new(file);
        reader
            .lines()
            .map_while(Result::ok)
            .any(|line| line.trim() == ".autosnap" || line.trim() == "/.autosnap")
    } else {
        false
    };

    // Add .autosnap if not already present
    if !pattern_exists {
        // Check if we need a leading newline
        let needs_newline = if exclude_path.exists() {
            let contents = fs::read_to_string(&exclude_path)?;
            !contents.is_empty() && !contents.ends_with('\n')
        } else {
            false
        };

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&exclude_path)
            .with_context(|| format!("failed to open {} for writing", exclude_path.display()))?;

        if needs_newline {
            writeln!(file)?;
        }

        writeln!(file, ".autosnap")?;
    }

    Ok(())
}

/// Take a single snapshot of the working tree and commit it into `.autosnap`.
pub fn snapshot_once(repo_root: &Path, message: Option<&str>) -> Result<()> {
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
        return Ok(());
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

    // Print short id for script-friendliness per implementation plan
    if let Ok(short) = repo.find_object(oid, None).and_then(|o| o.short_id())
        && let Some(s) = short.as_str()
    {
        println!("{}", s);
    }

    Ok(())
}

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
fn build_index(repo: &Repository) -> Result<()> {
    let work_tree = repo
        .workdir()
        .context("repository has no working directory")?;

    // Try optimized path first, fall back to standard approach
    if let Some(files) = discover_files(work_tree)? {
        update_index_from_file_list(repo, files)
    } else {
        update_index_standard(repo)
    }
}

// Discover files in the working tree using git ls-files for performance
// Returns None if git ls-files is not available
fn discover_files(work_tree: &Path) -> Result<Option<Vec<String>>> {
    let git_dir = work_tree.join(".git");
    if !git_dir.exists() {
        return Ok(None);
    }

    let output = Command::new("git")
        .current_dir(work_tree)
        .args([
            "ls-files",
            "-z",                 // Null-terminated
            "--cached",           // Tracked files
            "--others",           // Untracked files
            "--exclude-standard", // Respect .gitignore
        ])
        .output()
        .context("failed to run git ls-files")?;

    if !output.status.success() {
        return Ok(None);
    }

    let mut files = Vec::new();
    if !output.stdout.is_empty() {
        for file_bytes in output.stdout.split(|&b| b == 0).filter(|s| !s.is_empty()) {
            let file_path =
                std::str::from_utf8(file_bytes).context("invalid UTF-8 in file path")?;

            // Skip internal git directories
            if should_skip_path(file_path) {
                continue;
            }

            files.push(file_path.to_string());
        }
    }

    Ok(Some(files))
}

// Update index using a pre-discovered file list
fn update_index_from_file_list(repo: &Repository, files: Vec<String>) -> Result<()> {
    let mut index = repo.index().context("failed to get index")?;

    // Update tracked files first (uses stat cache)
    index
        .update_all(["."].iter(), None)
        .context("failed to update tracked files")?;

    // Remove stale entries
    remove_stale_entries(&mut index, &files)?;

    // Add new files
    for file_path in files {
        let _ = index.add_path(Path::new(&file_path));
    }

    index.write().context("failed to write index")?;
    Ok(())
}

// Remove index entries for files that no longer exist
fn remove_stale_entries(index: &mut git2::Index, current_files: &[String]) -> Result<()> {
    let files_set: std::collections::HashSet<&str> =
        current_files.iter().map(|s| s.as_str()).collect();

    let mut to_remove = Vec::new();
    for i in 0..index.len() {
        if let Some(entry) = index.get(i)
            && let Ok(path_str) = std::str::from_utf8(&entry.path)
            && !files_set.contains(path_str)
        {
            to_remove.push(path_str.to_string());
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

    // Update tracked files
    index
        .update_all(["."].iter(), None)
        .context("failed to update tracked files")?;

    // Add new files
    index
        .add_all(["."].iter(), git2::IndexAddOption::DEFAULT, None)
        .context("failed to add new files")?;

    // Clean up internal directories
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
fn write_tree_with_retries(
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

/// Garbage collect snapshots - compress objects and optionally prune old ones.
///
/// When `prune` is false, only compresses/packs objects without removing any snapshots.
/// When `prune` is true with `prune_days` set, removes snapshots older than the specified days.
///
/// Uses git command directly instead of libgit2 because libgit2 doesn't provide
/// APIs for reflog expiration or garbage collection. These are considered
/// "policy-based" housekeeping operations that libgit2 intentionally omits,
/// requiring applications to either implement custom logic using low-level
/// primitives or shell out to git (which is the standard practice).
pub fn gc(repo_root: &Path, prune: bool, prune_days: Option<u32>) -> Result<()> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        return Ok(()); // nothing to do
    }

    let gitdir = autosnap.to_string_lossy().to_string();

    if prune {
        // Pruning mode: expire reflog and prune old objects
        let days = prune_days.unwrap_or(60);
        let expire = format!("{}d", days);

        // First expire the reflog
        let status = Command::new("git")
            .args([
                format!("--git-dir={}", gitdir).as_str(),
                "reflog",
                "expire",
                format!("--expire={}", expire).as_str(),
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
                format!("--git-dir={}", gitdir).as_str(),
                "gc",
                format!("--prune={}", expire).as_str(),
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
            .args([format!("--git-dir={}", gitdir).as_str(), "gc"])
            .status()
            .context("failed to run git gc")?;
        if !status.success() {
            return Err(anyhow::anyhow!("git gc exited with status {status}"));
        }
    }

    Ok(())
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

/// Open a snapshot in a subshell for exploration.
///
/// # Arguments
/// * `repo_root` - The repository root directory
/// * `commit` - Optional commit SHA or reference to explore (defaults to HEAD)
/// * `interactive` - If true, opens skim UI for interactive commit selection
///
/// # Returns
/// * `Ok(())` if the subshell session completes successfully
/// * `Err` if snapshot extraction or subshell launch fails
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
    use git2::{ObjectType, TreeWalkMode, TreeWalkResult};

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
///
/// Opens a terminal UI allowing users to browse and select from available snapshots.
///
/// # Arguments
/// * `autosnap_dir` - Path to the .autosnap bare repository
///
/// # Returns
/// * `Ok(Some(sha))` if a commit was selected
/// * `Ok(None)` if user cancelled selection
/// * `Err` if no snapshots exist or skim fails
fn select_commit_interactive(autosnap_dir: &Path) -> Result<Option<String>> {
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

    // Create skim input from string
    let item_reader = SkimItemReader::default();
    let items = item_reader.of_bufread(Cursor::new(items_str));

    // Run skim
    let selected = Skim::run_with(&options, Some(items)).and_then(|out| {
        if out.is_abort {
            None
        } else {
            out.selected_items.first().map(|item| {
                let text = item.output();
                // Extract the commit SHA (first part before tab)
                text.split('\t').next().unwrap_or("").to_string()
            })
        }
    });

    if selected.is_none() {
        bail!("No snapshot selected");
    }

    Ok(selected)
}

/// List commits from the repository.
///
/// Collects recent commits with their short SHA and first line of message.
///
/// # Arguments
/// * `repo` - The repository to list commits from
/// * `limit` - Maximum number of commits to retrieve
///
/// # Returns
/// * `Ok(Vec<(sha, message)>)` - List of commit tuples with short SHA and first line
/// * `Err` if revwalk or commit retrieval fails
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

/// Restore files from a snapshot to the working tree.
///
/// # Arguments
/// * `repo_root` - Path to the main repository root
/// * `commit` - Optional commit SHA or ref to restore from (defaults to HEAD)
/// * `interactive` - Whether to interactively select a commit
/// * `force` - Force restore even with uncommitted changes
/// * `dry_run` - Preview changes without actually restoring
/// * `full` - Full restore (remove files not in snapshot)
/// * `paths` - Specific paths to restore (empty = all)
///
/// # Safety
/// By default, refuses to overwrite uncommitted changes unless `force` is true.
/// In `full` mode, removes files not present in the snapshot.
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
                // Check for any modifications, additions, deletions, or untracked files
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
        // In dry run mode, just simulate
        checkout_builder.dry_run();
    } else if force {
        // Force mode - overwrite everything
        checkout_builder.force();
    } else {
        // Safe mode - refuse to overwrite changes
        checkout_builder.safe();
    }

    // Configure what to update
    // Always recreate missing files and don't update the autosnap repo's index
    checkout_builder.recreate_missing(true);
    checkout_builder.update_index(false);

    // Note: We handle full restore file removal manually after checkout
    // to avoid accidentally removing .git and .autosnap directories

    // If specific paths are requested, configure them
    if !paths.is_empty() {
        for path in paths {
            checkout_builder.path(path);
        }
    }

    // Progress callback for user feedback
    checkout_builder.progress(|path, _cur, _total| {
        if let Some(p) = path {
            println!("  {}", p.display());
        }
    });

    // Perform the checkout
    repo.checkout_tree(tree.as_object(), Some(&mut checkout_builder))
        .context("failed to restore snapshot")?;

    // For full restore, manually remove files not in snapshot (excluding .git and .autosnap)
    if full && !dry_run {
        // Get list of files in working directory
        let entries = fs::read_dir(repo_root).context("failed to read repository directory")?;

        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Skip .git and .autosnap directories
            if file_name == ".git" || file_name == ".autosnap" {
                continue;
            }

            // Check if this path exists in the tree
            let relative_path = path.strip_prefix(repo_root).unwrap_or(&path);

            if tree.get_path(relative_path).is_err() {
                // Path doesn't exist in snapshot, remove it
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

    // Update the index to match the working tree (unless dry run)
    if !dry_run {
        // Open main repo to update its index
        let main_repo =
            Repository::discover(repo_root).context("failed to open main repository")?;

        let mut index = main_repo
            .index()
            .context("failed to open main repository index")?;

        // Update index from working tree
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

// Build a tree from the working directory for diff operations
fn build_working_tree_from_status<'a>(repo: &'a Repository, repo_root: &Path) -> Result<Tree<'a>> {
    // Set the workdir temporarily for the bare repo
    repo.set_workdir(repo_root, false)
        .context("failed to set workdir")?;

    // Build the index from working tree
    build_index(repo).context("failed to build index")?;

    // Get the index and write the tree
    let mut index = repo.index().context("failed to get index")?;
    let tree_oid = index.write_tree().context("failed to write tree")?;
    repo.find_tree(tree_oid).context("failed to find tree")
}

/// Output format for diff display
#[derive(Debug, Clone, Copy)]
pub enum DiffFormat {
    /// Full unified diff output
    Unified,
    /// Statistics only (files changed, insertions, deletions)
    Stat,
    /// Only show names of changed files
    NameOnly,
    /// Show names and status of changed files (Added, Modified, Deleted)
    NameStatus,
}

/// Show diff between snapshots or working tree
///
/// # Arguments
/// * `repo_root` - Path to the repository root
/// * `commit1` - First commit ref (None means working tree)
/// * `commit2` - Second commit ref (None means HEAD)
/// * `interactive` - Whether to use interactive commit selection
/// * `format` - Output format for the diff
/// * `paths` - Specific paths to diff (empty means all)
///
/// # Returns
/// * `Ok(())` on success
/// * `Err` on failure
pub fn diff(
    repo_root: &Path,
    commit1: Option<&str>,
    commit2: Option<&str>,
    interactive: bool,
    format: DiffFormat,
    paths: &[String],
) -> Result<()> {
    let autosnap_path = repo_root.join(".autosnap");
    let repo = Repository::open(&autosnap_path).context("failed to open .autosnap repository")?;

    // Set workdir for proper working tree access
    repo.set_workdir(repo_root, false)
        .context("failed to set working directory")?;

    // Handle interactive mode for commit selection
    let (actual_commit1, actual_commit2) = if interactive {
        println!("Select first commit to compare (or press ESC to use working tree):");
        let first = select_commit_interactive(&autosnap_path)?;

        println!("Select second commit to compare (or press ESC to use HEAD):");
        let second = select_commit_interactive(&autosnap_path)?;

        (first, second)
    } else {
        (commit1.map(String::from), commit2.map(String::from))
    };

    // Resolve commits - special handling for working tree comparisons
    let (tree1, tree2) = match (actual_commit1, actual_commit2) {
        // No arguments: compare working tree to HEAD
        (None, None) => {
            // Working tree
            let work_tree = build_working_tree_from_status(&repo, repo_root)?;

            // HEAD
            let head = repo
                .head()
                .context("no snapshots found in .autosnap repository")?;
            let head_commit = head.peel_to_commit().context("failed to get HEAD commit")?;
            let head_tree = head_commit.tree()?;

            (Some(work_tree), Some(head_tree))
        }
        // One argument: compare that commit to working tree
        (Some(ref commit_ref), None) => {
            let obj = repo
                .revparse_single(commit_ref)
                .with_context(|| format!("failed to find commit: {}", commit_ref))?;
            let commit = obj
                .peel_to_commit()
                .with_context(|| format!("failed to resolve commit: {}", commit_ref))?;
            let commit_tree = commit.tree()?;

            // Working tree
            let work_tree = build_working_tree_from_status(&repo, repo_root)?;

            (Some(commit_tree), Some(work_tree))
        }
        // Two arguments: compare two commits
        (Some(ref commit1_ref), Some(ref commit2_ref)) => {
            let obj1 = repo
                .revparse_single(commit1_ref)
                .with_context(|| format!("failed to find commit: {}", commit1_ref))?;
            let commit1 = obj1
                .peel_to_commit()
                .with_context(|| format!("failed to resolve commit: {}", commit1_ref))?;

            let obj2 = repo
                .revparse_single(commit2_ref)
                .with_context(|| format!("failed to find commit: {}", commit2_ref))?;
            let commit2 = obj2
                .peel_to_commit()
                .with_context(|| format!("failed to resolve commit: {}", commit2_ref))?;

            (Some(commit1.tree()?), Some(commit2.tree()?))
        }
        // None for first but Some for second doesn't make sense, treat as working tree vs commit2
        (None, Some(ref commit_ref)) => {
            // Working tree
            let work_tree = build_working_tree_from_status(&repo, repo_root)?;

            let obj = repo
                .revparse_single(commit_ref)
                .with_context(|| format!("failed to find commit: {}", commit_ref))?;
            let commit = obj
                .peel_to_commit()
                .with_context(|| format!("failed to resolve commit: {}", commit_ref))?;

            (Some(work_tree), Some(commit.tree()?))
        }
    };

    // Create diff options
    let mut diff_opts = git2::DiffOptions::new();

    // Add path filters if specified
    for path in paths {
        diff_opts.pathspec(path);
    }

    // Perform the diff
    let diff = match (tree1.as_ref(), tree2.as_ref()) {
        (Some(t1), Some(t2)) => repo.diff_tree_to_tree(Some(t1), Some(t2), Some(&mut diff_opts))?,
        _ => {
            bail!("failed to create diff between specified commits");
        }
    };

    // Format and display the diff based on requested format
    match format {
        DiffFormat::Unified => {
            print_unified_diff(&diff)?;
        }
        DiffFormat::Stat => {
            print_diff_stats(&diff)?;
        }
        DiffFormat::NameOnly => {
            print_diff_name_only(&diff)?;
        }
        DiffFormat::NameStatus => {
            print_diff_name_status(&diff)?;
        }
    }

    Ok(())
}

/// Print unified diff output using the similar crate for better formatting
fn print_unified_diff(diff: &git2::Diff) -> Result<()> {
    // Set up styles for different diff elements
    let added_style = Style::new().green();
    let removed_style = Style::new().red();
    let context_style = Style::new().dim();
    let header_style = Style::new().cyan().bold();

    // For each file in the diff
    diff.foreach(
        &mut |delta, _progress| {
            // Print file header
            let old_path = delta
                .old_file()
                .path()
                .and_then(|p| p.to_str())
                .unwrap_or("unknown");
            let new_path = delta
                .new_file()
                .path()
                .and_then(|p| p.to_str())
                .unwrap_or("unknown");

            println!("{}", header_style.apply_to(format!("--- {}", old_path)));
            println!("{}", header_style.apply_to(format!("+++ {}", new_path)));
            true
        },
        None,
        Some(&mut |_delta, _hunk| {
            // Hunk callback - we'll handle in line callback
            true
        }),
        Some(&mut |_delta, hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("");

            // Handle hunk headers specially
            if let Some(hunk) = hunk {
                let header = format!(
                    "@@ -{},{} +{},{} @@",
                    hunk.old_start(),
                    hunk.old_lines(),
                    hunk.new_start(),
                    hunk.new_lines()
                );
                if line.origin() == '@' {
                    println!("{}", header_style.apply_to(header));
                    return true;
                }
            }

            // Apply appropriate styling based on line type
            match line.origin() {
                '+' => print!("{}", added_style.apply_to(format!("+{}", content))),
                '-' => print!("{}", removed_style.apply_to(format!("-{}", content))),
                ' ' => print!("{}", context_style.apply_to(format!(" {}", content))),
                _ => print!("{}", content),
            }
            true
        }),
    )?;

    Ok(())
}

/// Print diff statistics
fn print_diff_stats(diff: &git2::Diff) -> Result<()> {
    let stats = diff.stats()?;

    println!(
        " {} files changed, {} insertions(+), {} deletions(-)",
        stats.files_changed(),
        stats.insertions(),
        stats.deletions()
    );

    // Print per-file stats
    diff.foreach(
        &mut |delta, _progress| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(|p| p.to_str())
                .unwrap_or("unknown");

            print!(" {}", path);
            true
        },
        None,
        None,
        None,
    )?;

    Ok(())
}

/// Print only file names that changed
fn print_diff_name_only(diff: &git2::Diff) -> Result<()> {
    diff.foreach(
        &mut |delta, _progress| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(|p| p.to_str())
                .unwrap_or("unknown");

            println!("{}", path);
            true
        },
        None,
        None,
        None,
    )?;
    Ok(())
}

/// Print file names with their status
fn print_diff_name_status(diff: &git2::Diff) -> Result<()> {
    diff.foreach(
        &mut |delta, _progress| {
            let path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .and_then(|p| p.to_str())
                .unwrap_or("unknown");

            let status = match delta.status() {
                git2::Delta::Added => "A",
                git2::Delta::Deleted => "D",
                git2::Delta::Modified => "M",
                git2::Delta::Renamed => "R",
                git2::Delta::Copied => "C",
                git2::Delta::Ignored => "I",
                git2::Delta::Untracked => "?",
                git2::Delta::Typechange => "T",
                git2::Delta::Unmodified => "U",
                git2::Delta::Conflicted => "C",
                _ => "?",
            };

            println!("{}\t{}", status, path);
            true
        },
        None,
        None,
        None,
    )?;
    Ok(())
}
