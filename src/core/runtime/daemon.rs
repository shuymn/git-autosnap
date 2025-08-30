use std::{
    fs,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use libc;
use nix::{
    sys::signal::{self, Signal},
    unistd::Pid,
};

use crate::{
    config::AutosnapConfig,
    core::runtime::process::{pid_file, status},
};

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
    let pid_str = fs::read_to_string(&pid_path)
        .with_context(|| format!("failed to read {}", pid_path.display()))?;
    let pid_num: i32 = pid_str
        .trim()
        .parse()
        .with_context(|| format!("invalid PID in file: {}", pid_str.trim()))?;

    // Send SIGTERM using nix for type-safe signal handling
    let pid = Pid::from_raw(pid_num);
    match signal::kill(pid, Signal::SIGTERM) {
        Ok(()) => {
            println!("sent SIGTERM to {}", pid_num);
        }
        Err(e) => {
            eprintln!("failed to send signal: {}", e);
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
