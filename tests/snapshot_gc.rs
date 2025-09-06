#![cfg(feature = "container-tests")]

use anyhow::Result;
use predicates::prelude::*;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn snapshot_commit_message_and_contents() -> Result<()> {
    // Use prebuilt image if available, otherwise use the alpine image with git installed
    // The image should include git, bash, and git-autosnap in PATH
    // Build with: docker build -f Dockerfile.test -t git-autosnap-test:latest .
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));

    let container = image.start().await?;

    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;
    exec_in(&container, "/repo", "sh -c 'echo hello > a.txt'").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    let subject = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap log -1 --format=%s",
    )
    .await?;
    let re = predicate::str::is_match(r"^AUTOSNAP\[[^\]]+\] \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:")?
        .and(predicate::str::contains("AUTOSNAP["));
    assert!(re.eval(subject.trim()), "bad subject: {subject}");

    let listing = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap ls-tree -r HEAD",
    )
    .await?;
    assert!(
        listing.contains("a.txt"),
        "missing a.txt in tree: {listing}"
    );
    Ok(())
}

#[tokio::test]
async fn gc_invocation_succeeds() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;
    exec_in(&container, "/repo", "sh -c 'echo content > f.txt'").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    exec_in(&container, "/repo", "git autosnap gc --prune --days 1").await?;
    Ok(())
}
