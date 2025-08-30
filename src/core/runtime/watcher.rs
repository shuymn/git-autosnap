use crate::config::AutosnapConfig;
use crate::core::git;
use crate::core::runtime::process;
use crate::logging::init::flush_logs;
use anyhow::{Context, Result, anyhow};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::time::Duration;
use tracing::{error, info, warn};
use watchexec::Watchexec;
use watchexec_events::FileType;
use watchexec_filterer_ignore::IgnoreFilterer;

/// Perform exec to restart the process with the same arguments.
/// This replaces the current process with a new instance.
fn perform_exec() {
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => {
            error!("failed to get current exe: {}", e);
            return;
        }
    };

    let args: Vec<String> = std::env::args().collect();
    info!("re-executing {} with args {:?}", exe.display(), &args[1..]);

    // Flush logs before exec to ensure all buffered messages are written
    flush_logs();

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let error = std::process::Command::new(exe)
            .args(&args[1..]) // Skip program name
            .exec(); // Never returns on success

        // Only reached if exec fails
        error!("exec failed: {}", error);
    }

    #[cfg(not(unix))]
    {
        error!("exec not supported on non-Unix platforms");
    }
}

/// Start the foreground watcher loop using watchexec with git-aware ignores and debounce.
///
/// - Debounce is controlled by `autosnap.debounce-ms` (default 1000ms).
/// - The watchexec action callback remains non-blocking; heavy work (snapshots, exec)
///   is deferred and performed after the watcher stops to avoid internal backpressure.
pub fn start_foreground(repo_root: &Path, cfg: &AutosnapConfig) -> Result<()> {
    // ensure exists; ignore if already present
    git::init_autosnap(repo_root).ok();
    // Acquire single-instance lock and write pid
    let _guard = process::acquire_lock(repo_root)?;

    // Run watchexec on a Tokio runtime to handle async APIs.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    rt.block_on(run_watcher(repo_root, cfg.debounce_ms))
}

// Exit actions to perform after the watcher stops.
// Higher value means higher precedence when coalescing multiple intents.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ExitAction {
    None = 0,
    Snapshot = 1,
    ReloadExec = 2,
    BinaryUpdateExec = 3,
}

/// Shared watcher state used by handlers.
struct WatcherState {
    repo_root: PathBuf,
    tracked_ignores: HashSet<PathBuf>,
    // What to do after the watcher stops (snapshot/reload/binary update).
    exit_action: Arc<AtomicU8>,
    binary_update_tx: SyncSender<bool>,
    original_binary_metadata: Option<std::fs::Metadata>,
    snapshot_in_progress: Arc<AtomicBool>,
}

/// Control flow from a handler.
enum Flow {
    Continue,
    Quit,
}

// Ensure we only ever increase the exit action's precedence.
fn elevate_exit_action(exit_action: &Arc<AtomicU8>, new: ExitAction) {
    let new_val = new as u8;
    let mut cur = exit_action.load(Ordering::SeqCst);
    loop {
        if cur >= new_val {
            break;
        }
        match exit_action.compare_exchange(cur, new_val, Ordering::SeqCst, Ordering::SeqCst) {
            Ok(_) => break,
            Err(actual) => cur = actual,
        }
    }
}

fn load_exit_action(exit_action: &Arc<AtomicU8>) -> ExitAction {
    match exit_action.load(Ordering::SeqCst) {
        0 => ExitAction::None,
        1 => ExitAction::Snapshot,
        2 => ExitAction::ReloadExec,
        3..=u8::MAX => ExitAction::BinaryUpdateExec,
    }
}

async fn run_watcher(repo_root: &Path, debounce_ms: u64) -> Result<()> {
    // Build git-aware ignore filterer and tracked ignore files
    let (filterer, tracked_ignore_files) = build_filterer_and_ignores(repo_root)
        .await
        .context("failed to create watchexec")?;

    info!(
        "tracking {} ignore files for changes",
        tracked_ignore_files.len()
    );

    // Build shared state and binary update channel
    let (state, binary_update_rx) = build_state(repo_root, tracked_ignore_files)?;

    // Build watchexec config and start
    let config = build_watchexec_config(state.clone(), filterer, debounce_ms);
    let wx = Watchexec::with_config(config).context("failed to create watchexec")?;

    info!(event = "watch_start", path = %repo_root.display(), debounce_ms, "watching");
    let handle = wx.main();
    let result = match handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(anyhow!("watchexec critical error: {e}")),
        Err(e) => Err(anyhow!("watchexec task join error: {e}")),
    };

    // Perform any requested final actions outside the watchexec action callback
    finalize_exit_actions(repo_root, &state.exit_action, &binary_update_rx);

    result
}

