# git-autosnap Testing Guidelines

## Overview

This document defines testing strategies for `git-autosnap` that ensure comprehensive coverage while protecting the host system from unintended side effects. All tests must be isolated, reproducible, and safe to run in any environment.

## Core Testing Principles

### 1. Isolation Requirements

- **No Host System Modification**: Tests must never modify the actual file system outside of designated temporary directories
- **No Real Git Repository Manipulation**: Tests must never affect existing git repositories on the host machine
- **Process Isolation**: Background processes spawned during tests must be properly contained and terminated
- **Signal Handler Safety**: Signal testing must not interfere with the test runner or other system processes
- **Container-First for Integration Tests**: Use testcontainers for complete isolation when testing git operations, file watching, and daemon processes

### 2. Container-Based Testing Strategy (Recommended)

For maximum isolation and safety, use testcontainers for integration tests (Rust 1.80 MSRV assumed):

```rust
use anyhow::Result;
use testcontainers::{clients::Cli, images::generic::GenericImage, Container};

fn setup_isolated_test_environment() -> Result<Container> {
    let docker = Cli::default();
    
    // Create container with git and rust toolchain
    let image = GenericImage::new("rust", "1.80-bookworm")
        .with_env_var("HOME", "/test-home")
        .with_env_var("GIT_CONFIG_NOSYSTEM", "1");
    
    let container = docker.run(image);
    
    // Install git-autosnap binary in container
    // Note: use the proper Exec API for your testcontainers version
    // e.g., ExecCommand in async runners; check exit status and capture stdout/stderr.
    // container.exec(ExecCommand::new("bash").with_args(["-lc", "cargo install --path ."]))?;
    
    Ok(container)
}

#[cfg(feature = "container-tests")]
#[test]
fn test_complete_workflow_in_container() {
    let container = setup_isolated_test_environment().unwrap();
    
    // All operations happen inside the container
    container.exec(vec!["git", "init", "/test-repo"]);
    container.exec(vec!["sh", "-c", "cd /test-repo && git autosnap init"]);
    container.exec(vec!["sh", "-c", "cd /test-repo && git autosnap start --daemon"]);
    
    // Verify behavior without any risk to host
    let output = container.exec(vec!["sh", "-c", "cd /test-repo && git autosnap status"]);
    assert!(output.status.success());
}
```

### 3. Fallback: Host-Based Testing with Temporary Directories

For environments where Docker is not available:

```rust
use tempfile::{tempdir, TempDir};
use git2::Repository;

fn setup_test_repo() -> Result<(TempDir, Repository)> {
    let temp_dir = tempdir()?;
    let repo = Repository::init(temp_dir.path())?;
    
    // Configure git user for commits
    let mut config = repo.config()?;
    config.set_str("user.name", "Test User")?;
    config.set_str("user.email", "test@example.com")?;
    
    Ok((temp_dir, repo))
}
```

### 4. Test Categories

#### Unit Tests (`src/*/mod.rs`)

- Test pure functions with no side effects
- Test data transformations and business logic
- **NO MOCKS** - if it needs mocking, use containers instead
- Example: Parsing commit messages, calculating timestamps

#### Integration Tests (`tests/`) - Container-Based

- Test real behavior in isolated containers
- Use actual git operations, file systems, and processes
- Verify end-to-end functionality with real dependencies
- Container cleanup is automatic

#### Edge Case Tests (`tests/edge_cases/`)

- Test error conditions with real scenarios
- Corrupt git repositories, permission issues, signal handling
- All tested in containers for safety
- No mocked failures - create real failure conditions

## Container-Based Integration Testing

### Why Containers for git-autosnap?

Testing git-autosnap involves several "scary" operations that could affect the host system:

- Git configuration changes (global/system config)
- Long-running daemon processes
- Signal handling (SIGTERM, SIGINT, SIGUSR1/2)
- File system watchers with recursive monitoring
- PID file management and process control

Containers provide **complete isolation** from the host system, making these tests 100% safe.

### Container Sharing Strategy

#### Benefits of Shared Containers

- **Speed**: Container startup time (~5-10s) amortized across all tests
- **Resource Efficiency**: Single container uses less memory/CPU
- **Image Caching**: Build image once, reuse for all tests
- **Parallel Testing**: Multiple tests run simultaneously in isolated workspaces

#### When to Use Each Strategy

