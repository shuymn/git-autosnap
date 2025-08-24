# Repository Guidelines

## Project Structure & Module Organization
- `src/`: Rust sources — `cli.rs` (CLI), `gitlayer.rs` (Git ops, snapshots, gc, diff, restore, shell), `watcher.rs` (watchexec loop), `daemon.rs` (background start/stop), `process.rs` (PID/locks), `config.rs` (git config), `lib.rs` (wiring), `main.rs` (entry).
- `tests/`: Unit/integration tests; container tests are behind `--features container-tests` and use `tests/support/` helpers.
- `docs/`: Specs, style, and testing strategy.  `Taskfile.yml` and `lefthook.yml` aid local dev.  `Dockerfile.test` builds the test image.

## Build, Test, and Development Commands
- Build: `cargo build` (debug), `cargo build --release` (release). Example run: `cargo run -- --help`.
- Task runner (optional): `task build`, `task test-unit`, `task test-container` (requires Docker), `task install` (hot‑reloads daemon via SIGUSR2).
- Tests: `cargo test` (unit); container tests: `cargo test --features container-tests` or `task test-container`.

## Coding Style & Naming Conventions
- Formatting: `cargo fmt --all` must be clean; CI/lefthook enforce it.
- Linting: `cargo clippy --all-targets --all-features -D warnings` must pass.
- Naming: Types/traits = PascalCase; functions/vars = snake_case; constants = SCREAMING_SNAKE_CASE; modules = snake_case.
- Error handling: use `anyhow` with context; avoid `unwrap`/`expect` in non-test code.
- Logging: use `tracing`; `RUST_LOG` or `-v/-vv` control verbosity.

## Testing Guidelines
- Unit tests live alongside code or in `tests/` and use `assert_cmd`, `tempfile`, etc.
- Container integration tests (feature `container-tests`) use `testcontainers` and `Dockerfile.test` for isolation.
- Run fast suite: `cargo test`; full suite: `task test` (includes container tests).
- Keep tests isolated; do not touch host state. Prefer temp dirs or containers.

## Commit & Pull Request Guidelines
- Commits: Imperative mood, concise summary (<72 chars) with optional scope (e.g., `watcher:`). Explain rationale in body.
- PRs: Clear description, linked issue(s), reproduction steps, and before/after output where relevant.
- Requirements: Passing `fmt`/`clippy`/tests, updated docs (`docs/spec.md`, README) when CLI/behavior changes, and tests for new logic.
- Keep changes focused; avoid drive‑by fixes unrelated to the PR.

## Configuration & Architecture Notes
- Local bare repo at `.autosnap/`; main history untouched. Respects `.gitignore`.
- Config via `git config`: `autosnap.debounce-ms` (default 1000), `autosnap.gc.prune-days` (default 60).
- Daemon uses PID at `.autosnap/autosnap.pid`; signals: SIGTERM/SIGINT (graceful), SIGUSR1 (snapshot), SIGUSR2 (hot‑reload).
