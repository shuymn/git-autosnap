# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build and Development Commands

### Using Task (Preferred)
```bash
# Build
task build               # Debug build
task build-release       # Release build

# Test
task test-unit          # Unit tests only (fast)
task test-container     # Container-based integration tests (requires Docker)
task test               # Run all tests

# Lint and format
task fmt                # Format code
task clippy            # Run clippy linter
task lint              # Run all linters (fmt-check + clippy)

# Development
task run -- [ARGS]      # Run binary with arguments
task watch             # Watch for changes and rebuild
task install           # Install to system with hot-reload support
```

### Using Cargo Directly
```bash
# Build
cargo build
cargo build --release

# Run
cargo run -- [SUBCOMMAND]
./target/debug/git-autosnap [SUBCOMMAND]

# Format and lint
cargo fmt --all
cargo clippy --all-features --all-targets -- -D warnings

# Test
cargo test --all-features --verbose      # All tests
cargo test --lib                         # Unit tests only
cargo test --features container-tests    # Container tests (requires Docker image)
cargo test test_name                     # Specific test
cargo test -- --nocapture               # Tests with output

# Check
cargo check --all-features
cargo machete                           # Check for unused dependencies
cargo audit                             # Security audit

# Documentation
cargo doc --no-deps --all-features
```

## Testing Commands

### Container-Based Testing
```bash
# Build test image first
docker build -f Dockerfile.test -t git-autosnap-test:latest .

# Run container tests
cargo test --features container-tests

# Interactive shell in test container
task docker-shell
```

## Architecture Overview

### Core Module Structure

- **`src/cli.rs`**: Clap-based CLI definition with all subcommands (init, start, stop, status, once, gc, restore, diff, shell, uninstall, logs)
- **`src/gitlayer.rs`**: Core git operations using git2 library - manages the `.autosnap` bare repository, creates snapshots, handles restore/diff operations
- **`src/watcher.rs`**: File system watching using watchexec - monitors changes with debouncing, respects .gitignore, handles signal-based graceful shutdown
- **`src/daemon.rs`**: Background process management - daemonization, detaching from terminal
- **`src/process.rs`**: PID file management and single-instance locking using fs2
- **`src/config.rs`**: Git config integration for reading `autosnap.*` configuration values
- **`src/logs.rs`**: Log file management and viewing with follow mode support
- **`src/lib.rs`**: Entry point that routes CLI commands to appropriate modules

### Key Design Patterns

1. **Dual Repository Architecture**: Main git repo + hidden `.autosnap` bare repo for snapshots
2. **Single Instance Enforcement**: PID-based locking prevents multiple watchers
3. **Signal-Based Control**:
   - SIGTERM/SIGINT: Graceful shutdown with final snapshot
   - SIGUSR1: Force immediate snapshot
   - SIGUSR2: Hot-reload daemon with new binary
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

Example:
```bash
git config autosnap.debounce-ms 2000
git config autosnap.gc.prune-days 30
```

## Testing Strategy

### Test Organization
1. **Unit tests**: Pure functions without side effects (`cargo test --lib`)
2. **Integration tests**: Located in `tests/` directory
3. **Container tests**: Enabled with `--features container-tests`, run inside Docker
4. **Test helpers**: `tests/support/tc_exec.rs` provides container execution utilities

### Key Test Files
- `tests/cli_help.rs`: CLI argument validation
- `tests/daemon_lifecycle.rs`: Process management
- `tests/snapshot_gc.rs`: Garbage collection
- `tests/restore_command.rs`: File restoration
- `tests/diff_command.rs`: Diff operations
- `tests/signal_handling.rs`: Signal handling behavior
- `tests/watcher_module.rs`: File watching logic

## Critical Implementation Details

### PID File Locking (`src/process.rs`)
- Location: `.autosnap/autosnap.pid`
- Uses exclusive file locking (`fs2::FileExt::try_lock_exclusive`)
- Lock released on process exit ensures cleanup

### Graceful Shutdown (`src/watcher.rs`)
- Installs signal handlers for SIGTERM/SIGINT
- Takes final snapshot before exit
- Properly releases PID lock
- Flushes logs before termination

### Git Exclude Integration (`src/gitlayer.rs:42-96`)
- Automatically adds `.autosnap` to `.git/info/exclude`
- Prevents snapshot directory from appearing in `git status`
- Preserves existing exclude entries

### Interactive Selection (`src/gitlayer.rs`)
- Uses skim fuzzy finder for commit selection
- Available in shell, restore, and diff commands with `-i` flag

### Logging System (`src/logs.rs`)
- Logs written to `.autosnap/autosnap.log*` files
- Supports log rotation
- View with `git autosnap logs` command
- Follow mode available with `-f` flag

## Development Workflow

### Hot-Reload During Development
```bash
# Install with hot-reload support
task install

# The daemon will automatically restart when you install a new version
# This uses SIGUSR2 signal to trigger re-exec
```

### Debug Logging
```bash
# Enable verbose logging
RUST_LOG=git_autosnap=debug cargo run -- start -vv

# Or set environment variable
export RUST_LOG=git_autosnap=trace
```

### Pre-commit Hooks
The project uses lefthook for pre-commit hooks:
- Automatically formats code with `cargo fmt`
- Runs clippy linting
- Executes unit tests
- Performs cargo check

## Common Development Tasks

### Adding a New Subcommand
1. Add the command struct to `src/cli.rs`
2. Implement the handler in the appropriate module
3. Wire it up in `src/lib.rs::run()`
4. Add tests in `tests/` directory

### Working with Snapshots
```bash
# Create manual snapshot
git autosnap once -m "Before major refactor"

# View snapshots interactively
git autosnap shell -i

# Restore file from snapshot
git autosnap restore -i path/to/file

# Diff with snapshot
git autosnap diff -i
```

### Daemon Management
```bash
# Start watcher daemon
git autosnap start

# Check status
git autosnap status

# View logs
git autosnap logs -f  # Follow mode

# Stop daemon
git autosnap stop

# Force snapshot via signal
kill -USR1 $(cat .autosnap/autosnap.pid)
```