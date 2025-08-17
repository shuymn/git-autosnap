#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn test_restore_with_uncommitted_changes() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create and commit a file in the main repo
    exec_in(
        &container,
        "/repo",
        "echo 'committed content' > committed.txt",
    )
    .await?;
    exec_in(&container, "/repo", "git add committed.txt").await?;
    exec_in(&container, "/repo", "git commit -m 'initial commit'").await?;

    // Create a test file and take a snapshot
    exec_in(
        &container,
        "/repo",
        "echo 'snapshot content' > snapshot.txt",
    )
    .await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Modify the committed file (creating uncommitted changes)
    exec_in(
        &container,
        "/repo",
        "echo 'modified content' > committed.txt",
    )
    .await?;

    // Try to restore without --force flag
    let restore_output = exec_in(&container, "/repo", "git autosnap restore").await?;
    assert!(restore_output.contains("Working tree has uncommitted changes"));

    Ok(())
}

#[tokio::test]
async fn test_diff_without_autosnap() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository but don't init autosnap
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;

    // Create a test file
    exec_in(&container, "/repo", "echo 'test content' > test.txt").await?;

    // Try to run diff without autosnap initialized
    let diff_output = exec_in(&container, "/repo", "git autosnap diff").await?;
    assert!(diff_output.contains("failed to open .autosnap repository"));

    Ok(())
}

#[tokio::test]
async fn test_shell_without_autosnap() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository but don't init autosnap
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;

    // Try to run shell without autosnap initialized
    let shell_output = exec_in(&container, "/repo", "git autosnap shell").await?;
    assert!(shell_output.contains(".autosnap is missing"));

    Ok(())
}

#[tokio::test]
async fn test_restore_nonexistent_commit() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Try to restore from a nonexistent commit
    let restore_output = exec_in(
        &container,
        "/repo",
        "git autosnap restore nonexistent-commit",
    )
    .await?;
    assert!(restore_output.contains("failed to parse commit reference"));

    Ok(())
}

#[tokio::test]
async fn test_diff_nonexistent_commits() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Try to diff between nonexistent commits
    let diff_output = exec_in(
        &container,
        "/repo",
        "git autosnap diff nonexistent-commit1 nonexistent-commit2",
    )
    .await?;
    assert!(diff_output.contains("failed to find commit"));

    Ok(())
}

#[tokio::test]
async fn test_uninstall_without_autosnap() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository but don't init autosnap
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;

    // Try to uninstall when .autosnap doesn't exist
    let uninstall_output = exec_in(&container, "/repo", "git autosnap uninstall").await?;
    assert!(uninstall_output.contains("Nothing to remove"));

    Ok(())
}