| Strategy | Use Case | Performance | Isolation |
|----------|----------|-------------|-----------|
| **Shared** | Most tests, file operations, git commands | Fast (ms per test) | Workspace-level |
| **Isolated** | Daemon tests, global config changes, signals | Slower (5-10s per test) | Complete |
| **Hybrid** | Mix based on test requirements | Optimal | As needed |

#### Guidelines for Shared Containers

1. **Always use isolated workspaces** - Each test gets `/test-workspace-{id}`
2. **Automatic cleanup via RAII** - TestWorkspace uses Drop trait for guaranteed cleanup
3. **Avoid global state changes** - No system-wide config modifications
4. **Use atomic operations** - Prevent race conditions between tests
5. **Panic-safe cleanup** - Resources cleaned up even if tests panic or fail

### Container Test Setup

#### Shared Container Strategy (Recommended for Speed)

```rust
use anyhow::Result;
use testcontainers::{clients::Cli, core::WaitFor, images::generic::GenericImage};
use std::sync::{Arc, LazyLock, Weak, atomic::{AtomicUsize, Ordering}};
use tokio::sync::Mutex;

// Shared container instance for all tests
static SHARED_CONTAINER: LazyLock<Arc<Mutex<Option<GitAutosnapTestContainer>>>> =
    LazyLock::new(|| Arc::new(Mutex::new(None)));

struct GitAutosnapTestContainer {
    container: Container<'static, GenericImage>,
    test_counter: AtomicUsize,
}

impl GitAutosnapTestContainer {
    async fn get_or_create() -> Result<Arc<Self>> {
        let mut container_lock = SHARED_CONTAINER.lock().await;
        
        if let Some(ref container) = *container_lock {
            return Ok(container.clone());
        }
        
        // Create new shared container from a prebuilt image.
        // Build 'git-autosnap-test:latest' in CI before tests to avoid inline Dockerfile builds.
        let cli = Cli::default();
        let image = GenericImage::new("git-autosnap-test", "latest")
            .with_wait_for(WaitFor::message_on_stdout("ready"));
        
        let container = cli.run(image);
        let shared = Arc::new(Self { 
            container, 
            test_counter: AtomicUsize::new(0),
        });
        
        *container_lock = Some(shared.clone());
        Ok(shared)
    }
    
    // Create isolated workspace for each test with automatic cleanup
    async fn create_test_workspace(&self) -> Result<TestWorkspace> {
        let test_id = self.test_counter.fetch_add(1, Ordering::SeqCst);
        let workspace_path = format!("/test-workspace-{}", test_id);
        
        self.exec(vec!["mkdir", "-p", &workspace_path]).await?;
        self.exec(vec!["git", "init", &workspace_path]).await?;
        
        Ok(TestWorkspace {
            path: workspace_path,
            container: Arc::downgrade(&self),
        })
    }
    
    async fn exec(&self, cmd: Vec<&str>) -> Result<String> {
        // Delegate to the reusable helper shown below
        // Prefer exec_in_async for a per-cwd command; here we run as-is
        let script = cmd.join(" ");
        exec_bash_async(&self.container, &script).await
    }
}

// RAII wrapper for automatic workspace cleanup
struct TestWorkspace {
    path: String,
    container: Weak<GitAutosnapTestContainer>,
}

impl TestWorkspace {
    fn path(&self) -> &str {
        &self.path
    }
}

impl Drop for TestWorkspace {
    fn drop(&mut self) {
        // Best-effort cleanup on drop without assuming an existing async runtime
        if let Some(container) = self.container.upgrade() {
            let path = self.path.clone();
            std::thread::spawn(move || {
                if let Ok(rt) = tokio::runtime::Runtime::new() {
                    let _ = rt.block_on(container.exec(vec![
                        "sh",
                        "-c",
                        &format!("rm -rf {}", path),
                    ]));
                }
            });
        }
    }
}
```

#### Per-Test Container Strategy (Maximum Isolation)

```rust
struct IsolatedTestContainer {
    container: Container<'static, GenericImage>,
}

impl IsolatedTestContainer {
    async fn new() -> Result<Self> {
        let cli = Cli::default();
        // Use a pre-built image (build in CI ahead of tests)
        let image = GenericImage::new("git-autosnap-test", "latest");
        let container = cli.run(image);
        Ok(Self { container })
    }
}
```

#### Hybrid Strategy (Best of Both)

