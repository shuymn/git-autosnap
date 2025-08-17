use crate::config::AutosnapConfig;
use crate::gitlayer;
use crate::process;
use anyhow::{Context, Result, anyhow};
use std::path::Path;
use tracing::{error, info};

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
        let has_project_excludes = origin_files.iter().any(|f| f.applies_in.is_none());
        if !has_project_excludes {
            origin_files.extend(env_files);
        }

        let mut filter = ignore_files::IgnoreFilter::new(&repo_root, &origin_files)
            .await
            .map_err(|e| anyhow!("ignore filter build failed: {e}"))?;
        filter
            .add_globs(&["/.git", "/.autosnap"], Some(&repo_root))
            .map_err(|e| anyhow!("ignore hard excludes failed: {e}"))?;
        let filterer = IgnoreFilterer(filter);

        // Handler: trigger snapshot on any coalesced (throttled) event batch
        let root_for_handler = repo_root.clone();
        let wx = Watchexec::new(move |mut action| {
            // Quit on interrupt/terminate
            if action.signals().next().is_some() {
                action.quit();
                return action;
            }

            // Snapshot on any events with paths
            if action.paths().next().is_some() {
                let root = root_for_handler.clone();
                std::thread::spawn(move || {
                    if let Err(e) = gitlayer::snapshot_once(&root) {
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
        match handle.await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(anyhow!("watchexec critical error: {e}")),
            Err(e) => Err(anyhow!("watchexec task join error: {e}")),
        }
    })
}
