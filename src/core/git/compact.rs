use std::{path::Path, process::Command};

use anyhow::{Context, Result};
use git2::{Oid, Repository, Signature, Time};

use super::{ops_lock::acquire_ops_lock, repo::autosnap_dir};

const BASELINE_MESSAGE: &str = "AUTOSNAP_COMPACT_BASELINE";

/// Result summary of a compact operation.
#[derive(Debug, Clone, Copy)]
pub struct CompactResult {
    pub before_commits: usize,
    pub after_commits: usize,
    pub rewritten: bool,
    pub baseline_created: bool,
}

#[derive(Debug, Clone)]
struct CommitReplayData {
    tree_id: Oid,
    message: String,
    author: SignatureData,
    committer: SignatureData,
}

#[derive(Debug, Clone)]
struct SignatureData {
    name: String,
    email: String,
    seconds: i64,
    offset_minutes: i32,
}

impl SignatureData {
    fn from_signature(sig: &Signature<'_>) -> Self {
        let when = sig.when();
        Self {
            name: sig.name().unwrap_or("git-autosnap").to_string(),
            email: sig.email().unwrap_or("git-autosnap@local").to_string(),
            seconds: when.seconds(),
            offset_minutes: when.offset_minutes(),
        }
    }

    fn to_signature(&self) -> Result<Signature<'static>> {
        let time = Time::new(self.seconds, self.offset_minutes);
        Signature::new(&self.name, &self.email, &time)
            .with_context(|| format!("failed to build signature {} <{}>", self.name, self.email))
    }
}

/// Compact old snapshot history by collapsing commits older than `days` into one baseline commit.
///
/// After compacting, this always runs post-maintenance:
/// - `git reflog expire --expire=now --all`
/// - `git gc --prune=now`
///
/// # Errors
/// Returns an error if repository rewrite or post-gc commands fail.
pub fn compact(repo_root: &Path, days: u32) -> Result<CompactResult> {
    let autosnap = autosnap_dir(repo_root);
    if !autosnap.exists() {
        return Ok(CompactResult {
            before_commits: 0,
            after_commits: 0,
            rewritten: false,
            baseline_created: false,
        });
    }

    let _ops_lock = acquire_ops_lock(repo_root)?;

    let repo = Repository::open(&autosnap)
        .with_context(|| format!("failed to open autosnap repo at {}", autosnap.display()))?;

    let commits = list_commits_oldest_first(&repo)?;
    let before_commits = commits.len();
    let cutoff = cutoff_epoch(days);

    if before_commits == 0 {
        run_post_gc(&autosnap)?;
        return Ok(CompactResult {
            before_commits,
            after_commits: 0,
            rewritten: false,
            baseline_created: false,
        });
    }

    let (old_oids, keep_oids): (Vec<Oid>, Vec<Oid>) = commits.into_iter().partition(|oid| {
        repo.find_commit(*oid)
            .map(|c| c.time().seconds() < cutoff)
            .unwrap_or(false)
    });

    if old_oids.is_empty() {
        run_post_gc(&autosnap)?;
        return Ok(CompactResult {
            before_commits,
            after_commits: before_commits,
            rewritten: false,
            baseline_created: false,
        });
    }

    let baseline_source_oid = *old_oids
        .last()
        .context("missing baseline source in old commit set")?;
    let baseline_source = repo
        .find_commit(baseline_source_oid)
        .context("failed to load baseline source commit")?;

    let baseline_tree = baseline_source
        .tree()
        .context("failed to load baseline source tree")?;
    let baseline_author =
        SignatureData::from_signature(&baseline_source.author()).to_signature()?;
    let baseline_committer =
        SignatureData::from_signature(&baseline_source.committer()).to_signature()?;

    let baseline_oid = repo
        .commit(
            None,
            &baseline_author,
            &baseline_committer,
            BASELINE_MESSAGE,
            &baseline_tree,
            &[],
        )
        .context("failed to create baseline commit")?;

    let mut parent_oid = baseline_oid;

    for keep_oid in keep_oids {
        let src = repo
            .find_commit(keep_oid)
            .with_context(|| format!("failed to load keep commit {keep_oid}"))?;

        let replay = CommitReplayData {
            tree_id: src.tree_id(),
            message: src.message().unwrap_or("").to_string(),
            author: SignatureData::from_signature(&src.author()),
            committer: SignatureData::from_signature(&src.committer()),
        };

        let tree = repo
            .find_tree(replay.tree_id)
            .with_context(|| format!("failed to find tree {}", replay.tree_id))?;
        let parent = repo
            .find_commit(parent_oid)
            .with_context(|| format!("failed to find parent commit {parent_oid}"))?;

        let author = replay.author.to_signature()?;
        let committer = replay.committer.to_signature()?;

        parent_oid = repo
            .commit(
                None,
                &author,
                &committer,
                &replay.message,
                &tree,
                &[&parent],
            )
            .with_context(|| format!("failed to replay commit {keep_oid}"))?;
    }

    update_head_target(&repo, parent_oid)?;
    run_post_gc(&autosnap)?;

    let after_commits = list_commits_oldest_first(&repo)?.len();

    Ok(CompactResult {
        before_commits,
        after_commits,
        rewritten: true,
        baseline_created: true,
    })
}

fn cutoff_epoch(days: u32) -> i64 {
    use time::{Duration, OffsetDateTime};
    let now = OffsetDateTime::now_utc();
    (now - Duration::days(i64::from(days))).unix_timestamp()
}

fn list_commits_oldest_first(repo: &Repository) -> Result<Vec<Oid>> {
    let mut revwalk = repo.revwalk().context("failed to create revwalk")?;
    if revwalk.push_head().is_err() {
        return Ok(Vec::new());
    }

    let mut oids = Vec::new();
    for oid in revwalk {
        oids.push(oid.context("failed to iterate revwalk")?);
    }
    oids.reverse();

    Ok(oids)
}

fn update_head_target(repo: &Repository, target: Oid) -> Result<()> {
    let mut head = repo
        .find_reference("HEAD")
        .context("failed to find HEAD reference")?;

    if let Some(symbolic_target) = head.symbolic_target().map(std::string::ToString::to_string) {
        match repo.find_reference(&symbolic_target) {
            Ok(mut reference) => {
                reference
                    .set_target(target, "autosnap compact")
                    .with_context(|| format!("failed to update {symbolic_target}"))?;
            }
            Err(_) => {
                let _ = repo
                    .reference(&symbolic_target, target, true, "autosnap compact")
                    .with_context(|| format!("failed to create {symbolic_target}"))?;
            }
        }
    } else {
        let _ = head
            .set_target(target, "autosnap compact")
            .context("failed to update HEAD target")?;
    }

    Ok(())
}

fn run_post_gc(autosnap: &Path) -> Result<()> {
    let gitdir = autosnap.to_string_lossy().to_string();

    let status = Command::new("git")
        .args([
            format!("--git-dir={gitdir}").as_str(),
            "reflog",
            "expire",
            "--expire=now",
            "--all",
        ])
        .status()
        .context("failed to run git reflog expire --expire=now --all")?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "git reflog expire --expire=now --all exited with status {status}"
        ));
    }

    let status = Command::new("git")
        .args([format!("--git-dir={gitdir}").as_str(), "gc", "--prune=now"])
        .status()
        .context("failed to run git gc --prune=now")?;
    if !status.success() {
        return Err(anyhow::anyhow!(
            "git gc --prune=now exited with status {status}"
        ));
    }

    Ok(())
}
