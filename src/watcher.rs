use crate::config::AutosnapConfig;
use crate::gitlayer;
use crate::process;
use anyhow::{Context, Result, anyhow};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
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
pub fn start_foreground(repo_root: &Path, cfg: &AutosnapConfig) -> Result<()> {
    // ensure exists; ignore if already present
    gitlayer::init_autosnap(repo_root).ok();
    // Acquire single-instance lock and write pid
    let _guard = process::acquire_lock(repo_root)?;

    // Run watchexec on a Tokio runtime to handle async APIs.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    let repo_root = repo_root.to_path_buf();
    rt.block_on(run_watcher(repo_root, cfg.debounce_ms))
}

/// Shared watcher state used by handlers.
struct WatcherState {
    repo_root: PathBuf,
    tracked_ignores: HashSet<PathBuf>,
    binary_update_requested: Arc<AtomicBool>,
    binary_update_tx: Sender<bool>,
}

/// Control flow from a handler.
enum Flow {
    Continue,
    Quit,
}

async fn run_watcher(repo_root: PathBuf, debounce_ms: u64) -> Result<()> {
    // Build git-aware ignore filterer and tracked ignore files
    let (filterer, tracked_ignore_files) = build_filterer_and_ignores(&repo_root)
        .await
        .context("failed to create watchexec")?;

    info!(
        "tracking {} ignore files for changes",
        tracked_ignore_files.len()
    );

    // State shared with handler
    let (binary_update_tx, binary_update_rx) = std::sync::mpsc::channel::<bool>();
    let binary_update_requested = Arc::new(AtomicBool::new(false));
    let state = Arc::new(WatcherState {
        repo_root: repo_root.clone(),
        tracked_ignores: tracked_ignore_files,
        binary_update_requested: binary_update_requested.clone(),
        binary_update_tx,
    });

    // Handler: trigger snapshot on any coalesced (throttled) event batch
    let handler_state = state.clone();
    let wx = Watchexec::new(move |mut action| {
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
    })
    .context("failed to create watchexec")?;

    // Configure watchexec
    configure_watchexec(&wx, &repo_root, filterer, debounce_ms);

    info!(path = %repo_root.display(), debounce_ms, "watching");
    let handle = wx.main();
    let result = match handle.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(e)) => Err(anyhow!("watchexec critical error: {e}")),
        Err(e) => Err(anyhow!("watchexec task join error: {e}")),
    };

    await_binary_update_and_maybe_exec(&binary_update_requested, &binary_update_rx);

    result
}

async fn build_filterer_and_ignores(
    repo_root: &PathBuf,
) -> Result<(IgnoreFilterer, HashSet<PathBuf>)> {
    // Build git-aware ignore filterer (project + environment), then add hard excludes
    let (mut origin_files, _errors1) = ignore_files::from_origin(repo_root.as_path()).await;
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
        .add_globs(&["/.git", "/.autosnap"], Some(repo_root))
        .map_err(|e| anyhow!("ignore hard excludes failed: {e}"))?;
    let filterer = IgnoreFilterer(filter);

    Ok((filterer, tracked_ignore_files))
}

fn configure_watchexec(
    wx: &Watchexec,
    repo_root: &Path,
    filterer: IgnoreFilterer,
    debounce_ms: u64,
) {
    wx.config.pathset([repo_root.to_path_buf()]);
    wx.config.filterer(filterer);
    wx.config.throttle(Duration::from_millis(debounce_ms));
    wx.config.on_error(|err: watchexec::ErrorHook| {
        tracing::error!("watchexec error: {}", err.error);
    });
}

fn handle_ignore_file_updates(paths: &[(&Path, Option<&FileType>)], state: &WatcherState) -> Flow {
    for (path, _file_type) in paths {
        if state.tracked_ignores.contains(*path) {
            info!("detected change to tracked ignore file: {}", path.display());

            // Final snapshot before restart
            if let Err(e) = gitlayer::snapshot_once(&state.repo_root, None) {
                error!(error = ?e, "pre-restart snapshot failed");
            }

            // Immediate exec to reload with fresh filters
            perform_exec();
            return Flow::Quit;
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
                info!("received shutdown signal");
                if let Err(e) = gitlayer::snapshot_once(&state.repo_root, None) {
                    error!(error = ?e, "final snapshot failed");
                } else {
                    info!("final snapshot created");
                }
                return Flow::Quit;
            }
            // SIGHUP: Reload (for future config reload implementation)
            Signal::Hangup => {
                info!("received SIGHUP - reload signal (not yet implemented)");
                // TODO: Reload configuration
            }
            // SIGUSR1: Force immediate snapshot
            Signal::User1 => {
                info!("received SIGUSR1 - forcing snapshot");
                let root = state.repo_root.clone();
                std::thread::spawn(move || {
                    if let Err(e) = gitlayer::snapshot_once(&root, None) {
                        error!(error = ?e, "forced snapshot failed");
                    } else {
                        info!("forced snapshot created");
                    }
                });
            }
            // SIGUSR2: Prepare for binary replacement (exec new binary)
            Signal::User2 => {
                info!("received SIGUSR2 - preparing for binary update");
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
        let root = state.repo_root.clone();
        std::thread::spawn(move || {
            if let Err(e) = gitlayer::snapshot_once(&root, None) {
                error!(error = ?e, "snapshot failed");
            } else {
                info!("snapshot created");
            }
        });
    }
}

fn request_binary_update(state: &WatcherState) {
    // Phase 1: Final snapshot
    if let Err(e) = gitlayer::snapshot_once(&state.repo_root, None) {
        error!(error = ?e, "pre-update snapshot failed");
    } else {
        info!("pre-update snapshot created");
    }

    // Phase 2: Start polling for binary change
    // Mark that we should wait for an update window on shutdown
    state.binary_update_requested.store(true, Ordering::SeqCst);
    let exe_path = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            error!("failed to get current exe: {}", e);
            return;
        }
    };

    // Get current binary metadata
    let original_metadata = exe_path.metadata().ok();

    // Spawn polling task
    let exe_for_poll = exe_path.clone();
    let tx_for_poll = state.binary_update_tx.clone();
    std::thread::spawn(move || {
        info!("waiting for binary to change at {}", exe_for_poll.display());

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
                            "binary changed (inode), ready to exec after {} ms",
                            (i + 1) * 500
                        );
                        let _ = tx_for_poll.send(true);
                        return;
                    }
                }

                if new_meta.modified().ok() != orig.modified().ok() {
                    info!(
                        "binary changed (mtime), ready to exec after {} ms",
                        (i + 1) * 500
                    );
                    let _ = tx_for_poll.send(true);
                    return;
                }
            }
        }

        warn!("binary unchanged after 15s timeout");
        let _ = tx_for_poll.send(false);
    });
}

fn await_binary_update_and_maybe_exec(requested: &AtomicBool, rx: &Receiver<bool>) {
    // Only wait for binary update window if SIGUSR2 was received
    if requested.load(Ordering::SeqCst)
        && let Ok(should_exec) = rx.recv_timeout(Duration::from_secs(16))
    {
        if should_exec {
            info!("binary update detected, performing exec");
            perform_exec();
            // If exec fails, we'll fall through
            warn!("exec failed after SIGUSR2, exiting normally");
        } else {
            warn!("binary update timeout, exiting without exec");
        }
    }
}
