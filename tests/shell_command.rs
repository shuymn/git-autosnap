#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn test_shell_basic() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'test content' > test.txt").await?;
    let commit_output = exec_in(&container, "/repo", "git autosnap once").await?;
    let commit_hash = commit_output.trim();

    // Test basic shell command
    let shell_output = exec_in(
        &container,
        "/repo",
        &format!("echo 'ls' | git autosnap shell {commit_hash}"),
    )
    .await?;
    assert!(shell_output.contains("test.txt"));

    Ok(())
}

#[tokio::test]
async fn test_shell_with_commit() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create test files and take multiple snapshots
    exec_in(&container, "/repo", "echo 'content1' > file1.txt").await?;
    let commit1_output = exec_in(&container, "/repo", "git autosnap once").await?;
    let commit1_hash = commit1_output.trim();

    exec_in(&container, "/repo", "echo 'content2' > file2.txt").await?;
    let commit2_output = exec_in(&container, "/repo", "git autosnap once").await?;
    let commit2_hash = commit2_output.trim();

    // Test shell with specific commit
    let shell_output = exec_in(
        &container,
        "/repo",
        &format!("echo 'ls' | git autosnap shell {commit1_hash}"),
    )
    .await?;
    assert!(shell_output.contains("file1.txt"));
    assert!(!shell_output.contains("file2.txt"));

    let shell_output = exec_in(
        &container,
        "/repo",
        &format!("echo 'ls' | git autosnap shell {commit2_hash}"),
    )
    .await?;
    assert!(shell_output.contains("file1.txt"));
    assert!(shell_output.contains("file2.txt"));

    Ok(())
}

#[tokio::test]
#[ignore = "Interactive tests require TTY which is not available in container environment"]
async fn test_shell_interactive() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create test files and take snapshots
    exec_in(&container, "/repo", "echo 'content1' > file1.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    exec_in(&container, "/repo", "echo 'content2' > file2.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test interactive shell mode (simulating user input)
    // We'll test by providing input through a script
    let script = r"echo -e '\nls\nexit' | git autosnap shell -i";
    let shell_output = exec_in(&container, "/repo", script).await?;

    // Should contain file listing from the snapshot
    assert!(shell_output.contains("file1.txt"));
    assert!(shell_output.contains("file2.txt"));

    Ok(())
}
