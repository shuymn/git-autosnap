# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Rust sources. Key modules: `cli.rs` (CLI), `gitlayer.rs` (Git ops: snapshots, gc, diff, restore, shell), `watcher.rs` (watchexec loop), `daemon.rs` (background start/stop), `process.rs` (PID/locks), `config.rs` (git config), `lib.rs` (wiring), `main.rs` (entry).
- `tests/`: Unit/integration tests; container tests behind `--features container-tests`; helpers in `tests/support/`.
- `docs/`: Specs, style, testing strategy. Also `Taskfile.yml`, `lefthook.yml`, and `Dockerfile.test`.

## Build, Test, and Development Commands
- Prefer Taskfile tasks:
  - Build: `task build` (debug), `task build-release` (release)
  - Run: `task run -- ARGS="--help"`
  - Install: `task install` (hot‑reloads daemon via `SIGUSR2` if running)
  - Lint/Format: `task fmt`, `task clippy`, `task lint`
  - Tests: `task test`, `task test-unit`, `task test-container` (Docker required)
  - Docs: `task docs` (or `task docs-all`)
  - Clean: `task clean`
  - Utilities: `task deps`, `task update`, `task coverage`, `task bench`, `task version`
- Cargo alternatives: `cargo build`, `cargo build --release`, `cargo run -- --help`.
- Verbose logging: `-v`/`-vv` or `RUST_LOG=git_autosnap=debug`.

## Coding Style & Naming Conventions
- Formatting: `cargo fmt --all` must be clean.
- Linting: `cargo clippy --all-targets --all-features -D warnings` must pass.
- Naming: Types/traits = PascalCase; functions/vars = snake_case; constants = SCREAMING_SNAKE_CASE; modules = snake_case.
- Errors: use `anyhow` with context; avoid `unwrap`/`expect` in non-test code.
- Logging: use `tracing` and control via `RUST_LOG` or `-v/-vv`.

## Testing Guidelines
- Fast suite: `task test` (or `task test-unit`).
- Container tests: `task test-container` (or `cargo test --features container-tests`); use `tests/support/` helpers.
- Keep tests isolated; prefer temp dirs or containers; do not touch host state.

## Commit & Pull Request Guidelines
- Commits: Imperative mood, concise summary (<72 chars) with optional scope (e.g., `watcher:`). Provide rationale in the body.
- PRs: Clear description, linked issue(s), repro steps, and before/after output when relevant. Require passing fmt/clippy/tests; update `docs/spec.md` and README for CLI/behavior changes. Keep changes focused; avoid drive‑by fixes.

## Configuration & Architecture Notes
- Local bare repo at `.autosnap/`; main history untouched; honors `.gitignore`.
- Config via git: `git config autosnap.debounce-ms 1000`, `git config autosnap.gc.prune-days 60`.
- Daemon PID: `.autosnap/autosnap.pid`; signals: `SIGTERM`/`SIGINT` (graceful), `SIGUSR1` (snapshot), `SIGUSR2` (hot‑reload). Example: `kill -USR1 $(cat .autosnap/autosnap.pid)`.