fn build_state(
    repo_root: &Path,
    tracked_ignore_files: HashSet<PathBuf>,
) -> Result<(Arc<WatcherState>, Receiver<bool>)> {
    let (binary_update_tx, binary_update_rx) = sync_channel::<bool>(1);
    let exit_action = Arc::new(AtomicU8::new(ExitAction::None as u8));
    let snapshot_in_progress = Arc::new(AtomicBool::new(false));

    // Capture original binary metadata at startup for hot-reload detection
    let original_binary_metadata = std::env::current_exe()
        .ok()
        .and_then(|path| path.metadata().ok());

    let state = Arc::new(WatcherState {
        repo_root: repo_root.to_path_buf(),
        tracked_ignores: tracked_ignore_files,
        exit_action: exit_action.clone(),
        binary_update_tx,
        original_binary_metadata,
        snapshot_in_progress,
    });

    Ok((state, binary_update_rx))
}

fn build_watchexec_config(
    state: Arc<WatcherState>,
    filterer: IgnoreFilterer,
    debounce_ms: u64,
) -> watchexec::Config {
    let config = watchexec::Config::default();

    // Handler: trigger snapshot on any coalesced (throttled) event batch
    let handler_state = state.clone();
    config.on_action(move |mut action| {
        // Check if any changed path is an ignore file we're tracking
        let paths: Vec<(&Path, Option<&FileType>)> = action.paths().collect();
        if let Flow::Quit = handle_ignore_file_updates(&paths, &handler_state) {
            action.quit();
            return action;
        }

        // Handle signals - collect them first to avoid borrowing issues
        let signals: Vec<_> = action.signals().collect();
        if let Flow::Quit = handle_signals(&signals, &handler_state) {
            action.quit();
            return action;
        }

        // Snapshot on any events with paths
        handle_fs_events(&paths, &handler_state);

        action
    });

    // Configure watchexec paths, filters and throttling
    config.pathset([state.repo_root.clone()]);
    config.filterer(filterer);
    config.throttle(Duration::from_millis(debounce_ms));
    config.on_error(|err: watchexec::ErrorHook| {
        tracing::error!(event = "watchexec_error", "watchexec error: {}", err.error);
    });

    config
}

fn finalize_exit_actions(
    repo_root: &Path,
    exit_action: &Arc<AtomicU8>,
    binary_update_rx: &Receiver<bool>,
) {
    let action = load_exit_action(exit_action);

    if (action as u8) >= (ExitAction::Snapshot as u8) {
        match git::snapshot_once(repo_root, None) {
            Ok(Some(hash)) => {
                info!(
                    hash = hash,
                    event = "snapshot_created",
                    "final snapshot created"
                );
            }
            Ok(None) => {
                info!(event = "snapshot_skipped", "no changes to snapshot");
            }
            Err(e) => {
                error!(error = ?e, event = "snapshot_failed", "final snapshot failed");
            }
        }
    }

    if action == ExitAction::BinaryUpdateExec {
        await_binary_update_and_maybe_exec(binary_update_rx);
    } else if action == ExitAction::ReloadExec {
        info!(event = "reload_exec", "reloading after ignore file change");
        perform_exec();
        warn!("exec failed after ignore reload, exiting normally");
    }
}

async fn build_filterer_and_ignores(
    repo_root: &Path,
) -> Result<(IgnoreFilterer, HashSet<PathBuf>)> {
    // Build git-aware ignore filterer (project + environment), then add hard excludes
    let (mut origin_files, _errors1) = ignore_files::from_origin(repo_root).await;
    let (env_files, _errors2) = ignore_files::from_environment(None).await;

    // Track all ignore file paths for change detection
    let mut tracked_ignore_files = HashSet::new();
    for file in &origin_files {
        tracked_ignore_files.insert(file.path.clone());
    }

    let has_project_excludes = origin_files.iter().any(|f| f.applies_in.is_none());
    if !has_project_excludes {
        origin_files.extend(env_files.clone());
        // Also track environment ignore files if we're using them
        for file in &env_files {
            tracked_ignore_files.insert(file.path.clone());
        }
    }

    let mut filter = ignore_files::IgnoreFilter::new(repo_root, &origin_files)
        .await
        .map_err(|e| anyhow!("ignore filter build failed: {e}"))?;
    filter
        .add_globs(&["/.git", "/.autosnap"], Some(&repo_root.to_path_buf()))
        .map_err(|e| anyhow!("ignore hard excludes failed: {e}"))?;
    let filterer = IgnoreFilterer(filter);

    Ok((filterer, tracked_ignore_files))
}

