use std::path::Path;

use anyhow::{Context, Result};
use git2::Repository;

/// Autosnap configuration values sourced from git config.
#[derive(Debug, Clone, Copy)]
pub struct AutosnapConfig {
    /// Debounce window in milliseconds for the file watcher.
    pub debounce_ms: u64,
    /// Prune retention in days for `gc`.
    pub prune_days: u32,
}

impl Default for AutosnapConfig {
    fn default() -> Self {
        Self {
            debounce_ms: 1000,
            prune_days: 60,
        }
    }
}

impl AutosnapConfig {
    /// Load configuration from git config with precedence: local → global → system.
    /// Currently returns defaults; to be implemented using `git2`.
    ///
    /// # Errors
    /// Returns an error if repository discovery or reading config fails.
    pub fn load(repo_root: &Path) -> Result<Self> {
        // Discover the repository from repo_root; fall back to defaults if discovery fails
        let repo = Repository::discover(repo_root).with_context(|| {
            format!(
                "failed to discover Git repository from {}",
                repo_root.display()
            )
        })?;

        let cfg = repo.config().context("failed to open git config")?;

        let mut out = Self::default();

        if let Ok(v) = cfg.get_i64("autosnap.debounce-ms")
            && v >= 0
            && let Ok(vu) = u64::try_from(v)
        {
            out.debounce_ms = vu;
        }
        if let Ok(v) = cfg.get_i64("autosnap.gc.prune-days")
            && v >= 0
            && let Ok(vu) = u32::try_from(v)
        {
            out.prune_days = vu;
        }

        Ok(out)
    }
}