```rust
// Use shared containers for fast, read-only tests
// Use isolated containers for tests that modify global state

#[derive(Clone)]
enum ContainerStrategy {
    Shared,      // Share container, isolated workspace
    Isolated,    // New container per test
}

async fn get_test_container(strategy: ContainerStrategy) -> Result<TestEnvironment> {
    match strategy {
        ContainerStrategy::Shared => {
            let container = GitAutosnapTestContainer::get_or_create().await?;
            let workspace = container.create_test_workspace().await?;
            Ok(TestEnvironment::Shared { container, workspace })
        }
        ContainerStrategy::Isolated => {
            let container = IsolatedTestContainer::new().await?;
            Ok(TestEnvironment::Isolated { container })
        }
    }
}
```

#### Reusable Exec Helper (Drop‑in)

Choose one variant based on your testcontainers runner. Both are Rust 1.80 compatible and attach useful error context per the style guide.

Async runner variant (uses ExecCommand; feature-gate or place in a module used only by async tests):

```rust
// Cargo.toml (dev-dependencies)
// anyhow = "1"
// shell-escape = "0.1"
// testcontainers = "<your-version>"  # with async support

use anyhow::{bail, Context, Result};
use shell_escape::unix::escape;
use testcontainers::{core::ExecCommand, Container, Image};

pub async fn exec_bash_async<I: Image>(c: &Container<'_, I>, cmd: &str) -> Result<String> {
    let out = c
        .exec(ExecCommand {
            cmd: vec!["bash".into(), "-lc".into(), cmd.into()],
            ..Default::default()
        })
        .await
        .context("container exec failed")?;

    if out.exit_code != 0 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("command failed (code {}): {}", out.exit_code, stderr);
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

pub async fn exec_in_async<I: Image>(c: &Container<'_, I>, cwd: &str, cmd: &str) -> Result<String> {
    let script = format!("cd {} && {}", escape(cwd.into()), cmd);
    exec_bash_async(c, &script).await
}
```

Blocking runner variant (classic `clients::Cli`):

```rust
// Cargo.toml (dev-dependencies)
// anyhow = "1"
// shell-escape = "0.1"
// testcontainers = "<your-version>"

use anyhow::{bail, Context, Result};
use shell_escape::unix::escape;
use testcontainers::{Container, Image};

pub fn exec_bash<I: Image>(c: &Container<'_, I>, cmd: &str) -> Result<String> {
    // Most versions expose a blocking exec that returns an output struct
    let out = c.exec(vec!["bash", "-lc", cmd]);

    // Adjust field names if your version differs
    if out.exit_code != 0 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("command failed (code {}): {}", out.exit_code, stderr);
    }
    Ok(String::from_utf8(out.stdout).context("invalid utf8 on stdout")?)
}

pub fn exec_in<I: Image>(c: &Container<'_, I>, cwd: &str, cmd: &str) -> Result<String> {
    let script = format!("cd {} && {}", escape(cwd.into()), cmd);
    exec_bash(c, &script)
}
```

Usage inside your wrapper (delegates to the helper and adds quoting by using `exec_in_*`):

```rust
impl GitAutosnapTestContainer {
    #[allow(dead_code)]
    pub async fn exec(&self, cmd: &str) -> anyhow::Result<String> {
        exec_bash_async(&self.container, cmd).await
    }

    #[allow(dead_code)]
    pub async fn exec_in(&self, cwd: &str, cmd: &str) -> anyhow::Result<String> {
        exec_in_async(&self.container, cwd, cmd).await
    }
}
```

### Testing Dangerous Operations Safely

