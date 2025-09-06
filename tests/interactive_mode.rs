#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

// Helper to check if TTY is available in container
#[allow(clippy::future_not_send)]
async fn has_tty<I: testcontainers::Image>(container: &testcontainers::ContainerAsync<I>) -> bool {
    exec_bash(container, "test -t 0").await.is_ok()
}

#[tokio::test]
#[ignore = "Interactive tests require TTY which is not available in container environment"]
async fn test_interactive_selection() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Skip test if no TTY available
    if !has_tty(&container).await {
        println!("Skipping test: No TTY available in container");
        return Ok(());
    }

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create test files and take snapshots
    exec_in(&container, "/repo", "echo 'content1' > file1.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    exec_in(&container, "/repo", "echo 'content2' > file2.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test interactive restore mode (simulating user input)
    // We'll test by providing input through a script
    let script = r"echo -e '\n1\nexit' | git autosnap restore -i";
    let restore_output = exec_in(&container, "/repo", script).await?;

    // Should contain information about the restore operation
    assert!(restore_output.contains("Restoring from snapshot"));

    Ok(())
}

#[tokio::test]
#[ignore = "Interactive tests require TTY which is not available in container environment"]
async fn test_interactive_cancel() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Skip test if no TTY available
    if !has_tty(&container).await {
        println!("Skipping test: No TTY available in container");
        return Ok(());
    }

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create a test file and take a snapshot
    exec_in(&container, "/repo", "echo 'content1' > file1.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test interactive mode cancellation (simulating ESC key)
    // We'll test by providing empty input to simulate cancellation
    let script = r"echo -e '\n\n' | git autosnap restore -i";
    let restore_output = exec_in(&container, "/repo", script).await?;

    // Should contain error about no snapshot selected
    assert!(restore_output.contains("No snapshot selected"));

    Ok(())
}

#[tokio::test]
#[ignore = "Interactive tests require TTY which is not available in container environment"]
async fn test_interactive_diff() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    // Skip test if no TTY available
    if !has_tty(&container).await {
        println!("Skipping test: No TTY available in container");
        return Ok(());
    }

    // Create a test repository
    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;

    // Create test files and take snapshots
    exec_in(&container, "/repo", "echo 'content1' > file1.txt").await?;
    let commit1_output = exec_in(&container, "/repo", "git autosnap once").await?;
    let commit1_hash = commit1_output.trim();

    exec_in(&container, "/repo", "echo 'content2' > file1.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    // Test interactive diff mode (simulating user input)
    // We'll test by providing input through a script
    let script = format!(r"echo -e '\n{commit1_hash}\n' | git autosnap diff -i");
    let diff_output = exec_in(&container, "/repo", &script).await?;

    // Should contain diff information
    assert!(diff_output.contains("@@"));

    Ok(())
}
