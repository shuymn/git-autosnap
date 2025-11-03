use assert_cmd::{Command, cargo::cargo_bin_cmd};
use predicates::prelude::*;
use tempfile::TempDir;

fn git_autosnap_cmd() -> Command {
    cargo_bin_cmd!("git-autosnap")
}

#[test]
fn test_diff_help() {
    git_autosnap_cmd()
        .arg("diff")
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Show diff between snapshots"));
}

#[test]
fn test_diff_without_autosnap() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize a git repo but not autosnap
    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Try to run diff without autosnap initialized
    git_autosnap_cmd()
        .arg("diff")
        .current_dir(&temp_dir)
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            ".autosnap is missing; run `git autosnap init` first",
        ));
}

#[test]
fn test_diff_formats() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize git repo
    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Configure git user
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&temp_dir)
        .assert()
        .success();

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Initialize autosnap
    git_autosnap_cmd()
        .arg("init")
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Create a test file and take a snapshot
    std::fs::write(temp_dir.path().join("test.txt"), "initial content").unwrap();

    git_autosnap_cmd()
        .arg("once")
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Modify the file
    std::fs::write(temp_dir.path().join("test.txt"), "modified content").unwrap();

    // Test --stat format
    git_autosnap_cmd()
        .args(["diff", "--stat"])
        .current_dir(&temp_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("1 files changed"));

    // Test --name-only format
    git_autosnap_cmd()
        .args(["diff", "--name-only"])
        .current_dir(&temp_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("test.txt"));

    // Test --name-status format
    git_autosnap_cmd()
        .args(["diff", "--name-status"])
        .current_dir(&temp_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("M\ttest.txt"));
}

#[test]
fn test_diff_between_commits() {
    let temp_dir = TempDir::new().unwrap();

    // Initialize git repo
    Command::new("git")
        .arg("init")
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Configure git user
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(&temp_dir)
        .assert()
        .success();

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Initialize autosnap
    git_autosnap_cmd()
        .arg("init")
        .current_dir(&temp_dir)
        .assert()
        .success();

    // Create first snapshot
    std::fs::write(temp_dir.path().join("file1.txt"), "content1").unwrap();

    let output1 = git_autosnap_cmd()
        .arg("once")
        .current_dir(&temp_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let commit1 = String::from_utf8_lossy(&output1).trim().to_string();

    // Create second snapshot
    std::fs::write(temp_dir.path().join("file2.txt"), "content2").unwrap();

    let output2 = git_autosnap_cmd()
        .arg("once")
        .current_dir(&temp_dir)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    let commit2 = String::from_utf8_lossy(&output2).trim().to_string();

    // Test diff between two commits
    git_autosnap_cmd()
        .args(["diff", &commit1, &commit2, "--name-only"])
        .current_dir(&temp_dir)
        .assert()
        .success()
        .stdout(predicate::str::contains("file2.txt"));
}
