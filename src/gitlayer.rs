use anyhow::{Context, Result, bail};
use git2::{Commit, IndexAddOption, Repository, Signature, Tree};
use skim::Skim;
use skim::prelude::{SkimItemReader, SkimOptionsBuilder};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Cursor, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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
    let mut index = repo
        .index()
        .context("failed to open index for autosnap repo")?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("index add_all failed")?;
    // Best-effort remove .autosnap and .git if they got picked up
    let _ = index.remove_all([".autosnap", ".git"], None);
    index.write().context("failed to write index")?;
    let tree_id = index
        .write_tree()
        .context("failed to write tree from index")?;
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
    let msg = format!("AUTOSNAP[{branch}] {ts}");

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

/// Garbage collect (prune) snapshots older than the given number of days.
///
/// Uses git command directly instead of libgit2 because libgit2 doesn't provide
/// APIs for reflog expiration or garbage collection. These are considered
/// "policy-based" housekeeping operations that libgit2 intentionally omits,
/// requiring applications to either implement custom logic using low-level
/// primitives or shell out to git (which is the standard practice).
pub fn gc(repo_root: &Path, prune_days: u32) -> Result<()> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        return Ok(()); // nothing to do
    }

    let gitdir = autosnap.to_string_lossy().to_string();
    // Use Git's duration syntax like "60d" for 60 days
    let expire = format!("{}d", prune_days);

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

    let status = Command::new("git")
        .args([
            format!("--git-dir={}", gitdir).as_str(),
            "gc",
            format!("--prune={}", expire).as_str(),
        ])
        .status()
        .context("failed to run git gc")?;
    if !status.success() {
        return Err(anyhow::anyhow!("git gc exited with status {status}"));
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
