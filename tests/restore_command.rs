use assert_cmd::Command;
use std::fs;
use tempfile::TempDir;

/// Test basic restore functionality
#[test]
fn test_restore_basic() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Initialize a git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Configure git user
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Initialize autosnap
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success();

    // Create a test file and take a snapshot
    let test_file = repo_path.join("test.txt");
    fs::write(&test_file, "original content").unwrap();

    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("once")
        .assert()
        .success();

    // Modify the file
    fs::write(&test_file, "modified content").unwrap();

    // Restore from snapshot
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .args(["restore", "--force", "HEAD"])
        .assert()
        .success();

    // Verify file was restored
    let content = fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "original content",
        "File was not properly restored"
    );
}

/// Test dry-run mode
#[test]
fn test_restore_dry_run() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Initialize a git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Configure git user
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Initialize autosnap
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success();

    // Create a test file and take a snapshot
    let test_file = repo_path.join("test.txt");
    fs::write(&test_file, "original content").unwrap();

    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("once")
        .assert()
        .success();

    // Modify the file
    fs::write(&test_file, "modified content").unwrap();

    // Restore with dry-run
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .args(["restore", "--dry-run", "HEAD"])
        .assert()
        .success();

    // Verify file was NOT changed (dry-run)
    let content = fs::read_to_string(&test_file).unwrap();
    assert_eq!(
        content, "modified content",
        "File was changed during dry-run"
    );
}

/// Test safety check (refuses to restore with uncommitted changes)
#[test]
fn test_restore_safety_check() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Initialize a git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Configure git user for commits
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Initialize autosnap
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success();

    // Create a test file and commit it
    let test_file = repo_path.join("test.txt");
    fs::write(&test_file, "original content").unwrap();

    std::process::Command::new("git")
        .args(["add", "test.txt"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Take a snapshot
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("once")
        .assert()
        .success();

    // Modify the file (creating uncommitted changes)
    fs::write(&test_file, "modified content").unwrap();

    // Try to restore without --force (should fail)
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .args(["restore", "HEAD"])
        .assert()
        .failure()
        .stderr(predicates::str::contains("uncommitted changes"));
}

/// Test full restore mode
#[test]
fn test_restore_full_mode() {
    let temp = TempDir::new().unwrap();
    let repo_path = temp.path();

    // Initialize a git repo
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Configure git user
    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Initialize autosnap
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success();

    // Create a test file and take a snapshot
    let test_file = repo_path.join("test.txt");
    fs::write(&test_file, "original content").unwrap();

    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("once")
        .assert()
        .success();

    // Create an extra file that's not in the snapshot
    let extra_file = repo_path.join("extra.txt");
    fs::write(&extra_file, "extra content").unwrap();

    // Full restore should remove the extra file
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .args(["restore", "--force", "--full", "HEAD"])
        .assert()
        .success();

    // Verify extra file was removed
    assert!(
        !extra_file.exists(),
        "Extra file should have been removed in full restore"
    );

    // Verify original file still exists
    assert!(test_file.exists(), "Original file should still exist");

    // Verify .autosnap was NOT removed
    assert!(
        repo_path.join(".autosnap").exists(),
        ".autosnap should not be removed"
    );

    // Verify .git was NOT removed
    assert!(
        repo_path.join(".git").exists(),
        ".git should not be removed"
    );
}