```rust
#[cfg(feature = "container-tests")]
#[tokio::test(flavor = "multi_thread", timeout = 5)]
async fn test_local_git_config_modification() -> Result<()> {
    let container = GitAutosnapTestContainer::get_or_create().await?;
    let workspace = container.create_test_workspace().await?;
    
    // Workspace will be cleaned up automatically even if test fails
    container.exec(vec![
        "sh", "-c",
        &format!("cd {} && git config --local autosnap.debounce-ms 500", workspace.path())
    ]).await?;
    
    let output = container.exec(vec![
        "sh", "-c",
        &format!("cd {} && git config --get autosnap.debounce-ms", workspace.path())
    ]).await?;
    assert_eq!(output.trim(), "500");
    
    Ok(())
    // No manual cleanup needed - Drop trait handles it
}

#[cfg(feature = "container-tests")]
#[tokio::test(flavor = "multi_thread", timeout = 10)]
async fn test_daemon_process_lifecycle() -> Result<()> {
    let test_env = get_test_container(ContainerStrategy::Isolated).await?;
    
    test_env.exec(vec!["git", "autosnap", "start", "--daemon"]).await?;
    
    let status = test_env.exec(vec!["git", "autosnap", "status"]).await?;
    assert!(status.contains("running"));

    // Send signal to the specific process recorded in the pidfile
    let pid = test_env.exec(vec!["sh", "-c", "cat .autosnap/autosnap.pid"]).await?;
    let pid = pid.trim();
    test_env.exec(vec!["kill", "-USR1", pid]).await?;
    
    test_env.exec(vec!["git", "autosnap", "stop"]).await?;
    
    Ok(())
}

#[cfg(feature = "container-tests")]
#[tokio::test(flavor = "multi_thread", timeout = 20)]
async fn test_concurrent_snapshot_operations() -> Result<()> {
    let container = GitAutosnapTestContainer::get_or_create().await?;
    
    // Run multiple test scenarios in parallel, each in its own workspace
    let tasks: Vec<_> = (0..10).map(|i| {
        let container = container.clone();
        tokio::spawn(async move {
            let workspace = container.create_test_workspace().await?;
            // Workspace auto-cleanup even if any operation fails
            
            container.exec(vec![
                "sh", "-c",
                &format!("cd {} && git autosnap init", workspace.path())
            ]).await?;
            
            container.exec(vec![
                "sh", "-c",
                &format!("cd {} && echo 'test {}' > file.txt", workspace.path(), i)
            ]).await?;
            
            container.exec(vec![
                "sh", "-c",
                &format!("cd {} && git autosnap once", workspace.path())
            ]).await?;
            
            // Verify snapshot created
            let log = container.exec(vec![
                "sh", "-c",
                &format!("cd {} && git --git-dir=.autosnap log --oneline", workspace.path())
            ]).await?;
            
            assert!(log.contains("AUTOSNAP"));
            Ok::<(), anyhow::Error>(())
            // Workspace cleaned up automatically when it goes out of scope
        })
    }).collect();
    
    // Wait for all tests to complete
    for task in tasks {
        task.await??;
    }
    
    Ok(())
}
```

## Component-Specific Testing

### 1. Watcher Component

```rust
#[cfg(feature = "container-tests")]
#[tokio::test(flavor = "multi_thread", timeout = 10)]
async fn test_file_watcher_debounce() -> Result<()> {
    use anyhow::Result;
    let container = GitAutosnapTestContainer::get_or_create().await?;
    
    // Create real repository
    container.exec(vec!["git", "init", "/test-repo"]).await?;
    container.exec(vec!["sh", "-c", "cd /test-repo && git autosnap init"]).await?;
    
    // Start real watcher
    container.exec(vec!["sh", "-c", "cd /test-repo && git autosnap start --daemon"]).await?;
    
    // Create rapid file changes
    use std::time::Duration;
    for i in 0..5 {
        container.exec(vec![
            "sh", "-c",
            &format!("echo 'change {}' > /test-repo/file.txt", i)
        ]).await?;
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    
    // Wait for debounce window
    tokio::time::sleep(Duration::from_millis(300)).await;
    
    // Verify only one snapshot was created (debouncing worked)
    let log = container.exec(vec![
        "git", "--git-dir=/test-repo/.autosnap", "log", "--oneline"
    ]).await?;
    
    let snapshot_count = log.lines().filter(|l| l.contains("AUTOSNAP")).count();
    assert_eq!(snapshot_count, 1, "Debouncing should create only one snapshot");
    
    Ok(())
}
```

**Test Cases:**

- Debounce timing with file events
- .gitignore file parsing and respect
- Recursive directory watching behavior
- Exclusion of `.git/` and `.autosnap/` directories

### 2. Git Operations

```rust
#[cfg(feature = "container-tests")]
#[tokio::test(flavor = "multi_thread", timeout = 10)]
async fn test_git_snapshot_operations() -> Result<()> {
    use anyhow::Result;
    let container = GitAutosnapTestContainer::get_or_create().await?;
    
    // Use real git commands
    container.exec(vec!["git", "init", "/repo"]).await?;
    container.exec(vec!["sh", "-c", "cd /repo && git autosnap init"]).await?;
    
    // Create real files
    container.exec(vec!["sh", "-c", "echo 'content' > /repo/file.txt"]).await?;
    container.exec(vec!["sh", "-c", "mkdir -p /repo/src && echo 'code' > /repo/src/main.rs"]).await?;
    
    // Take real snapshot
    container.exec(vec!["sh", "-c", "cd /repo && git autosnap once"]).await?;
    
    // Verify with real git commands
    let log = container.exec(vec![
        "git", "--git-dir=/repo/.autosnap", "log", "--format=%s"
    ]).await?;
    
    // Check real commit message format
    assert!(log.contains("AUTOSNAP["), "Missing AUTOSNAP prefix");
    assert!(log.contains("T"), "Missing ISO8601 timestamp");
    
    // Verify real file contents in snapshot
    let files = container.exec(vec![
        "git", "--git-dir=/repo/.autosnap", "ls-tree", "-r", "HEAD"
    ]).await?;
    
    assert!(files.contains("file.txt"));
    assert!(files.contains("src/main.rs"));
    
    Ok(())
}
```

