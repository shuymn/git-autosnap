use crate::config::AutosnapConfig;
use crate::process::{pid_file, status};
use anyhow::{Context, Result};
use libc;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};

/// Start the watcher in background (daemonize). Placeholder.
pub fn start_daemon(repo_root: &Path, _cfg: &AutosnapConfig) -> Result<()> {
    // If already running, report and exit
    if status(repo_root)? {
        println!("already running");
        return Ok(());
    }

    // Spawn a detached child running `start` (foreground) with stdio to /dev/null and new session
    let exe = std::env::current_exe().context("failed to get current executable")?;
    let mut cmd = Command::new(exe);
    unsafe {
        cmd.current_dir(repo_root)
            .arg("start")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .pre_exec(|| {
                // SAFETY: setsid creates a new session, detaching from controlling terminal
                libc::setsid();
                Ok(())
            });
    }

    let child = cmd.spawn().context("failed to spawn daemon child")?;
    println!("started (pid={})", child.id());
    Ok(())
}

/// Stop the running daemon via pidfile and signal. Placeholder.
pub fn stop(repo_root: &Path) -> Result<()> {
    let pid_path = pid_file(repo_root);
    if !pid_path.exists() {
        println!("stopped");
        return Ok(());
    }
    let pid = fs::read_to_string(&pid_path)
        .with_context(|| format!("failed to read {}", pid_path.display()))?;
    let pid = pid.trim();

    let status = Command::new("/bin/kill").arg("-TERM").arg(pid).status();
    match status {
        Ok(s) if s.success() => {
            println!("sent SIGTERM to {}", pid);
        }
        Ok(s) => {
            eprintln!("kill exited with status: {}", s);
        }
        Err(e) => {
            eprintln!("failed to execute kill: {}", e);
        }
    }

    // Wait briefly for shutdown and pidfile cleanup
    for _ in 0..20 {
        if !pid_path.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    Ok(())
}
