# Implementation Plan

## Watcher Cleanup and Style Compliance

Goals
- Eliminate ad-hoc state in `watcher.rs` by introducing a typed exit intent and bounded channels.
- Reduce function size and improve readability without changing behavior.
- Align with docs/style.md: avoid blocking in async contexts, set timeouts/buffer limits, and document public items.

Scope
- `src/watcher.rs` (primary), minimal supporting changes if needed.
- Tests for precedence and behavior where feasible.

Plan
1) Bounded channel for binary-update poller (MUST)
   - Replace `std::sync::mpsc::channel` with `sync_channel(1)`.
   - Update `WatcherState` to use `SyncSender<bool>`.
   - Keep receiver side unchanged; ensure no unbounded buffering.

2) Typed exit intent (SHOULD)
   - Introduce `#[repr(u8)] enum ExitAction { None=0, Snapshot=1, ReloadExec=2, BinaryUpdateExec=3 }`.
   - Store `Arc<AtomicU8>` in state (for atomic updates) and provide helpers:
     - `fn elevate_exit_action(exit: &Arc<AtomicU8>, new: ExitAction)` — only increases precedence.
     - `fn load_exit_action(exit: &Arc<AtomicU8>) -> ExitAction` — converts byte to enum safely.
   - Replace raw constants and casts with enum helpers for clarity.

3) Split `run_watcher` into focused helpers (SHOULD)
   - `fn build_state(repo_root: &Path) -> (Arc<WatcherState>, Receiver<bool>)` — sets up state, original metadata, and bounded channel.
   - `fn build_watchexec_config(state: Arc<WatcherState>, filterer: IgnoreFilterer, debounce_ms: u64) -> watchexec::Config` — wires callbacks, filters, throttle, and errors.
   - `fn finalize_exit_actions(repo_root: &Path, exit: &Arc<AtomicU8>, binary_rx: &Receiver<bool>)` — performs final snapshot and optional execs per precedence.
   - Keep existing `build_filterer_and_ignores` as-is.

4) Non-blocking snapshots (NICE)
   - Consider `tokio::task::spawn_blocking` instead of `std::thread::spawn` for snapshot work to integrate with the runtime, while still keeping callback non-blocking.
   - Maintain the `snapshot_in_progress` lock semantics.

5) Documentation updates (SHOULD)
   - Add brief `///` docs to public functions describing debounce semantics and exit-action precedence.
   - Note that heavy work runs after watcher stop to avoid filling internal channels.

6) Logging consistency (SHOULD)
   - Ensure structured fields for key events (e.g., `event="snapshot_created"`, `debounce_ms`, `path`).
   - Keep logs concise at info/warn; use debug for skips due to in-progress snapshot.

7) Tests (SHOULD)
   - Add a small unit test for exit-action elevation precedence (pure Rust test, no I/O).
   - Container tests remain unchanged; optionally assert that ignore-file changes cause a reload exec path.

8) Verification
   - `cargo fmt`, `cargo clippy -D warnings`, `cargo test`.
   - Manual smoke test: `git autosnap start`, modify files, send `SIGUSR1`, `SIGINT`, `SIGUSR2`; verify expected behavior and absence of channel backpressure logs.

Out of Scope
- Import grouping/order (left to rustfmt per guidance).
- Behavior changes beyond readability, bounded channels, and typed exit intent.

Acceptance Criteria
- No functional regressions: snapshots still occur; signals behave as before; hot-reload works.
- Watchexec callback remains non-blocking; no “sending into a full channel” spam.
- Code passes fmt/clippy/tests and adheres to style guidelines.
