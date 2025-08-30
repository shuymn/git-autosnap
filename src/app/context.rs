use std::path::PathBuf;

use anyhow::Result;

use crate::config::AutosnapConfig;

#[derive(Debug, Clone)]
pub struct AppContext {
    pub repo_root: PathBuf,
    pub cfg: AutosnapConfig,
    pub verbosity: u8,
}

impl AppContext {
    pub fn new(repo_root: PathBuf, cfg: AutosnapConfig, verbosity: u8) -> Self {
        Self {
            repo_root,
            cfg,
            verbosity,
        }
    }

    /// Convenience constructor loading config from repo root.
    pub fn from_repo(verbosity: u8) -> Result<Self> {
        let root = crate::core::git::repo_root()?;
        let cfg = crate::config::AutosnapConfig::load(&root)?;
        Ok(Self::new(root, cfg, verbosity))
    }
}
