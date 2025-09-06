use std::{
    fs,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
};

use anyhow::{Context, Result};
use nix::{
    sys::signal::{self, Signal},
    unistd::{self, Pid},
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
    cmd.current_dir(repo_root)
        .arg("start")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // SAFETY:
    // - `pre_exec` runs in the child process after `fork()` and before `exec()`. The closure must
    //   only perform async-signal-safe operations. We call `setsid()` via `nix::unistd::setsid`,
    //   which corresponds to the async-signal-safe libc `setsid(2)` to detach into a new session.
    // - The closure does not capture or touch external state, perform allocations, or invoke
    //   non–async-signal-safe functions. Converting the errno to `io::Error` with
    //   `from_raw_os_error` is a simple value construction used only when propagating an error.
    unsafe {
        cmd.pre_exec(|| {
            // Detach from controlling terminal: create a new session via setsid()
            unistd::setsid()
                .map(|_| ())
                .map_err(|e| std::io::Error::from_raw_os_error(e as i32))
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
