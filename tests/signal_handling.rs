#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

#[tokio::test]
async fn test_sigterm_handling() -> Result<()> {
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

    // Read PID
    let pid_output = exec_in(&container, "/repo", "cat .autosnap/autosnap.pid").await?;
    let pid = pid_output.trim();

    // Send SIGTERM
    exec_bash(&container, &format!("kill -TERM {}", pid)).await?;

    // Wait a bit for signal handling
    exec_in(&container, "/repo", "sleep 1").await?;

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

#[tokio::test]
async fn test_sigint_handling() -> Result<()> {
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

    // Read PID
    let pid_output = exec_in(&container, "/repo", "cat .autosnap/autosnap.pid").await?;
    let pid = pid_output.trim();

    // Send SIGINT
    exec_bash(&container, &format!("kill -INT {}", pid)).await?;

    // Wait a bit for signal handling
    exec_in(&container, "/repo", "sleep 1").await?;

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

#[tokio::test]
async fn test_sigusr1_handling() -> Result<()> {
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

    // Create a test file
    exec_in(&container, "/repo", "echo 'test content' > test.txt").await?;

    // Read PID
    let pid_output = exec_in(&container, "/repo", "cat .autosnap/autosnap.pid").await?;
    let pid = pid_output.trim();

    // Send SIGUSR1 to force a snapshot
    exec_bash(&container, &format!("kill -USR1 {}", pid)).await?;

    // Wait a bit for snapshot creation
    exec_in(&container, "/repo", "sleep 1").await?;

    // Verify a snapshot was created
    let log_output = exec_in(&container, "/repo", "git --git-dir=.autosnap log --oneline").await?;
    assert!(log_output.contains("AUTOSNAP"));

    // Stop daemon
    exec_in(&container, "/repo", "git autosnap stop").await?;

    Ok(())
}

#[tokio::test]
async fn test_sigusr2_handling() -> Result<()> {
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

    // Read PID
    let pid_output = exec_in(&container, "/repo", "cat .autosnap/autosnap.pid").await?;
    let pid = pid_output.trim();

    // Send SIGUSR2 to prepare for binary update
    exec_bash(&container, &format!("kill -USR2 {}", pid)).await?;

    // Wait a bit for signal handling
    exec_in(&container, "/repo", "sleep 1").await?;

    // Verify daemon is no longer running (should have exited for binary update)
    let status_output = exec_in(
        &container,
        "/repo",
        "sh -lc 'git autosnap status; echo EXIT:$?; true'",
    )
    .await?;
    assert!(status_output.contains("EXIT:1"));

    Ok(())
}
