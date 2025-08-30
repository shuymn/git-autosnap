use anyhow::{Context, Result, bail};
use console::Style;
use git2::{Repository, Tree};

use super::{index::build_index, repo::autosnap_dir, shell::select_commit_interactive};

#[derive(Clone, Copy, Debug)]
pub enum DiffFormat {
    Unified,
    Stat,
    NameOnly,
    NameStatus,
}

pub fn diff(
    repo_root: &std::path::Path,
    commit1: Option<&str>,
    commit2: Option<&str>,
    interactive: bool,
    format: DiffFormat,
    paths: &[String],
) -> Result<()> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        bail!(".autosnap is missing; run `git autosnap init` first")
    }

    // Open the autosnap bare repository
    let repo = Repository::open(&autosnap)
        .with_context(|| format!("failed to open autosnap repo at {}", autosnap.display()))?;

    // Interactive selection of commit(s)
    let (sel1, sel2) = if interactive {
        (
            select_commit_interactive(&autosnap)?,
            select_commit_interactive(&autosnap)?,
        )
    } else {
        (None, None)
    };
    let commit1 = commit1.or(sel1.as_deref());
    let commit2 = commit2.or(sel2.as_deref());

    // Resolve trees for diffing
    let (tree1, tree2) = match (commit1, commit2) {
        (None, None) => {
            // Diff working tree vs HEAD
            let head_obj = repo
                .revparse_single("HEAD")
                .context("failed to find HEAD")?;
            let head_commit = head_obj.peel_to_commit().context("failed to peel HEAD")?;
            let commit_tree = head_commit.tree()?;
            let work_tree = build_working_tree_from_status(&repo, repo_root)?;
            (Some(work_tree), Some(commit_tree))
        }
        (Some(ref commit_ref), None) => {
            let obj = repo
                .revparse_single(commit_ref)
                .with_context(|| format!("failed to find commit: {}", commit_ref))?;
            let commit = obj
                .peel_to_commit()
                .with_context(|| format!("failed to resolve commit: {}", commit_ref))?;
            let commit_tree = commit.tree()?;
            let work_tree = build_working_tree_from_status(&repo, repo_root)?;
            (Some(commit_tree), Some(work_tree))
        }
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
        (None, Some(ref commit_ref)) => {
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

    let mut diff_opts = git2::DiffOptions::new();
    for path in paths {
        diff_opts.pathspec(path);
    }

    let diff = match (tree1.as_ref(), tree2.as_ref()) {
        (Some(t1), Some(t2)) => repo.diff_tree_to_tree(Some(t1), Some(t2), Some(&mut diff_opts))?,
        _ => bail!("failed to create diff between specified commits"),
    };

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

// Build a tree from the working directory for diff operations
fn build_working_tree_from_status<'a>(
    repo: &'a Repository,
    repo_root: &std::path::Path,
) -> Result<Tree<'a>> {
    repo.set_workdir(repo_root, false)
        .context("failed to set workdir")?;
    build_index(repo).context("failed to build index")?;
    let mut index = repo.index().context("failed to get index")?;
    let tree_id = index.write_tree()?;
    repo.find_tree(tree_id)
        .context("failed to find written tree")
}

/// Print unified diff output using styles
fn print_unified_diff(diff: &git2::Diff) -> Result<()> {
    let added_style = Style::new().green();
    let removed_style = Style::new().red();
    let context_style = Style::new().dim();
    let header_style = Style::new().cyan().bold();

    diff.foreach(
        &mut |delta, _| {
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
        Some(&mut |_delta, _hunk| true),
        Some(&mut |_delta, hunk, line| {
            let content = std::str::from_utf8(line.content()).unwrap_or("");
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

fn print_diff_stats(diff: &git2::Diff) -> Result<()> {
    let stats = diff.stats()?;
    println!(
        " {} files changed, {} insertions(+), {} deletions(-)",
        stats.files_changed(),
        stats.insertions(),
        stats.deletions()
    );
    diff.foreach(
        &mut |delta, _| {
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

fn print_diff_name_only(diff: &git2::Diff) -> Result<()> {
    diff.foreach(
        &mut |delta, _| {
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

fn print_diff_name_status(diff: &git2::Diff) -> Result<()> {
    diff.foreach(
        &mut |delta, _| {
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
