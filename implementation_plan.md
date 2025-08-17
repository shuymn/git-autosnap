# git-autosnap — Implementation Plan

This plan translates the docs in `docs/spec.md`, `docs/testing.md`, and `docs/style.md` into concrete implementation steps for the Rust CLI binary `git-autosnap`.

**Last Updated**: 2025-08-17

## Current Implementation Status

✅ **Core Functionality Complete**: All planned commands are fully implemented and operational.

### Completed Features
- ✅ All CLI commands (init, start, stop, status, once, gc, uninstall)
- ✅ Additional `shell` command for interactive snapshot exploration with skim
- ✅ Comprehensive signal handling (SIGTERM, SIGINT, SIGHUP, SIGUSR1, SIGUSR2)
- ✅ Graceful daemon update during installation via SIGUSR2
- ✅ Self-restart on ignore file changes (automatic filter refresh)
- ✅ Hot-reload binary updates with PID preservation via exec()
- ✅ ISO8601 timestamps with timezone offset support using `time` crate
- ✅ Git config integration for `autosnap.debounce-ms` and `autosnap.gc.prune-days`
- ✅ Process control with PID file locking using `fs2`
- ✅ Watchexec-based file watcher with .gitignore support
- ✅ Automatic .autosnap exclusion in .git/info/exclude
- ✅ Bare repository snapshots with proper commit messages
- ✅ Container-based testing infrastructure

## Scope and Goals

- Implement a Git extension that records timestamped snapshots of a working tree into a local bare repository at `.autosnap/`.
- Provide a long‑running watcher process with start/stop/status controls and a single-instance lock via a PID file.
- Respect `.gitignore`, avoid touching the main repo history, and keep all data local to the repo.
- Follow the coding standards in `docs/style.md` and the testing strategy in `docs/testing.md`.

## Recent Enhancements (2025-08-17)

### GC Command Alignment with Git
The `gc` command has been updated to match Git's semantics:
- `git autosnap gc` - Now only compresses/packs objects (no data loss)
- `git autosnap gc --prune` - Compresses and prunes old snapshots (previous behavior)
- This provides clearer, more intuitive behavior aligned with standard Git

### Signal Handling Implementation
The daemon now supports comprehensive signal handling for improved operations:

1. **SIGTERM/SIGINT**: Graceful shutdown with final snapshot creation
2. **SIGHUP**: Reserved for future configuration reload functionality  
3. **SIGUSR1**: Force immediate snapshot on demand (`kill -USR1 $(cat .autosnap/autosnap.pid)`)
4. **SIGUSR2**: Hot-reload binary update - polls for binary change and performs exec() to maintain PID

### Diff Command Implementation (2025-08-18)
The `diff` command provides comprehensive comparison between snapshots and working tree:

**Features**:
- Multiple comparison modes: working tree vs HEAD, commit vs working tree, commit vs commit
- Interactive commit selection with `-i` flag using skim fuzzy finder
- Multiple output formats: unified diff (default), stats, name-only, name-status
- Path filtering for targeted comparisons
- Enhanced terminal output with colored diff using console crate
- Proper exclusion of .git and .autosnap directories from working tree comparisons

**Implementation**:
- Uses libgit2's diff APIs for efficient tree comparison
- Supports all standard git diff output formats
- Terminal-friendly colored output for better readability
- Integrates with existing interactive commit selection infrastructure

### Restore Command Implementation (2025-08-18)
The `restore` command provides safe recovery from snapshots:

**Features**:
- Safe-by-default: refuses to overwrite uncommitted changes unless `--force`
- Dry-run mode: preview changes with `--dry-run`
- Full restore: `--full` removes files not in snapshot (preserves .git/.autosnap)
- Partial restore: specify paths to restore specific files/directories
- Interactive selection: `-i` flag for fuzzy-finding commits with skim

**Implementation**:
- Uses libgit2's `checkout_tree` for efficient file restoration
- Manual handling of file removal in full mode to protect critical directories
- Updates main repository index after successful restore
- Comprehensive safety checks and user feedback

### Install Task Enhancement
The `task install` command now:
- Performs atomic binary replacement (copy to .new, then rename)
- Detects running daemons before installation
- Sends SIGUSR2 to trigger automatic hot-reload
- Daemon automatically restarts with new binary while maintaining same PID
- Zero downtime updates for running daemons

### Ignore File Change Detection
The daemon automatically detects changes to ignore files and self-restarts:
- Tracks all ignore files discovered by watchexec (`.gitignore`, `.git/info/exclude`, global ignores)
- Immediately performs exec() when any tracked ignore file changes
- Ensures filter consistency without manual restart
- Fixes stale filter issue when .gitignore patterns are added/removed

## Crate Structure (Modules)

