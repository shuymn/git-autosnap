#![cfg(feature = "container-tests")]
#![allow(clippy::future_not_send)]

use anyhow::{Context, Result};
use testcontainers::{ContainerAsync, GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

async fn read_usize(container: &ContainerAsync<GenericImage>, cmd: &str) -> Result<usize> {
    let output = exec_in(container, "/repo", cmd).await?;
    output
        .trim()
        .parse::<usize>()
        .with_context(|| format!("failed to parse usize from output: {output}"))
}

async fn setup_repo_with_seed(
    container: &ContainerAsync<GenericImage>,
    seed_script: &str,
) -> Result<()> {
    exec_bash(container, "mkdir -p /repo && git init /repo").await?;
    exec_in(container, "/repo", "git autosnap init").await?;
    exec_in(container, "/repo", seed_script).await?;
    Ok(())
}

#[tokio::test]
async fn compact_reduces_commit_count_and_preserves_head_tree() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    let seed_script = r#"bash -lc '
set -euo pipefail
make_commit() {
  days="$1"
  content="$2"
  msg="$3"
  blob=$(printf "%s" "$content" | git --git-dir=.autosnap hash-object -w --stdin)
  tree=$(printf "100644 blob %s\tstate.txt\n" "$blob" | git --git-dir=.autosnap mktree)
  date=$(date -u -d "$days days ago" "+%Y-%m-%dT12:00:00Z")
  if git --git-dir=.autosnap rev-parse -q --verify HEAD >/dev/null 2>&1; then
    parent=$(git --git-dir=.autosnap rev-parse HEAD)
    oid=$(printf "%s\n" "$msg" | GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.com GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.com GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" git --git-dir=.autosnap commit-tree "$tree" -p "$parent")
  else
    oid=$(printf "%s\n" "$msg" | GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.com GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.com GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" git --git-dir=.autosnap commit-tree "$tree")
  fi
  git --git-dir=.autosnap update-ref HEAD "$oid"
}
make_commit 120 v120 "AUTOSNAP old 120"
make_commit 80 v80 "AUTOSNAP old 80"
make_commit 40 v40 "AUTOSNAP old 40"
make_commit 10 v10 "AUTOSNAP keep 10"
make_commit 2 v2 "AUTOSNAP keep 2"
'"#;
    setup_repo_with_seed(&container, seed_script).await?;

    let before = read_usize(&container, "git --git-dir=.autosnap rev-list --count HEAD").await?;
    exec_in(&container, "/repo", "git autosnap compact --days 30").await?;
    let after = read_usize(&container, "git --git-dir=.autosnap rev-list --count HEAD").await?;

    assert!(
        after < before,
        "expected commit count to decrease: before={before} after={after}"
    );

    let baseline_count = read_usize(
        &container,
        "bash -lc \"git --git-dir=.autosnap log --format=%s | grep -c '^AUTOSNAP_COMPACT_BASELINE$' || true\"",
    )
    .await?;
    assert_eq!(baseline_count, 1, "baseline commit must be unique");

    let head_state = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap show HEAD:state.txt",
    )
    .await?;
    assert_eq!(
        head_state.trim(),
        "v2",
        "HEAD tree must remain latest state"
    );

    Ok(())
}

#[tokio::test]
async fn recompact_with_existing_baseline_keeps_single_baseline() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    let seed_script = r#"bash -lc '
set -euo pipefail
make_commit() {
  days="$1"
  content="$2"
  msg="$3"
  blob=$(printf "%s" "$content" | git --git-dir=.autosnap hash-object -w --stdin)
  tree=$(printf "100644 blob %s\tstate.txt\n" "$blob" | git --git-dir=.autosnap mktree)
  date=$(date -u -d "$days days ago" "+%Y-%m-%dT12:00:00Z")
  if git --git-dir=.autosnap rev-parse -q --verify HEAD >/dev/null 2>&1; then
    parent=$(git --git-dir=.autosnap rev-parse HEAD)
    oid=$(printf "%s\n" "$msg" | GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.com GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.com GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" git --git-dir=.autosnap commit-tree "$tree" -p "$parent")
  else
    oid=$(printf "%s\n" "$msg" | GIT_AUTHOR_NAME=Test GIT_AUTHOR_EMAIL=test@example.com GIT_COMMITTER_NAME=Test GIT_COMMITTER_EMAIL=test@example.com GIT_AUTHOR_DATE="$date" GIT_COMMITTER_DATE="$date" git --git-dir=.autosnap commit-tree "$tree")
  fi
  git --git-dir=.autosnap update-ref HEAD "$oid"
}
make_commit 120 v120 "AUTOSNAP old 120"
make_commit 90 v90 "AUTOSNAP old 90"
make_commit 45 v45 "AUTOSNAP old 45"
make_commit 20 v20 "AUTOSNAP keep 20"
make_commit 5 v5 "AUTOSNAP keep 5"
'"#;
    setup_repo_with_seed(&container, seed_script).await?;

    exec_in(&container, "/repo", "git autosnap compact --days 30").await?;
    exec_in(&container, "/repo", "git autosnap compact --days 7").await?;

    let baseline_second = read_usize(
        &container,
        "bash -lc \"git --git-dir=.autosnap log --format=%s | grep -c '^AUTOSNAP_COMPACT_BASELINE$' || true\"",
    )
    .await?;
    let count_second =
        read_usize(&container, "git --git-dir=.autosnap rev-list --count HEAD").await?;
    let head_second = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap show HEAD:state.txt",
    )
    .await?;

    exec_in(&container, "/repo", "git autosnap compact --days 7").await?;

    let baseline_third = read_usize(
        &container,
        "bash -lc \"git --git-dir=.autosnap log --format=%s | grep -c '^AUTOSNAP_COMPACT_BASELINE$' || true\"",
    )
    .await?;
    let count_third =
        read_usize(&container, "git --git-dir=.autosnap rev-list --count HEAD").await?;
    let head_third = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap show HEAD:state.txt",
    )
    .await?;

    assert_eq!(baseline_second, 1, "baseline count after second compact");
    assert_eq!(baseline_third, 1, "baseline count after third compact");
    assert_eq!(
        count_second, count_third,
        "re-running compact should be stable"
    );
    assert_eq!(head_second.trim(), "v5", "HEAD must stay at latest content");
    assert_eq!(head_third.trim(), "v5", "HEAD must stay at latest content");

    Ok(())
}