**Test Cases:**

- Bare repository creation and configuration
- Commit creation with git2
- File tracking and index operations
- Tree objects and blob storage

### 3. CLI Commands

```rust
use anyhow::Result;
use assert_cmd::Command;
use predicates::str::contains;
use tempfile::tempdir;

#[test]
fn test_init_command() -> Result<()> {
    let temp_dir = tempdir()?;
    
    Command::new("git")
        .current_dir(&temp_dir)
        .args(["autosnap", "init"])
        .assert()
        .success()
        .stdout(contains("Initialized .autosnap"));
    
    // Verify .autosnap directory exists
    assert!(temp_dir.path().join(".autosnap").exists());
    
    Ok(())
}
```

**Command Coverage Matrix:**

| Command | Test Cases |
|---------|------------|
| `init` | - Fresh initialization<br>- Reinit existing<br>- Invalid permissions |
| `start` | - Foreground mode<br>- Daemon mode<br>- Already running |
| `stop` | - Running process<br>- No process<br>- Permission denied |
| `status` | - Running state<br>- Stopped state<br>- PID file corruption |
| `once` | - Single snapshot<br>- Empty working tree<br>- Large file set |
| `gc` | - Age-based pruning<br>- Empty history<br>- Custom retention |
| `uninstall` | - Clean removal<br>- Running process<br>- Missing directory |

### 4. Process Control

```rust
#[test]
fn test_pid_file_locking() -> Result<()> {
    use anyhow::Result;
    use std::fs::OpenOptions;
    use fs2::FileExt;
    use tempfile::tempdir;

    let temp_dir = tempdir()?;
    let pid_file = temp_dir.path().join("autosnap.pid");
    
    // Test exclusive lock acquisition
    let file1 = OpenOptions::new()
        .create(true)
        .write(true)
        .open(&pid_file)?;
    
    file1.try_lock_exclusive()?;
    
    // Second lock should fail
    let file2 = OpenOptions::new()
        .write(true)
        .open(&pid_file)?;
    
    assert!(file2.try_lock_exclusive().is_err());
    
    Ok(())
}
```

**Key Test Cases:**

- PID file creation and locking
- Single-instance enforcement
- Signal handling (mocked)
- Daemon detachment (in subprocess)

### 5. Configuration

**CRITICAL: Git config operations require special isolation**

Git config has three levels that can affect the host system:

- **System** (`/etc/gitconfig`) - Requires root access
- **Global** (`~/.gitconfig`) - Affects user's git configuration
- **Local** (`.git/config`) - Repository-specific, safe in temp repos

```rust
#[test]
fn test_config_isolation() {
    use tempfile::tempdir;
    use git2::Repository;
    let temp_dir = tempdir().unwrap();
    let temp_home = tempdir().unwrap();
    
    // Override HOME to isolate global config
    std::env::set_var("HOME", temp_home.path());
    std::env::set_var("XDG_CONFIG_HOME", temp_home.path().join(".config"));
    
    // Create isolated git config environment
    let repo = Repository::init(temp_dir.path()).unwrap();
    let mut config = repo.config().unwrap();
    
    // Safe: Only affects the temporary repository's local config
    config.set_str("autosnap.debounce-ms", "300").unwrap();
    
    // For testing global config behavior, use the isolated HOME
    let global_config_path = temp_home.path().join(".gitconfig");
    std::fs::write(&global_config_path, "[autosnap]\n\tgc.prune-days = 45\n").unwrap();
    
    // Test precedence in isolated environment
    // local > global (isolated) > system (avoid modifying)
}

// Alternative: Use git2 config API with custom paths
#[test]
fn test_config_with_custom_paths() {
    use git2::Config;
    use tempfile::tempdir;
    
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join("test.gitconfig");
    
    // Create isolated config file
    let mut config = Config::open(&config_path).unwrap();
    config.set_str("autosnap.debounce-ms", "500").unwrap();
    
    // Read from isolated config
    let value = config.get_string("autosnap.debounce-ms").unwrap();
    assert_eq!(value, "500");
}

// For integration tests that need real git config behavior
#[test]
fn test_config_integration() {
    use assert_cmd::prelude::*;
    use assert_cmd::Command as AssertCommand;
    use tempfile::tempdir;
    // Use subprocess isolation
    let temp_dir = tempdir().unwrap();
    let temp_home = tempdir().unwrap();
    
    AssertCommand::new("git")
        .env("HOME", temp_home.path())
        .env("GIT_CONFIG_NOSYSTEM", "1") // Disable system config
        .current_dir(&temp_dir)
        .args(["config", "--global", "autosnap.gc.prune-days", "30"])
        .assert()
        .success();
}
```