### Implemented Modules
- ✅ `cli`: Define subcommands and flags - **COMPLETE** (all commands + shell)
- ✅ `config`: Load `autosnap.*` values from git config - **COMPLETE** 
- ✅ `gitlayer`: Operations on `.autosnap/` bare repo - **COMPLETE** (init, snapshot, gc, shell)
- ✅ `watcher`: Watchexec/Tokio based file watcher - **COMPLETE** with signal handling
- ✅ `daemon`: Foreground/daemon start, graceful shutdown - **COMPLETE**
- ✅ `process`: PID file read/write, exclusive locking - **COMPLETE**
- ✅ `lib`: Entry point and command dispatch - **COMPLETE**

### Not Implemented (Integrated Elsewhere)
- ❌ `errors`: Using `anyhow` directly instead of custom error types
- ❌ `timefmt`: ISO8601 functionality integrated directly in `gitlayer`

Initial file layout under `src/`:

```
src/
  cli.rs          # clap CLI surface: init/start/stop/status/once/gc/uninstall
  lib.rs          # init_tracing + run() + module wiring
  config.rs       # read git config (autosnap.debounce-ms, autosnap.gc.prune-days)
  gitlayer.rs     # init/open .autosnap, snapshot(), gc()
  watcher.rs      # start_watcher_foreground(), watch loop (debounced)
  daemon.rs       # start_daemon(), signal handlers, pause/resume
  process.rs      # pidfile path, locking, status detection, kill signals
  errors.rs       # optional typed errors
  timefmt.rs      # timestamp + message helpers
```

## Dependencies

### Current Dependencies (Implemented)
- ✅ `anyhow` - Error handling with context
- ✅ `clap` - CLI argument parsing  
- ✅ `git2` - libgit2 bindings for Git operations
- ✅ `tokio` - Async runtime for watchexec
- ✅ `tracing` & `tracing-subscriber` - Structured logging
- ✅ `watchexec` - File watching with gitignore support
- ✅ `watchexec-filterer-ignore` - Gitignore filtering for watchexec
- ✅ `watchexec-signals` - Signal type discrimination
- ✅ `ignore-files` - Gitignore file parsing
- ✅ `fs2` - Cross-platform file locking
- ✅ `libc` - Unix system calls (setsid for daemon)
- ✅ `time` - ISO8601 timestamp formatting
- ✅ `tempfile` - Temporary directories for shell command
- ✅ `skim` - Fuzzy finder for interactive commit selection

### Not Used
- ❌ `miette` - Using simpler error reporting with anyhow
- ❌ `thiserror` - Using anyhow for all error handling
- ❌ `daemonize` - Implemented custom daemonization with libc::setsid
- ❌ `notify` - Superseded by watchexec

## CLI Surface (clap)

### Implemented Commands
- ✅ `git autosnap init` — Initialize `.autosnap/` bare repository and add to `.git/info/exclude`
- ✅ `git autosnap start [--daemon]` — Launch watcher in foreground or background
- ✅ `git autosnap stop` — Send SIGTERM to PID and wait for graceful shutdown
- ✅ `git autosnap status` — Exit 0 if running; print "running" or "stopped"
- ✅ `git autosnap once` — Take one snapshot immediately and exit (prints short SHA)
- ✅ `git autosnap gc` — Compress snapshots (pack objects) without removing any
- ✅ `git autosnap gc --prune [--days N=60]` — Compress and prune snapshots older than N days
- ✅ `git autosnap uninstall` — Stop watcher (if running) and remove `.autosnap/`
- ✅ `git autosnap shell [COMMIT] [-i]` — Open snapshot in subshell for exploration
  - Interactive mode (`-i`) uses skim for commit selection
  - Extracts snapshot to temp directory with proper permissions
  - Launches subshell with custom prompt showing commit SHA
- ✅ `git autosnap restore [COMMIT] [PATH...] [--force] [--dry-run] [--full] [-i]` — Restore files from snapshot
  - Interactive mode (`-i`) uses skim for commit selection
  - `--force` overrides safety check for uncommitted changes
  - `--dry-run` previews changes without modifying files
  - `--full` removes files not present in snapshot (excludes .git/.autosnap)
  - Supports partial restore with specific paths
- ✅ `git autosnap diff [COMMIT1] [COMMIT2] [PATH...] [--stat] [--name-only] [--name-status] [-i]` — **NEW**: Show diff between snapshots
  - No arguments: compares working tree to HEAD
  - One argument: compares specified commit to working tree
  - Two arguments: compares two commits
  - Interactive mode (`-i`) uses skim for commit selection
  - `--stat` shows only statistics (files changed, insertions, deletions)
  - `--name-only` shows only names of changed files
  - `--name-status` shows names and status of changed files
  - Supports path filtering for specific files/directories

Return codes and output are script‑friendly; errors use `anyhow` with context.

## Config (`git config autosnap.*`)

