#![cfg(feature = "container-tests")]

use anyhow::Result;
use predicates::prelude::*;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn daemon_start_status_stop() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Start in daemon mode
    exec_in(&container, "/repo", "git autosnap start --daemon").await?;

    // Wait for pidfile to appear
    exec_in(&container, "/repo", "sh -lc 'for i in $(seq 1 30); do [ -f .autosnap/autosnap.pid ] && break; sleep 0.1; done; test -f .autosnap/autosnap.pid'").await?;

    // Status should be running (exit code == 0)
    exec_in(&container, "/repo", "git autosnap status").await?;

    // Stop
    exec_in(&container, "/repo", "git autosnap stop").await?;

    // Status should be non-zero; capture code without failing the helper
    let out = exec_in(
        &container,
        "/repo",
        "sh -lc 'git autosnap status; echo EXIT:$?; true'",
    )
    .await?;
    assert!(out.contains("EXIT:1"), "unexpected status output: {out}");
    // And pidfile should be gone
    exec_in(&container, "/repo", "test ! -f .autosnap/autosnap.pid").await?;

    Ok(())
}

#[tokio::test]
async fn daemon_creates_snapshot_on_change() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;
    exec_in(&container, "/repo", "git autosnap start --daemon").await?;

    // Trigger a change
    exec_in(&container, "/repo", "sh -lc 'echo hello > watched.txt'").await?;
    // Allow debounce window and processing time
    exec_in(&container, "/repo", "sh -lc 'sleep 1'").await?;

    // Expect at least one commit in .autosnap
    let subject = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap log -1 --format=%s",
    )
    .await?;
    let re = predicate::str::is_match(r"^AUTOSNAP\[[^\]]+\] \d{4}-\d{2}-\d{2}T\d{2}:\d{2}:")?;
    assert!(re.eval(subject.trim()), "bad subject: {subject}");

    // Stop the daemon
    exec_in(&container, "/repo", "git autosnap stop").await?;
    Ok(())
}
