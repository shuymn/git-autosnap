use std::{
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use fs2::FileExt;
use tracing::debug;

/// Path to the lock that serializes snapshot/compact git writes.
#[must_use]
pub fn ops_lock_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".autosnap").join("autosnap.ops.lock")
}

/// Guard that holds the autosnap operations lock.
pub struct OpsLockGuard {
    file: File,
}

impl Drop for OpsLockGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

/// Acquire an exclusive lock used to serialize snapshot and compact operations.
///
/// This call blocks until the lock is available.
///
/// # Errors
/// Returns an error if the lock file cannot be opened or locked.
pub fn acquire_ops_lock(repo_root: &Path) -> Result<OpsLockGuard> {
    let path = ops_lock_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create autosnap dir at {}", parent.display()))?;
    }

    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .with_context(|| format!("failed to open lock file {}", path.display()))?;

    debug!(path = %path.display(), "waiting for autosnap ops lock");
    file.lock_exclusive()
        .with_context(|| format!("failed to acquire lock {}", path.display()))?;

    Ok(OpsLockGuard { file })
}

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, thread, time::Duration};

    use super::*;

    #[test]
    fn lock_blocks_until_released() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let guard = acquire_ops_lock(tmp.path()).expect("first lock");

        let (tx, rx) = mpsc::channel();
        let repo = tmp.path().to_path_buf();
        let handle = thread::spawn(move || {
            let _guard = acquire_ops_lock(&repo).expect("second lock");
            tx.send(()).expect("send");
        });

        // The second lock should still be blocked.
        assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());

        drop(guard);

        // After releasing the first lock, the second must proceed.
        rx.recv_timeout(Duration::from_secs(2))
            .expect("second lock must proceed after release");
        handle.join().expect("thread join");
    }
}