**Config Testing Strategy:**

1. **Never modify real git config**: Always use isolation techniques
2. **Override environment variables**: Set `HOME`, `XDG_CONFIG_HOME`, `GIT_CONFIG_NOSYSTEM`
3. **Use temporary config files**: Create isolated `.gitconfig` files in temp directories
4. **Subprocess isolation**: Run git commands with modified environment
5. **Avoid mocks**: Prefer isolated temp configs and env vars; avoid mocks

## Testing Philosophy: Real Over Mocked

### Why We Avoid Mocks

1. **Mocks test implementation, not behavior**: They verify your code calls the right methods, not that it actually works
2. **False confidence**: Mocks can pass even when real integration would fail
3. **Maintenance burden**: Mocks need updating whenever implementation changes
4. **Self-serving tests**: Testing that mock was called correctly proves nothing about actual functionality

### Container-Based Real Testing

Instead of mocking, we test real behavior in isolated containers:

```rust
// ❌ BAD: Mock-based test (self-serving)
#[test]
fn test_with_mock() {
    let mut mock_fs = MockFileSystem::new();
    mock_fs.expect_write_file()
        .with(eq("/path/to/file"), eq(b"content"))
        .times(1)
        .returning(|_, _| Ok(()));
    
    // This only tests that we called the mock correctly
    my_function(&mock_fs);
}

// ✅ GOOD: Container-based real test
#[tokio::test]
async fn test_with_real_behavior() {
    let container = GitAutosnapTestContainer::get_or_create().await.unwrap();
    
    // Real file operations in container
    container.exec(vec!["git", "autosnap", "init"]).await.unwrap();
    
    // Verify real results
    let exists = container.exec(vec!["test", "-d", ".autosnap"]).await;
    assert!(exists.is_ok(), "Directory was actually created");
}
```

### When Pure Functions Don't Need Containers

Only truly pure functions should be tested without containers:

```rust
// Pure function - no side effects, no I/O
fn parse_commit_message(msg: &str) -> Result<(String, DateTime<Utc>)> {
    // Parsing logic only
}

#[test]
fn test_parse_commit_message() {
    let result = parse_commit_message("AUTOSNAP[main] 2024-01-15T10:30:00Z");
    assert_eq!(result.unwrap().0, "main");
}
```

## Test Execution Strategy

### Local Development

```bash
# Run all tests with output
cargo test -- --nocapture

# Run only unit tests
cargo test --lib

# Run only integration tests
cargo test --test '*'

# Run with coverage
cargo tarpaulin --out Html
```

### CI/CD Pipeline

#### Container-Based Testing (Recommended)

```yaml
test:
  stage: test
  services:
    - docker:dind
  script:
    # Run all tests in containers
    - cargo test --features container-tests --verbose
    
    # No cleanup needed - containers are ephemeral
```

#### Fallback: Environment Variable Isolation

```yaml
test-no-docker:
  stage: test
  script:
    # Create isolated test environment
    - export TMPDIR=$(mktemp -d)
    - export HOME=$TMPDIR
    - export GIT_CONFIG_NOSYSTEM=1
    
    # Run tests with strict isolation
    - cargo test --all-features --verbose
    
    # Cleanup
    - rm -rf $TMPDIR
```

## Safety Checklist

Before running tests, verify:

### Container-Based Tests (Safest)

- [ ] All integration tests run inside testcontainers
- [ ] Container images include necessary dependencies (git, rust, inotify-tools)
- [ ] No host filesystem mounts except read-only source code
- [ ] Containers are ephemeral and destroyed after tests

### Host-Based Tests (Use with Caution)

