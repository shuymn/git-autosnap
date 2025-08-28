# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Rust sources. Key modules: `cli.rs` (CLI), `gitlayer.rs` (Git ops: snapshots, gc, diff, restore, shell), `watcher.rs` (watchexec loop), `daemon.rs` (background start/stop), `process.rs` (PID/locks), `config.rs` (git config), `lib.rs` (wiring), `main.rs` (entry).
- `tests/`: Unit/integration tests; container tests behind `--features container-tests`; helpers in `tests/support/`.
- `docs/`: Specs, style, testing strategy. Also `Taskfile.yml`, `lefthook.yml`, and `Dockerfile.test`.

## Build, Test, and Development Commands
- Build: `cargo build` (debug), `cargo build --release` (release).
- Run: `cargo run -- --help`.
- Task runner: `task build`, `task test-unit`, `task test-container` (requires Docker), `task install` (hot‑reloads daemon via `SIGUSR2`).
- Verbose logging: `-v`/`-vv` or `RUST_LOG=git_autosnap=debug`.

## Coding Style & Naming Conventions
- Formatting: `cargo fmt --all` must be clean.
- Linting: `cargo clippy --all-targets --all-features -D warnings` must pass.
- Naming: Types/traits = PascalCase; functions/vars = snake_case; constants = SCREAMING_SNAKE_CASE; modules = snake_case.
- Errors: use `anyhow` with context; avoid `unwrap`/`expect` in non-test code.
- Logging: use `tracing` and control via `RUST_LOG` or `-v/-vv`.

## Testing Guidelines
- Fast suite: `cargo test`.
- Container tests: `cargo test --features container-tests` or `task test-container`; use `tests/support/` helpers.
- Keep tests isolated; prefer temp dirs or containers; do not touch host state.

## Commit & Pull Request Guidelines
- Commits: Imperative mood, concise summary (<72 chars) with optional scope (e.g., `watcher:`). Provide rationale in the body.
- PRs: Clear description, linked issue(s), repro steps, and before/after output when relevant. Require passing fmt/clippy/tests; update `docs/spec.md` and README for CLI/behavior changes. Keep changes focused; avoid drive‑by fixes.

## Configuration & Architecture Notes
- Local bare repo at `.autosnap/`; main history untouched; honors `.gitignore`.
- Config via git: `git config autosnap.debounce-ms 1000`, `git config autosnap.gc.prune-days 60`.
- Daemon PID: `.autosnap/autosnap.pid`; signals: `SIGTERM`/`SIGINT` (graceful), `SIGUSR1` (snapshot), `SIGUSR2` (hot‑reload). Example: `kill -USR1 $(cat .autosnap/autosnap.pid)`.

