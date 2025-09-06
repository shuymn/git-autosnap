#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

/// Test basic restore functionality
#[tokio::test]
async fn test_restore_basic() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Initialize a git repo in the container
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'original content' > test.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Modify the file
    exec_in(&container, "/repo", "echo 'modified content' > test.txt").await?;

    // Restore from snapshot
    exec_in(&container, "/repo", "git autosnap restore --force HEAD").await?;

    // Verify file was restored
    let content = exec_in(&container, "/repo", "cat test.txt").await?;
    assert_eq!(
        content.trim(),
        "original content",
        "File was not properly restored"
    );

    Ok(())
}

/// Test dry-run mode
#[tokio::test]
async fn test_restore_dry_run() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Initialize a git repo in the container
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'original content' > test.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Modify the file
    exec_in(&container, "/repo", "echo 'modified content' > test.txt").await?;

    // Restore with dry-run
    exec_in(&container, "/repo", "git autosnap restore --dry-run HEAD").await?;

    // Verify file was NOT changed (dry-run)
    let content = exec_in(&container, "/repo", "cat test.txt").await?;
    assert_eq!(
        content.trim(),
        "modified content",
        "File was changed during dry-run"
    );

    Ok(())
}

/// Test safety check (refuses to restore with uncommitted changes)
#[tokio::test]
async fn test_restore_safety_check() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Initialize a git repo in the container
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and commit it
    exec_in(&container, "/repo", "echo 'original content' > test.txt").await?;
    exec_in(&container, "/repo", "git add test.txt").await?;
    exec_in(&container, "/repo", "git commit -m 'Initial commit'").await?;

    // Take a snapshot
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Modify the file (creating uncommitted changes)
    exec_in(&container, "/repo", "echo 'modified content' > test.txt").await?;

    // Try to restore without --force (should fail)
    let result = exec_in(&container, "/repo", "git autosnap restore HEAD").await;
    assert!(
        result.is_err(),
        "Restore should have failed with uncommitted changes"
    );

    // Verify the error message contains expected text
    if let Err(e) = result {
        let error_msg = e.to_string();
        assert!(
            error_msg.contains("uncommitted changes") || error_msg.contains("command failed"),
            "Expected error about uncommitted changes, got: {error_msg}",
        );
    }

    Ok(())
}

/// Test full restore mode
#[tokio::test]
async fn test_restore_full_mode() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Initialize a git repo in the container
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'original content' > test.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Create an extra file that's not in the snapshot
    exec_in(&container, "/repo", "echo 'extra content' > extra.txt").await?;

    // Full restore should remove the extra file
    exec_in(
        &container,
        "/repo",
        "git autosnap restore --force --full HEAD",
    )
    .await?;

    // Verify extra file was removed
    let ls_result = exec_in(&container, "/repo", "ls -la extra.txt 2>&1").await;
    assert!(
        ls_result.is_err() || ls_result.unwrap().contains("No such file"),
        "Extra file should have been removed in full restore"
    );

    // Verify original file still exists
    let test_file = exec_in(&container, "/repo", "ls test.txt").await?;
    assert!(
        test_file.contains("test.txt"),
        "Original file should still exist"
    );

    // Verify .autosnap was NOT removed
    let autosnap_dir = exec_in(&container, "/repo", "ls -d .autosnap").await?;
    assert!(
        autosnap_dir.contains(".autosnap"),
        ".autosnap should not be removed"
    );

    // Verify .git was NOT removed
    let git_dir = exec_in(&container, "/repo", "ls -d .git").await?;
    assert!(git_dir.contains(".git"), ".git should not be removed");

    Ok(())
}