- Keys and defaults (read with precedence: local → global → system):
  - `autosnap.debounce-ms` (default 200 as per spec; CLI may override later)
  - `autosnap.gc.prune-days` (default 60; used by `gc`)
- Implement `Config::load(repo_root)` that uses `git2::Repository::discover()` then merges values.

## Git Layer

- `.autosnap/` location: `repo_root/.autosnap`.
- Init: `Repository::init_bare()` with workdir pointing to `repo_root` via `repo.set_workdir()`.
- Snapshot algorithm (per spec):
  1. `index.add_all(["*"], IndexAddOption::DEFAULT, None)`
  2. `index.write()` + `index.write_tree()`
  3. `repo.commit(Some("HEAD"), &sig, &sig, msg, &tree, &parents)`
     - Message: `AUTOSNAP[<branch-or-DETACHED>] <ISO8601 offset timestamp>`
     - Author/committer: from main repo config (`user.name`/`user.email`) or sensible fallback
- Parents: if `.autosnap` has `HEAD`, use it; otherwise create initial commit.
- Optional: lightweight GC implementation for `gc --days N`:
  - Walk commits and remove refs older than threshold or
  - Shell out to `git` inside `.autosnap` for `reflog expire --expire=<N>d --all && gc --prune=<N>d` (runs in a blocking subprocess outside async loops).

## Watcher

- `watchexec` runner on Tokio 1.
- Path-set: repo root, recursive.
- Ignores: built‑in .gitignore support + hard‑coded `/.git/` and `/.autosnap/`.
- Debounce: sliding window 200 ms (or from `autosnap.debounce-ms`).
- On event batch: trigger a snapshot (coalesced).
- On shutdown signals (SIGINT/SIGTERM): flush pending snapshot, then exit.
- Optional pause/resume on SIGUSR1/SIGUSR2.

## Daemon and Process Control

- PID file: `.autosnap/autosnap.pid` with permissions 0600.
- Single-instance: `fs2::FileExt::try_lock_exclusive()` on the PID file; if lock fails, refuse to start.
- Foreground: run watcher loop in current process.
- Daemon mode (Unix): `daemonize` to fork/detach; write PID of child to pidfile.
- `stop`: read PID, send SIGTERM, wait with timeout, clean pidfile.
- `status`: check pidfile existence + liveness probe; exit 0 if running, else non-zero.

## Logging & Error Handling

- Logging: `tracing` with `RUST_LOG` override; fields for event name, repo path, counts, durations.
- Errors: use `anyhow::Result` at boundaries; attach context with `with_context`; define `thiserror` enums if needed internally.
- No panics in normal paths; no `unwrap`/`expect` in public API or command paths.

## Security and Safety

- No network I/O; writes only under repo directory.
- Respect `.gitignore`; never watch or write into `.git/`.
- PID file permissions 0600.

## Build & Install

- Produce a single binary with `cargo build --release`.
- Install into `$(git --exec-path)/` for Git extension discovery: `git-autosnap`.

## Implementation Phases (Completed)

### Phase 1: CLI Structure ✅
- ✅ Removed template commands (`Greet`/`Sum`)
- ✅ Implemented all commands in `src/cli.rs`
- ✅ Wired `run()` in `src/lib.rs` to dispatch commands
- ✅ Added `shell` command for snapshot exploration

### Phase 2: Dependencies & Architecture ✅
- ✅ Added all required dependencies (watchexec, fs2, time, etc.)
- ✅ Created all necessary modules
- ✅ Integrated ISO8601 timestamps directly in gitlayer

### Phase 3: Core Git Operations ✅
- ✅ `.autosnap` bare repository initialization
- ✅ Automatic addition to `.git/info/exclude`
- ✅ Snapshot algorithm with deduplication (skips if no changes)
- ✅ Proper commit message format: `AUTOSNAP[branch] ISO8601`
- ✅ Author/committer signature from main repo config

### Phase 4: Process Control ✅
- ✅ PID file management with exclusive locking
- ✅ Single-instance enforcement
- ✅ Status checking via PID liveness
- ✅ Graceful shutdown with timeout

### Phase 5: File Watching ✅
- ✅ Watchexec integration with Tokio runtime
- ✅ Git-aware ignore filtering
- ✅ Configurable debounce window
- ✅ Hard exclusion of `.git/` and `.autosnap/`

### Phase 6: Signal Handling ✅
- ✅ SIGTERM/SIGINT for graceful shutdown
- ✅ SIGHUP placeholder for config reload
- ✅ SIGUSR1 for forced immediate snapshot
- ✅ SIGUSR2 for graceful binary update

### Phase 7: Daemon Mode ✅
- ✅ Background process spawning with setsid
- ✅ Proper stdio redirection to /dev/null
- ✅ Stop command with SIGTERM and wait

