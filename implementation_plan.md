# git-autosnap — Implementation Plan

This plan translates the docs in `docs/spec.md`, `docs/testing.md`, and `docs/style.md` into concrete implementation steps for the Rust CLI binary `git-autosnap`.

## Scope and Goals

- Implement a Git extension that records timestamped snapshots of a working tree into a local bare repository at `.autosnap/`.
- Provide a long‑running watcher process with start/stop/status controls and a single-instance lock via a PID file.
- Respect `.gitignore`, avoid touching the main repo history, and keep all data local to the repo.
- Follow the coding standards in `docs/style.md` and the testing strategy in `docs/testing.md`.

## Crate Structure (Modules)

- `cli` (existing): Define subcommands and flags.
- `config`: Load `autosnap.*` values from git config (local > global > system).
- `gitlayer`: Operations on the `.autosnap/` bare repo (init, open, snapshot commit, optional GC).
- `watcher`: Watchexec/Tokio based file watcher with debounce/ignore integration.
- `daemon`: Foreground/daemon start, signal handling, graceful shutdown.
- `process`: PID file read/write, exclusive locking via `fs2`, status checks.
- `errors`: Error types if needed (with `thiserror`); applications aggregate via `anyhow`.
- `timefmt`: Helpers for ISO8601 timestamps and commit message formatting.

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

- Keep: `anyhow`, `clap`, `git2`, `tokio`, `tracing`, `tracing-subscriber`, `miette`.
- Add per spec:
  - `watchexec` (for file events + built-in gitignore + debounce)
  - `fs2` (file locking)
  - `thiserror` (typed errors where useful)
  - `time` (ISO8601 timestamps) or `chrono` (choose `time` for no-std friendly API)
  - `nix` or `tokio` unix signals (use `tokio::signal::unix` on macOS/Unix)
  - `daemonize` (daemon mode on Unix), gated behind `cfg(unix)`
- Remove/replace: `notify` (superseded by `watchexec` per spec).

## CLI Surface (clap)

- `git autosnap init` — Initialize `.autosnap/` bare repository.
- `git autosnap start [--daemon]` — Launch watcher in foreground or background.
- `git autosnap stop` — Send SIGTERM to PID in `.autosnap/autosnap.pid` and wait for exit.
- `git autosnap status` — Exit 0 if running; print concise state.
- `git autosnap once` — Take one snapshot immediately and exit.
- `git autosnap gc [--days N=60]` — Prune snapshots older than N days.
- `git autosnap uninstall` — Stop watcher (if running) and remove `.autosnap/`.

Return codes and output must be script‑friendly; errors via `anyhow` + `miette` reporting.

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

## Step-by-Step Implementation Phases

1) Replace template commands with real CLI
- Remove `Greet`/`Sum` and introduce `init/start/stop/status/once/gc/uninstall` in `src/cli.rs`.
- Wire `run()` in `src/lib.rs` to match new command enum.

2) Add dependencies and scaffolding
- Add: `watchexec`, `fs2`, `thiserror`, `time`, `daemonize` (unix), ensure `tokio` features.
- Remove: `notify`.
- Create module skeletons: `config`, `gitlayer`, `watcher`, `daemon`, `process`, `timefmt`, `errors`.

3) Implement `.autosnap` initialization
- Discover repo root (fail if not in a Git repo).
- Create `.autosnap` via `git2::Repository::init_bare` if missing; set workdir to root.
- Output concise confirmation.

4) Implement snapshot commit
- `gitlayer::snapshot(repo_root)` implementing the algorithm and message format.
- Helper to get current branch name or `DETACHED`.
- Unit tests for commit-message formatting and parent handling.

5) Implement `once`
- Command calls `snapshot()` exactly once; prints new commit id short hash.

6) Implement process control (pidfile)
- `process::PidFile`: create/open, exclusive lock, write pid, read pid, remove.
- Unit tests for locking using `fs2` (host-based, `tempfile`).

7) Implement watcher (foreground)
- `watcher::run_foreground(config, repo_root, shutdown_rx)`.
- Use watchexec with debounce to coalesce events; ignore `.git/` and `.autosnap/`.
- On event → `snapshot()`; log counts and durations.

8) Implement daemon mode and signals
- `daemon::start_daemon(...)` forks and runs watcher in child.
- Signals: handle SIGINT/SIGTERM for graceful shutdown; optionally SIGUSR1/SIGUSR2 pause/resume.
- `stop` sends SIGTERM to PID from pidfile; wait with timeout for exit.

9) Implement `status`
- Read pidfile then check if process is alive; set exit code accordingly and print state.

10) Implement `gc`
- Parse `--days` or read from config default.
- Either call into a GC helper or shell out to `git` within `.autosnap` to prune; log results.

11) Implement `uninstall`
- Stop if running; remove `.autosnap` recursively.

12) Diagnostics & polish
- Add structured logs for all commands; ensure helpful `miette` error reports.
- Ensure no blocking work inside async tasks; use `spawn_blocking` if needed.

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
- `gc`: Removes commits older than `--days` (or config); leaves recent commits.
- `uninstall`: Stops daemon if running and removes `.autosnap/` directory.

## Notes on Platform Scope

- Focus on macOS/Unix per spec. Use `cfg(unix)` for daemonization and unix signals; gate unsupported features on non‑Unix.

## Next Actions

1. Update `Cargo.toml`: add required crates, remove `notify`.
2. Replace template commands with autosnap commands.
3. Implement `.autosnap` init + snapshot core.
4. Implement pidfile + watcher (foreground), then daemon mode.
5. Implement stop/status/gc/uninstall.
6. Add unit tests and feature‑gated container tests; wire into CI.
7. Ensure style gates pass (`fmt`, `clippy`, docs) and finalize logging.

