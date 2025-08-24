# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

```bash
# Build the project
cargo build
cargo build --release

# Run the binary
cargo run -- [SUBCOMMAND]
./target/debug/git-autosnap [SUBCOMMAND]

# Format code
cargo fmt

# Lint with clippy
cargo clippy --all-features --all-targets -- -D warnings

# Check compilation
cargo check --all-features

# Generate documentation
cargo doc --no-deps --all-features
```

## Testing Commands

```bash
# Run all tests
cargo test --all-features --verbose

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture

# Run only unit tests
cargo test --lib

# Run only integration tests  
cargo test --test '*'

# Run container-based tests (requires Docker)
docker build -f Dockerfile.test -t git-autosnap-test:latest .
cargo test --features container-tests

# Run ignored tests (stress tests)
cargo test -- --ignored

# Check for unused dependencies
cargo machete

# Security audit
cargo audit
```

## Architecture Overview

### Core Module Structure

The codebase is organized into distinct modules that handle specific responsibilities:

- **`src/cli.rs`**: Clap-based CLI definition with all subcommands (init, start, stop, status, once, gc, restore, diff, shell, uninstall)
- **`src/gitlayer.rs`**: Core git operations using git2 library - manages the `.autosnap` bare repository, creates snapshots, handles restore/diff operations
- **`src/watcher.rs`**: File system watching using watchexec - monitors changes with debouncing, respects .gitignore, handles signal-based graceful shutdown
- **`src/daemon.rs`**: Background process management - daemonization, detaching from terminal
- **`src/process.rs`**: PID file management and single-instance locking using fs2
- **`src/config.rs`**: Git config integration for reading `autosnap.*` configuration values
- **`src/lib.rs`**: Entry point that routes CLI commands to appropriate modules

### Key Design Patterns

1. **Dual Repository Architecture**: Main git repo + hidden `.autosnap` bare repo for snapshots
2. **Single Instance Enforcement**: PID-based locking prevents multiple watchers
3. **Signal-Based Shutdown**: SIGTERM/SIGINT triggers final snapshot before exit
4. **Debounced File Watching**: Configurable delay (default 1000ms) to batch rapid changes
5. **Git-Aware Ignoring**: Respects .gitignore patterns via watchexec-filterer-ignore

### Snapshot Storage Format

- Snapshots stored as commits in `.autosnap` bare repository
- Commit messages: `AUTOSNAP[branch] ISO8601_timestamp [optional_message]`
- Example: `AUTOSNAP[main] 2024-01-15T10:30:45Z File changes detected`
- Snapshots can be garbage collected based on age (default: 60 days)

### Configuration Hierarchy

Git config values (local → global → system precedence):
- `autosnap.debounce-ms`: Watcher debounce window (default: 1000)
- `autosnap.gc.prune-days`: Snapshot retention days (default: 60)

## Testing Strategy

The project uses container-based testing for isolation:

1. **Unit tests**: Pure functions without side effects
2. **Integration tests**: Run inside Docker containers via testcontainers
3. **Container tests**: Enabled with `--features container-tests`
4. **Test helper**: `tests/support/tc_exec.rs` provides container execution utilities

Key test files:
- `tests/cli_help.rs`: CLI argument validation
- `tests/daemon_lifecycle.rs`: Process management
- `tests/snapshot_gc.rs`: Garbage collection
- `tests/restore_command.rs`: File restoration
- `tests/diff_command.rs`: Diff operations

## Critical Implementation Details

### PID File Locking (src/process.rs)
- Location: `.autosnap/autosnap.pid`
- Uses exclusive file locking (fs2::FileExt::try_lock_exclusive)
- Lock released on process exit ensures cleanup

### Graceful Shutdown (src/watcher.rs)
- Installs signal handlers for SIGTERM/SIGINT
- Takes final snapshot before exit
- Properly releases PID lock

### Git Exclude Integration (src/gitlayer.rs:42-96)
- Automatically adds `.autosnap` to `.git/info/exclude`
- Prevents snapshot directory from appearing in `git status`
- Preserves existing exclude entries

### Interactive Selection (src/gitlayer.rs)
- Uses skim fuzzy finder for commit selection
- Available in shell, restore, and diff commands with `-i` flag
