use std::path::Path;
use anyhow::Result;

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
        Self { debounce_ms: 200, prune_days: 60 }
    }
}

impl AutosnapConfig {
    /// Load configuration from git config with precedence: local → global → system.
    /// Currently returns defaults; to be implemented using `git2`.
    pub fn load(_repo_root: &Path) -> Result<Self> {
        // TODO: Implement reading from git config using `git2` with proper precedence.
        Ok(Self::default())
    }
}