fn handle_ignore_file_updates(paths: &[(&Path, Option<&FileType>)], state: &WatcherState) -> Flow {
    for (path, _file_type) in paths {
        if state.tracked_ignores.contains(*path) {
            info!(event = "ignore_change", file = %path.display(), "detected change to tracked ignore file");
            // Defer heavy work: final snapshot and exec after we stop the watcher loop.
            // Elevate exit action to reload-exec.
            elevate_exit_action(&state.exit_action, ExitAction::ReloadExec);
            return Flow::Quit; // cause wx to stop; we'll exec after it returns
        }
    }
    Flow::Continue
}

fn handle_signals(signals: &[watchexec_signals::Signal], state: &WatcherState) -> Flow {
    use watchexec_signals::Signal;
    for signal in signals {
        match signal {
            // SIGTERM, SIGINT: Graceful shutdown with final snapshot
            Signal::Terminate | Signal::Interrupt => {
                info!(
                    event = "shutdown_signal",
                    "received shutdown signal; scheduling final snapshot"
                );
                // Defer final snapshot until after wx has stopped to avoid blocking the
                // watchexec action channel. This prevents channel-full errors.
                elevate_exit_action(&state.exit_action, ExitAction::Snapshot);
                return Flow::Quit; // stop wx; outer scope will run snapshot
            }
            // SIGHUP: Reload (for future config reload implementation)
            Signal::Hangup => {
                info!(
                    event = "reload_signal",
                    "received SIGHUP - reload signal (not yet implemented)"
                );
                // TODO: Reload configuration
            }
            // SIGUSR1: Force immediate snapshot
            Signal::User1 => {
                info!(
                    event = "force_snapshot_signal",
                    "received SIGUSR1 - forcing snapshot"
                );
                // Use the same deduplication logic as handle_fs_events
                if state
                    .snapshot_in_progress
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    let root = state.repo_root.clone();
                    let in_progress = state.snapshot_in_progress.clone();
                    tokio::task::spawn_blocking(move || {
                        match git::snapshot_once(&root, None) {
                            Ok(Some(hash)) => {
                                info!(
                                    hash = hash,
                                    event = "snapshot_created",
                                    "forced snapshot created"
                                );
                            }
                            Ok(None) => {
                                info!(event = "snapshot_skipped", "no changes to snapshot");
                            }
                            Err(e) => {
                                error!(error = ?e, event = "snapshot_failed", "forced snapshot failed");
                            }
                        }
                        in_progress.store(false, Ordering::SeqCst);
                    });
                } else {
                    warn!("snapshot already in progress, cannot force another");
                }
            }
            // SIGUSR2: Prepare for binary replacement (exec new binary)
            Signal::User2 => {
                info!(
                    event = "binary_update_signal",
                    "received SIGUSR2 - scheduling pre-update snapshot and exec"
                );
                // Defer snapshot and exec outside the action callback,
                // but start the binary-change poller now.
                request_binary_update(state);
                return Flow::Quit;
            }
            _ => {
                // Ignore other signals
            }
        }
    }
    Flow::Continue
}