- [ ] All file operations use `tempfile` or `tempdir`
- [ ] No hardcoded paths to real directories
- [ ] Git operations target only test repositories
- [ ] **Git config tests use HOME/XDG_CONFIG_HOME isolation**
- [ ] **Never modify ~/.gitconfig or /etc/gitconfig**
- [ ] **GIT_CONFIG_NOSYSTEM=1 set for subprocess tests**
- [ ] Process spawning is properly mocked or isolated
- [ ] Signal handlers don't affect test runner
- [ ] All resources cleaned up in test teardown
- [ ] No network operations performed
- [ ] PID files use test-specific locations

## Performance Testing

### Stress Tests

```rust
#[test]
#[ignore] // Run with: cargo test -- --ignored
fn test_large_repository() {
    let temp_dir = setup_test_repo().unwrap();
    
    // Generate 10,000 files
    for i in 0..10_000 {
        let path = temp_dir.path().join(format!("file_{}.txt", i));
        std::fs::write(&path, format!("content {}", i)).unwrap();
    }
    
    // Test snapshot performance
    // Verify memory usage stays reasonable
    // Check debouncing under load
}
```

### Benchmarks

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_snapshot_creation(c: &mut Criterion) {
    let (temp_dir, repo) = setup_test_repo().unwrap();
    
    c.bench_function("create_snapshot", |b| {
        b.iter(|| {
            create_snapshot(black_box(&repo))
        });
    });
}

criterion_group!(benches, benchmark_snapshot_creation);
criterion_main!(benches);
```

## Error Injection Testing

Test resilience to failures:

```rust
#[test]
fn test_corrupted_pid_file() {
    let temp_dir = tempdir().unwrap();
    let pid_file = temp_dir.path().join("autosnap.pid");
    
    // Write invalid content
    std::fs::write(&pid_file, "not_a_number").unwrap();
    
    // Verify graceful handling
    let result = read_pid(&pid_file);
    assert!(result.is_err());
    assert!(matches!(result.unwrap_err().kind(), ErrorKind::InvalidData));
}
```

## Test Data Management

### Fixtures

```
tests/fixtures/
├── small_repo/     # Minimal test repository
├── large_repo/     # Performance testing
├── corrupted_repo/ # Error handling tests
└── config_samples/ # Configuration test cases
```

### Test Helpers

```rust
// tests/common/mod.rs
pub mod helpers {
    pub fn create_test_files(dir: &Path, count: usize) { ... }
    pub fn corrupt_git_index(repo: &Repository) { ... }
    pub fn simulate_file_changes(dir: &Path, interval: Duration) { ... }
}
```

## Continuous Testing

### Watch Mode

```bash
# Auto-run tests on file changes
cargo watch -x test
```

### Mutation Testing

```bash
# Verify test quality with mutation testing
cargo mutants
```

## Documentation Tests

Ensure all code examples in documentation are tested:

```rust
/// Creates a snapshot of the current working tree
/// 
/// # Example
/// ```no_run
/// use git_autosnap::snapshot;
/// use tempfile::tempdir;
/// 
/// let temp = tempdir().unwrap();
/// let result = snapshot::create(&temp.path());
/// assert!(result.is_ok());
/// ```
pub fn create(path: &Path) -> Result<()> { ... }
```

## Test Maintenance

### Regular Audits

- Review test coverage monthly
- Update mocks when dependencies change
- Prune obsolete test cases
- Verify isolation mechanisms still work

### Test Database

Maintain a test case database:

```toml
# tests/test_cases.toml
[[test_case]]
id = "TC001"
component = "watcher"
description = "Verify debounce window"
priority = "high"
last_updated = "2024-01-15"
```

## Troubleshooting

### Common Issues

**Problem**: Tests fail with "Permission denied"
**Solution**: Ensure temp directories have proper permissions (0755)

**Problem**: Tests hang indefinitely
**Solution**: Add timeout annotations: `#[tokio::test(timeout = 10)]`

**Problem**: Flaky tests in CI
**PROHIBITED SOLUTIONS**:

- ❌ Increasing debounce windows or timeouts
- ❌ Adding retry logic
- ❌ Using `serial_test` to avoid race conditions

**REQUIRED ACTIONS**:

1. **Identify root cause**: Run test with `--nocapture` and logging enabled
2. **Document the issue**: Create detailed bug report with:
   - Test name and location
   - Failure frequency (e.g., "fails 3/10 times")
   - Environment where it fails (CI/local)
   - Full error output
