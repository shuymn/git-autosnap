use std::{
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
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
