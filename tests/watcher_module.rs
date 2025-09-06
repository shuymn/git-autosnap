#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn test_debounce_handling() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Start daemon with a short debounce time for testing
    exec_in(&container, "/repo", "git autosnap start --daemon").await?;

    // Wait for pidfile to appear
    exec_in(&container, "/repo", "sh -lc 'for i in $(seq 1 30); do [ -f .autosnap/autosnap.pid ] && break; sleep 0.1; done; test -f .autosnap/autosnap.pid'").await?;

    // Create all file changes in a single command to ensure they happen within debounce window
    let script = r#"
        for i in 1 2 3 4 5; do
            echo "change $i" > file.txt
            sleep 0.01
        done
    "#;
    exec_bash(&container, &format!("cd /repo && {script}")).await?;

    // Wait for debounce window to complete and snapshot to be created
    exec_in(&container, "/repo", "sleep 1").await?;

    // Verify only one snapshot was created (debouncing worked)
    let log_output = exec_in(&container, "/repo", "git --git-dir=.autosnap log --oneline").await?;
    let snapshot_count = log_output
        .lines()
        .filter(|l| l.contains("AUTOSNAP"))
        .count();

    // Allow for 1-2 snapshots (timing can vary in container environment)
    assert!(
        (1..=2).contains(&snapshot_count),
        "Expected 1-2 snapshots due to debouncing, got {snapshot_count}"
    );

    // Stop daemon
    exec_in(&container, "/repo", "git autosnap stop").await?;

    Ok(())
}

#[tokio::test]
async fn test_ignore_file_updates() -> Result<()> {
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

    // Create a .gitignore file
    exec_in(&container, "/repo", "echo '*.tmp' > .gitignore").await?;

    // Create some files that should be tracked and some that should be ignored
    exec_in(&container, "/repo", "echo 'tracked content' > tracked.txt").await?;
    exec_in(&container, "/repo", "echo 'ignored content' > ignored.tmp").await?;

    // Wait for file events to be processed
    exec_in(&container, "/repo", "sleep 1").await?;

    // Check that only the tracked file was included in the snapshot
    let log_output = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap log -p --name-only",
    )
    .await?;
    assert!(log_output.contains("tracked.txt"));
    assert!(!log_output.contains("ignored.tmp"));

    // Stop daemon
    exec_in(&container, "/repo", "git autosnap stop").await?;

    Ok(())
}

#[tokio::test]
async fn test_file_events() -> Result<()> {
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

    // Create a test file
    exec_in(&container, "/repo", "echo 'initial content' > test.txt").await?;

    // Wait for snapshot to be created
    exec_in(&container, "/repo", "sleep 1").await?;

    // Verify initial snapshot was created
    let log_output = exec_in(&container, "/repo", "git --git-dir=.autosnap log --oneline").await?;
    assert!(log_output.contains("AUTOSNAP"));

    // Modify the test file
    exec_in(&container, "/repo", "echo 'modified content' > test.txt").await?;

    // Wait for another snapshot to be created
    exec_in(&container, "/repo", "sleep 1").await?;

    // Verify another snapshot was created
    let log_output = exec_in(&container, "/repo", "git --git-dir=.autosnap log --oneline").await?;
    let snapshot_count = log_output
        .lines()
        .filter(|l| l.contains("AUTOSNAP"))
        .count();
    assert_eq!(snapshot_count, 2, "Should have created two snapshots");

    // Stop daemon
    exec_in(&container, "/repo", "git autosnap stop").await?;

    Ok(())
}