3. **Report to team**: Flaky tests indicate design problems that need fixing
4. **Mark as flaky**: Use `#[ignore]` with comment explaining the issue

   ```rust
   #[test]
   #[ignore] // FLAKY: Race condition in file watcher - see issue #123
   fn test_file_watcher_debounce() {
       // Test implementation
   }
   ```

**Problem**: Resource leaks between tests
**PROHIBITED SOLUTION**:

- ❌ Using `serial_test` to serialize test execution

**REQUIRED ACTIONS**:

1. **Fix the leak**: Tests must properly clean up their resources
2. **Use RAII patterns**: Ensure cleanup happens automatically
3. **Verify isolation**: Each test must be completely independent
4. **If truly necessary**: Document WHY tests can't run in parallel and get team approval

### Flaky Test Policy

```rust
// tests/flaky_test_guard.rs
#[cfg(test)]
mod flaky_guard {
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    static FLAKY_TEST_COUNT: AtomicUsize = AtomicUsize::new(0);
    
    pub fn report_flaky_test(test_name: &str, reason: &str) {
        FLAKY_TEST_COUNT.fetch_add(1, Ordering::SeqCst);
        eprintln!("⚠️  FLAKY TEST DETECTED: {}", test_name);
        eprintln!("   Reason: {}", reason);
        eprintln!("   Action Required: Fix root cause, do not extend timeouts");
        
        // In CI, fail the build if flaky tests are detected
        if std::env::var("CI").is_ok() {
            panic!("Flaky tests are not allowed in CI");
        }
    }
}

// Usage in tests
#[test]
fn test_watcher_timing() {
    if test_sometimes_fails() {
        flaky_guard::report_flaky_test(
            "test_watcher_timing",
            "Debounce window race condition"
        );
        panic!("Test is flaky - needs investigation");
    }
}
```

### Timeout Policy

Timeouts should be deterministic and minimal:

```rust
// GOOD: Explicit, minimal timeout (seconds)
#[tokio::test(timeout = 1)]
async fn test_quick_operation() {
    // Should complete in milliseconds
}

// BAD: Extended timeout to "fix" flaky test
#[tokio::test(timeout = 60)]  // ❌ PROHIBITED
async fn test_that_sometimes_hangs() {
    // Hiding a race condition
}
```

### Required Test Attributes

All timing-sensitive tests must include:

```rust
#[test]
fn test_with_timing_requirements() {
    // Document timing assumptions
    // TIMING: Assumes file system events arrive within 100ms
    // DEPENDENCY: Requires inotify support on Linux
    // ISOLATION: Must not run concurrent file operations
    
    // Test implementation
}
```

## Appendix: Required Test Dependencies

Install test dependencies using `cargo add` to ensure you get the latest compatible versions:

### Container-Based Testing (Recommended)

```bash
# Essential for safe integration testing
cargo add --dev testcontainers
cargo add --dev bollard  # Docker API client
# Note: Uses std::sync::LazyLock (Rust 1.80+) for shared containers
```

### Core Testing Libraries

```bash
# Fundamental test utilities
cargo add --dev tempfile      # Safe temporary file/directory creation
cargo add --dev assert_cmd    # Command execution assertions
cargo add --dev predicates    # Flexible assertions
cargo add --dev fs2           # File locking (pid file tests)
```

### Async Testing

```bash
# Tokio test utilities
cargo add --dev tokio-test
```

### Property-Based Testing

```bash
# For exhaustive edge case testing
cargo add --dev proptest
cargo add --dev quickcheck
```

### Benchmarking

```bash
# Performance measurement
cargo add --dev criterion
```

### Test Organization

```bash
# WARNING: Avoid using serial_test - see Flaky Test Policy
# cargo add --dev serial_test  # ❌ PROHIBITED except with team approval
cargo add --dev test-case      # Parameterized tests
```

### Code Coverage

```bash
# Install as a cargo subcommand (not a dependency)
cargo install cargo-tarpaulin
```

### Complete Setup Script

```bash
#!/bin/bash
# Run this script to set up all test dependencies

# Container testing (recommended)
cargo add --dev testcontainers bollard

# Core testing
cargo add --dev tempfile assert_cmd predicates fs2

# Async and property testing
cargo add --dev tokio-test proptest quickcheck

# Benchmarking and test organization
cargo add --dev criterion test-case

# Coverage tool
cargo install cargo-tarpaulin

echo "✅ Test dependencies installed successfully"
```
