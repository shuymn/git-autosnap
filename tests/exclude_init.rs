use assert_cmd::Command;
use git2::Repository;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_init_adds_to_git_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize a git repo
    Repository::init(repo_path).unwrap();

    // Run git autosnap init
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success()
        .stdout(predicates::str::contains("Initialized .autosnap"));

    // Check that .autosnap directory exists
    assert!(repo_path.join(".autosnap").exists());

    // Check that .git/info/exclude contains .autosnap
    let exclude_path = repo_path.join(".git").join("info").join("exclude");
    assert!(exclude_path.exists(), ".git/info/exclude should exist");

    let contents = fs::read_to_string(&exclude_path).unwrap();
    assert!(
        contents.lines().any(|line| line.trim() == ".autosnap"),
        ".git/info/exclude should contain .autosnap pattern"
    );
}

#[test]
fn test_init_idempotent_for_exclude() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize a git repo
    Repository::init(repo_path).unwrap();

    // Run git autosnap init twice
    for _ in 0..2 {
        Command::cargo_bin("git-autosnap")
            .unwrap()
            .current_dir(repo_path)
            .arg("init")
            .assert()
            .success();
    }

    // Check that .autosnap appears only once in exclude
    let exclude_path = repo_path.join(".git").join("info").join("exclude");
    let contents = fs::read_to_string(&exclude_path).unwrap();
    let count = contents
        .lines()
        .filter(|line| line.trim() == ".autosnap")
        .count();

    assert_eq!(
        count, 1,
        ".autosnap should appear exactly once in .git/info/exclude"
    );
}

#[test]
fn test_init_preserves_existing_exclude_entries() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize a git repo
    Repository::init(repo_path).unwrap();

    // Add some existing entries to .git/info/exclude
    let exclude_path = repo_path.join(".git").join("info").join("exclude");
    fs::create_dir_all(exclude_path.parent().unwrap()).unwrap();
    fs::write(&exclude_path, "*.log\n.DS_Store\n").unwrap();

    // Run git autosnap init
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success();

    // Check that both old entries and .autosnap are present
    let contents = fs::read_to_string(&exclude_path).unwrap();
    assert!(
        contents.lines().any(|line| line.trim() == "*.log"),
        "Existing *.log entry should be preserved"
    );
    assert!(
        contents.lines().any(|line| line.trim() == ".DS_Store"),
        "Existing .DS_Store entry should be preserved"
    );
    assert!(
        contents.lines().any(|line| line.trim() == ".autosnap"),
        ".autosnap should be added"
    );
}

#[test]
fn test_init_handles_missing_git_info_dir() {
    let temp_dir = TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize a git repo
    Repository::init(repo_path).unwrap();

    // Remove info dir if it exists
    let info_dir = repo_path.join(".git").join("info");
    if info_dir.exists() {
        fs::remove_dir_all(&info_dir).unwrap();
    }

    // Run git autosnap init - should create info dir and exclude file
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(repo_path)
        .arg("init")
        .assert()
        .success();

    // Check that info dir and exclude file were created
    assert!(info_dir.exists(), ".git/info directory should be created");
    let exclude_path = info_dir.join("exclude");
    assert!(exclude_path.exists(), ".git/info/exclude should be created");

    let contents = fs::read_to_string(&exclude_path).unwrap();
    assert!(
        contents.lines().any(|line| line.trim() == ".autosnap"),
        ".autosnap should be in newly created exclude file"
    );
}

#[test]
fn test_init_handles_non_git_directory() {
    let temp_dir = TempDir::new().unwrap();
    let non_git_path = temp_dir.path();

    // Don't initialize a git repo - just try to run git autosnap init
    Command::cargo_bin("git-autosnap")
        .unwrap()
        .current_dir(non_git_path)
        .arg("init")
        .assert()
        .failure(); // Should fail because not in a git repo

    // Verify no .git/info/exclude was created
    let exclude_path = non_git_path.join(".git").join("info").join("exclude");
    assert!(
        !exclude_path.exists(),
        ".git/info/exclude should not be created in non-git directory"
    );
}