### Phase 8: Additional Features ✅
- ✅ GC command using git subprocess
- ✅ Config loading from git config
- ✅ Uninstall with daemon stop and cleanup
- ✅ Interactive snapshot exploration with skim
- ✅ **Restore command with libgit2 checkout (2025-08-18)**

### Phase 9: Testing Infrastructure ✅
- ✅ Unit tests for core components
- ✅ Integration tests with containers
- ✅ Test helpers and utilities
- ✅ CI/CD pipeline configuration

## Testing Strategy Mapping (from docs/testing.md)

- Unit tests (pure logic): commit message parsing/formatting, config parsing/merging, PID tools, small helpers.
- Host-based tests with `tempfile`: safe fs ops, pidfile locking, CLI argument parsing with `assert_cmd` where no daemons/signals are involved.
- Container-based integration tests (feature `container-tests`):
  - Full workflow: `init` → `start --daemon` → file changes → `status` → `stop`.
  - Watcher debounce correctness: multiple rapid writes produce a single snapshot.
  - `once` creates a commit with correct contents and message format.
  - `gc` retention behavior in `.autosnap` (older commits pruned).
  - Process lifecycle: PID file creation, single-instance lock, stop and cleanup.
  - Config isolation using `HOME`, `XDG_CONFIG_HOME`, and `GIT_CONFIG_NOSYSTEM`.
- Safety rules: never modify real host repos or configs; prefer containers; no network.

Minimal dev-deps (aligning with Appendix): `tempfile`, `assert_cmd`, `predicates`, `fs2`; optional `testcontainers` under a feature for CI.

## Quality Gates (from docs/style.md)

- `cargo fmt --all --check` must pass.
- `cargo clippy --all-targets --all-features -D warnings` must pass.
- `cargo test` (unit, integration, doctests) must be green.
- No `unwrap`/`expect` in command paths; errors carry context.
- Public items documented with `///` and examples where meaningful.
- Concurrency: set timeouts, avoid blocking inside async, keep shared state minimal.

## Acceptance Criteria (per command)

- `init`: Creates `.autosnap/` bare repo and prints success; idempotent re-run.
- `once`: Creates a commit in `.autosnap` with correct message and includes changed files.
- `start`: Foreground runs watcher; `--daemon` detaches, writes PID, and begins committing on changes.
- `stop`: Terminates the daemon by pidfile; cleans up pidfile; idempotent if not running.
- `status`: Exit 0 with “running” when daemon alive; non-zero otherwise.
- `gc`: Compresses objects via packing; with `--prune`, removes commits older than `--days` (or config).
- `uninstall`: Stops daemon if running and removes `.autosnap/` directory.

## Notes on Platform Scope

- Focus on macOS/Unix per spec. Use `cfg(unix)` for daemonization and unix signals; gate unsupported features on non‑Unix.

## Testing Coverage

### Unit Tests
- ✅ Configuration loading (`tests/config_load.rs`)
- ✅ PID file locking (`tests/pid_lock.rs`)
- ✅ Git exclude functionality (`tests/exclude_init.rs`)
- ✅ CLI help output (`tests/cli_help.rs`)

### Integration Tests
- ✅ Daemon lifecycle (`tests/daemon_lifecycle.rs`)
- ✅ Snapshot and GC operations (`tests/snapshot_gc.rs`)
- ✅ Container-based test infrastructure (`tests/support/`)

## Future Enhancements

### Potential Improvements
1. **Compression**: Add snapshot compression options to reduce disk usage
2. **Remote Backup**: Optional remote repository sync for disaster recovery
3. **Web UI**: Simple web interface for browsing snapshots
4. ~~**Diff Viewer**: Built-in diff between snapshots~~ ✅ **IMPLEMENTED (2025-08-18)**
5. ~~**Restore Command**: Direct restore from snapshot to working tree~~ ✅ **IMPLEMENTED (2025-08-18)**
6. **Metrics**: Prometheus-compatible metrics endpoint for monitoring
7. **Windows Support**: Extend beyond Unix platforms
8. **Performance Optimizations**: Parallel file processing for large repositories
9. **Selective Watching**: Allow including/excluding specific directories from watching
10. **Snapshot Annotations**: Add custom messages or tags to snapshots

### Known Limitations
- Unix/macOS only (Windows not supported)
- No built-in snapshot encryption
- No automatic remote synchronization
- Single repository scope (no multi-repo watching)

## Project Status

**✅ PRODUCTION READY**: All core features are implemented, tested, and operational. The tool is ready for daily use in development workflows.

### Quality Metrics
- All commands functional and tested
- Signal handling for graceful operations
- Self-restart capability for ignore file changes and binary updates
- Zero-downtime updates via exec() with PID preservation
- Container-based testing ensures safety
- Follows Rust best practices per `docs/style.md`
- Comprehensive error handling with context
- Script-friendly output for automation

