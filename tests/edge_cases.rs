#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in, exec_in_allow_fail};

#[tokio::test]
async fn test_restore_empty_paths() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create test files and take a snapshot
    exec_in(&container, "/repo", "echo 'content1' > file1.txt").await?;
    exec_in(
        &container,
        "/repo",
        "mkdir -p dir && echo 'content2' > dir/file2.txt",
    )
    .await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test restore with empty paths (should fail due to uncommitted changes)
    let restore_output = exec_in_allow_fail(&container, "/repo", "git autosnap restore").await?;
    assert!(restore_output.contains("Working tree has uncommitted changes"));

    Ok(())
}

#[tokio::test]
async fn test_diff_with_nonexistent_commits() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'test content' > test.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test diff with nonexistent first commit
    let diff_output =
        exec_in_allow_fail(&container, "/repo", "git autosnap diff nonexistent-commit").await?;
    assert!(diff_output.contains("failed to find commit"));

    Ok(())
}

#[tokio::test]
async fn test_shell_with_invalid_commit() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'test content' > test.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test shell with invalid commit
    let shell_output = exec_in_allow_fail(
        &container,
        "/repo",
        "echo 'ls' | git autosnap shell invalid-commit",
    )
    .await?;
    assert!(shell_output.contains("failed to parse commit reference"));

    Ok(())
}

#[tokio::test]
async fn test_daemon_with_corrupted_pidfile() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a corrupted pidfile
    exec_in(
        &container,
        "/repo",
        "echo 'invalid-pid' > .autosnap/autosnap.pid",
    )
    .await?;

    // Try to start daemon - should fail gracefully
    let start_output =
        exec_in_allow_fail(&container, "/repo", "git autosnap start --daemon").await?;
    assert!(start_output.contains("invalid pid in pidfile"));

    Ok(())
}

#[tokio::test]
async fn test_compact_without_snapshots() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository and init autosnap
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Run compact without any snapshots - should not fail
    let compact_output = exec_in(&container, "/repo", "git autosnap compact").await?;
    assert!(compact_output.is_empty() || !compact_output.contains("error"));

    Ok(())
}

#[tokio::test]
async fn test_once_without_changes() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository and init autosnap
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Take first snapshot
    exec_in(&container, "/repo", "git autosnap once").await?;
    let head_after_first = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap rev-parse HEAD",
    )
    .await?
    .trim()
    .to_string();

    // Take second snapshot without changes - should not create duplicate
    exec_in(&container, "/repo", "git autosnap once").await?;
    let head_after_second = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap rev-parse HEAD",
    )
    .await?
    .trim()
    .to_string();

    // Wait a bit and take third snapshot
    exec_in(&container, "/repo", "sleep 1").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;
    let head_after_third = exec_in(
        &container,
        "/repo",
        "git --git-dir=.autosnap rev-parse HEAD",
    )
    .await?
    .trim()
    .to_string();

    // Verify no new snapshot commits are created when contents are unchanged.
    assert_eq!(head_after_first, head_after_second);
    assert_eq!(head_after_second, head_after_third);

    // Verify only one snapshot exists
    let log_output = exec_in(&container, "/repo", "git --git-dir=.autosnap log --oneline").await?;
    let snapshot_count = log_output
        .lines()
        .filter(|l| l.contains("AUTOSNAP"))
        .count();
    assert_eq!(snapshot_count, 1);

    Ok(())
}
