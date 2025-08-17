use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use anyhow::{anyhow, Context, Result};
use fs2::FileExt;

/// Path to the PID file inside `.autosnap`.
pub fn pid_file(repo_root: &Path) -> PathBuf {
    repo_root.join(".autosnap").join("autosnap.pid")
}

/// Returns true if a daemon appears to be running (pidfile exists and pid is alive).
pub fn status(repo_root: &Path) -> Result<bool> {
    let pid_path = pid_file(repo_root);
    if !pid_path.exists() {
        return Ok(false);
    }
    let pid = read_pidfile(&pid_path)?;
    Ok(pid.map_or(false, |p| is_pid_alive(p)))
}

/// Remove `.autosnap` directory after stopping the daemon.
/// Placeholder implementation.
pub fn uninstall(repo_root: &Path) -> Result<()> {
    let dir = repo_root.join(".autosnap");
    if dir.exists() {
        fs::remove_dir_all(&dir)?;
        println!("Removed {}", dir.display());
    } else {
        println!("Nothing to remove at {}", dir.display());
    }
    Ok(())
}

/// Guard that holds an exclusive lock on the pidfile for the process lifetime.
pub struct PidGuard {
    file: File,
    path: PathBuf,
}

impl Drop for PidGuard {
    fn drop(&mut self) {
        let _ = self.file.unlock();
        // Best-effort remove pidfile on drop
        let _ = fs::remove_file(&self.path);
    }
}

/// Acquire single-instance lock and write the current pid into the pidfile.
pub fn acquire_lock(repo_root: &Path) -> Result<PidGuard> {
    let pid_path = pid_file(repo_root);
    if let Some(parent) = pid_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(&pid_path)
        .with_context(|| format!("failed to open {}", pid_path.display()))?;

    // Try to lock; if already locked, report as already running
    if let Err(e) = file.try_lock_exclusive() {
        // Attempt to read pid to include in message
        let pid = read_pid_from_file(&file).unwrap_or(None);
        let pid_str = pid.map(|p| p.to_string()).unwrap_or_else(|| "unknown".into());
        return Err(anyhow!("autosnap already running (pid={pid_str}): {e}"));
    }

    // Truncate and write pid
    file.set_len(0)?;
    let pid = std::process::id();
    writeln!(&file, "{}", pid)?;

    // Permissions 0600
    let mut perms = file.metadata()?.permissions();
    perms.set_mode(0o600);
    fs::set_permissions(&pid_path, perms)?;

    Ok(PidGuard { file, path: pid_path })
}

fn read_pid_from_file(file: &File) -> Result<Option<i32>> {
    let mut buf = String::new();
    let mut f = file;
    f.read_to_string(&mut buf).ok();
    parse_pid(&buf)
}

fn read_pidfile(path: &Path) -> Result<Option<i32>> {
    let content = fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    parse_pid(&content)
}

fn parse_pid(s: &str) -> Result<Option<i32>> {
    let t = s.trim();
    if t.is_empty() {
        return Ok(None);
    }
    let p: i32 = t.parse().with_context(|| format!("invalid pid in pidfile: {t}"))?;
    Ok(Some(p))
}

fn is_pid_alive(pid: i32) -> bool {
    // Use /bin/kill -0 to test for liveness
    std::process::Command::new("/bin/kill")
        .arg("-0")
        .arg(pid.to_string())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