fn handle_fs_events(paths: &[(&Path, Option<&FileType>)], state: &WatcherState) {
    if !paths.is_empty() {
        tracing::debug!(event = "fs_events", count = paths.len());
        // Try to acquire the snapshot lock using compare_exchange
        // If false -> true succeeds, we got the lock and can proceed
        // If it fails, another snapshot is already in progress
        if state
            .snapshot_in_progress
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
        {
            let root = state.repo_root.clone();
            let in_progress = state.snapshot_in_progress.clone();
            tokio::task::spawn_blocking(move || {
                match git::snapshot_once(&root, None) {
                    Ok(Some(hash)) => {
                        info!(hash = hash, event = "snapshot_created", "snapshot created");
                    }
                    Ok(None) => {
                        info!(event = "snapshot_skipped", "no changes to snapshot");
                    }
                    Err(e) => {
                        error!(error = ?e, event = "snapshot_failed", "snapshot failed");
                    }
                }
                // Always clear the flag when done
                in_progress.store(false, Ordering::SeqCst);
            });
        } else {
            // Another snapshot is already in progress, skip this one
            tracing::debug!(event = "snapshot_skipped", reason = "in_progress");
        }
    }
}

fn request_binary_update(state: &WatcherState) {
    // Defer final snapshot and exec to after the watcher stops and
    // choose the highest-precedence action: binary update exec.
    elevate_exit_action(&state.exit_action, ExitAction::BinaryUpdateExec);
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            error!("failed to get current exe: {}", e);
            return;
        }
    };

    // Use the metadata captured at startup (before any updates)
    let original_metadata = state.original_binary_metadata.clone();

    // If we don't have original metadata, we can't detect changes
    if original_metadata.is_none() {
        warn!("no original binary metadata available, cannot detect updates");
        return;
    }

    // Spawn polling task
    let exe_for_poll = exe_path.clone();
    let tx_for_poll = state.binary_update_tx.clone();
    std::thread::spawn(move || {
        info!(
            event = "binary_update_wait",
            "waiting for binary to change at {}",
            exe_for_poll.display()
        );

        for i in 0..30 {
            // 30 * 500ms = 15s max
            std::thread::sleep(Duration::from_millis(500));

            if let Ok(new_meta) = exe_for_poll.metadata()
                && let Some(ref orig) = original_metadata
            {
                // Check if binary changed (different inode or modification time)
                #[cfg(unix)]
                {
                    use std::os::unix::fs::MetadataExt;
                    if new_meta.ino() != orig.ino() {
                        info!(
                            event = "binary_update_ready",
                            method = "inode",
                            delay_ms = (i + 1) * 500,
                            "binary changed (inode), ready to exec"
                        );
                        let _ = tx_for_poll.send(true);
                        return;
                    }
                }

                if new_meta.modified().ok() != orig.modified().ok() {
                    info!(
                        event = "binary_update_ready",
                        method = "mtime",
                        delay_ms = (i + 1) * 500,
                        "binary changed (mtime), ready to exec"
                    );
                    let _ = tx_for_poll.send(true);
                    return;
                }
            }
        }

        warn!(
            event = "binary_update_timeout",
            "binary unchanged after 15s timeout"
        );
        let _ = tx_for_poll.send(false);
    });
}

fn await_binary_update_and_maybe_exec(rx: &Receiver<bool>) {
    if let Ok(should_exec) = rx.recv_timeout(Duration::from_secs(16)) {
        if should_exec {
            info!(
                event = "binary_update_detected",
                "binary update detected, performing exec"
            );
            perform_exec();
            // If exec fails, we'll fall through
            warn!("exec failed after SIGUSR2, exiting normally");
        } else {
            warn!(
                event = "binary_update_timeout",
                "binary update timeout, exiting without exec"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_action_elevation_respects_precedence() {
        let a = Arc::new(AtomicU8::new(ExitAction::None as u8));

        // Elevate from None -> Snapshot
        elevate_exit_action(&a, ExitAction::Snapshot);
        assert_eq!(load_exit_action(&a), ExitAction::Snapshot);

        // Attempt to lower -> should remain Snapshot
        elevate_exit_action(&a, ExitAction::None);
        assert_eq!(load_exit_action(&a), ExitAction::Snapshot);

        // Elevate to ReloadExec
        elevate_exit_action(&a, ExitAction::ReloadExec);
        assert_eq!(load_exit_action(&a), ExitAction::ReloadExec);

        // Elevate to BinaryUpdateExec (highest)
        elevate_exit_action(&a, ExitAction::BinaryUpdateExec);
        assert_eq!(load_exit_action(&a), ExitAction::BinaryUpdateExec);

        // Further attempts to lower must be ignored
        elevate_exit_action(&a, ExitAction::Snapshot);
        assert_eq!(load_exit_action(&a), ExitAction::BinaryUpdateExec);
    }
}
