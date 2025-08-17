use crate::config::AutosnapConfig;
use crate::gitlayer;
use crate::process;
use anyhow::{Context, Result, anyhow};
use std::collections::HashSet;
use std::path::Path;
use tracing::{error, info, warn};

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
    gitlayer::init_autosnap(repo_root).ok(); // ensure exists; ignore if already present
    // Acquire single-instance lock and write pid
    let _guard = process::acquire_lock(repo_root)?;

    // Run watchexec on a Tokio runtime to handle async APIs.
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .context("failed to build tokio runtime")?;

    let repo_root = repo_root.to_path_buf();
    let debounce_ms = cfg.debounce_ms;

    rt.block_on(async move {
        use std::time::Duration;
        use watchexec::Watchexec;
        use watchexec_filterer_ignore::IgnoreFilterer;

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

        let mut filter = ignore_files::IgnoreFilter::new(&repo_root, &origin_files)
            .await
            .map_err(|e| anyhow!("ignore filter build failed: {e}"))?;
        filter
            .add_globs(&["/.git", "/.autosnap"], Some(&repo_root))
            .map_err(|e| anyhow!("ignore hard excludes failed: {e}"))?;
        let filterer = IgnoreFilterer(filter);

        info!(
            "tracking {} ignore files for changes",
            tracked_ignore_files.len()
        );

        // Handler: trigger snapshot on any coalesced (throttled) event batch
        let root_for_handler = repo_root.clone();
        let ignore_files_for_handler = tracked_ignore_files.clone();
        let (binary_update_tx, binary_update_rx) = std::sync::mpsc::channel::<bool>();

        let wx = Watchexec::new(move |mut action| {
            // Check if any changed path is an ignore file we're tracking
            let paths: Vec<_> = action.paths().collect();
            for (path, _file_type) in &paths {
                if ignore_files_for_handler.contains(*path) {
                    info!("detected change to tracked ignore file: {}", path.display());

                    // Final snapshot before restart
                    if let Err(e) = gitlayer::snapshot_once(&root_for_handler, None) {
                        error!(error = ?e, "pre-restart snapshot failed");
                    }

                    // Immediate exec to reload with fresh filters
                    perform_exec();
                    // Falls through to quit if exec fails
                    action.quit();
                    return action;
                }
            }

            // Handle signals - collect them first to avoid borrowing issues
            let signals: Vec<_> = action.signals().collect();
            for signal in signals {
                use watchexec_signals::Signal;
                match signal {
                    // SIGTERM, SIGINT: Graceful shutdown with final snapshot
                    Signal::Terminate | Signal::Interrupt => {
                        info!("received shutdown signal");
                        if let Err(e) = gitlayer::snapshot_once(&root_for_handler, None) {
                            error!(error = ?e, "final snapshot failed");
                        } else {
                            info!("final snapshot created");
                        }
                        action.quit();
                        return action;
                    }
                    // SIGHUP: Reload (for future config reload implementation)
                    Signal::Hangup => {
                        info!("received SIGHUP - reload signal (not yet implemented)");
                        // TODO: Reload configuration
                    }
                    // SIGUSR1: Force immediate snapshot
                    Signal::User1 => {
                        info!("received SIGUSR1 - forcing snapshot");
                        let root = root_for_handler.clone();
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

                        // Phase 1: Final snapshot
                        if let Err(e) = gitlayer::snapshot_once(&root_for_handler, None) {
                            error!(error = ?e, "pre-update snapshot failed");
                        } else {
                            info!("pre-update snapshot created");
                        }

                        // Phase 2: Start polling for binary change
                        let exe_path = match std::env::current_exe() {
                            Ok(p) => p,
                            Err(e) => {
                                error!("failed to get current exe: {}", e);
                                action.quit();
                                return action;
                            }
                        };

                        // Get current binary metadata
                        let original_metadata = exe_path.metadata().ok();

                        // Spawn polling task
                        let exe_for_poll = exe_path.clone();
                        let tx_for_poll = binary_update_tx.clone();
                        std::thread::spawn(move || {
                            use std::time::Duration;

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

                        // Stop watcher to prepare for exec
                        action.quit();
                        return action;
                    }
                    _ => {
                        // Ignore other signals
                    }
                }
            }

            // Snapshot on any events with paths
            if !paths.is_empty() {
                let root = root_for_handler.clone();
                std::thread::spawn(move || {
                    if let Err(e) = gitlayer::snapshot_once(&root, None) {
                        error!(error = ?e, "snapshot failed");
                    } else {
                        info!("snapshot created");
                    }
                });
            }

            action
        })
        .context("failed to create watchexec")?;

        // Configure watchexec
        wx.config.pathset([repo_root.clone()]);
        wx.config.filterer(filterer);
        wx.config.throttle(Duration::from_millis(debounce_ms));
        wx.config.on_error(|err: watchexec::ErrorHook| {
            tracing::error!("watchexec error: {}", err.error);
        });

        info!(path = %repo_root.display(), debounce_ms, "watching");
        let handle = wx.main();
        let result = match handle.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(anyhow!("watchexec critical error: {e}")),
            Err(e) => Err(anyhow!("watchexec task join error: {e}")),
        };

        // Check if we have a pending binary update
        if let Ok(should_exec) = binary_update_rx.recv_timeout(std::time::Duration::from_secs(16)) {
            if should_exec {
                info!("binary update detected, performing exec");
                perform_exec();
                // If exec fails, we'll fall through to return the result
                warn!("exec failed after SIGUSR2, exiting normally");
            } else {
                warn!("binary update timeout, exiting without exec");
            }
        }

        result
    })
}
