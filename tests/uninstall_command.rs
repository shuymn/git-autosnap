#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn test_uninstall_basic() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Verify .autosnap directory exists
    exec_in(&container, "/repo", "test -d .autosnap").await?;

    // Create a test file
    exec_in(&container, "/repo", "echo 'test content' > test.txt").await?;

    // Take a snapshot
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Verify snapshots exist
    let log_output = exec_in(&container, "/repo", "git --git-dir=.autosnap log --oneline").await?;
    assert!(log_output.contains("AUTOSNAP"));

    // Uninstall
    exec_in(&container, "/repo", "git autosnap uninstall").await?;

    // Verify .autosnap directory is removed
    let uninstall_output =
        exec_in(&container, "/repo", "test ! -d .autosnap || echo exists").await?;
    assert!(!uninstall_output.contains("exists"));

    // Verify uninstall command reports removal
    let uninstall_output = exec_in(&container, "/repo", "git autosnap uninstall").await?;
    assert!(uninstall_output.contains("Nothing to remove"));

    Ok(())
}

#[tokio::test]
async fn test_uninstall_with_daemon_running() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Start daemon
    exec_in(&container, "/repo", "git autosnap start --daemon").await?;

    // Wait for pidfile to appear
    exec_in(&container, "/repo", "sh -lc 'for i in $(seq 1 30); do [ -f .autosnap/autosnap.pid ] && break; sleep 0.1; done; test -f .autosnap/autosnap.pid'").await?;

    // Verify daemon is running
    exec_in(&container, "/repo", "git autosnap status").await?;

    // Try to uninstall while daemon is running
    let uninstall_output = exec_in(&container, "/repo", "git autosnap uninstall").await?;
    assert!(uninstall_output.contains("Removed"));

    // Verify daemon is no longer running
    let status_output = exec_in(
        &container,
        "/repo",
        "sh -lc 'git autosnap status; echo EXIT:$?; true'",
    )
    .await?;
    assert!(status_output.contains("EXIT:1"));

    Ok(())
}
