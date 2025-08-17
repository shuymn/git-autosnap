#![cfg(feature = "container-tests")]

// Drop-in helpers for executing commands inside testcontainers.
// Compatible with testcontainers v0.25.0 using async API
// Requires dev-deps when running with `--features container-tests`:
// - anyhow = "1"
// - shell-escape = "0.1"
// - testcontainers = "0.25"
// - tokio = { version = "1", features = ["macros", "rt-multi-thread"] }

use anyhow::{Context, Result, bail};
use shell_escape::unix::escape;
use testcontainers::{ContainerAsync, Image, core::ExecCommand};

/// Execute a bash command in a container and return stdout
/// Uses the async API
pub async fn exec_bash<I: Image>(c: &ContainerAsync<I>, cmd: &str) -> Result<String> {
    let exec_cmd = ExecCommand::new(["bash", "-lc", cmd]);
    let mut result = c.exec(exec_cmd).await.context("container exec failed")?;

    // Read stdout and stderr first before checking exit code
    // This ensures the command has completed
    let stdout = result.stdout_to_vec().await?;
    let stderr = result.stderr_to_vec().await?;

    // Now check the exit code
    let exit_code = result.exit_code().await?;
    if exit_code != Some(0) {
        let stderr_str = String::from_utf8_lossy(&stderr);
        bail!("command failed (code {:?}): {}", exit_code, stderr_str);
    }

    Ok(String::from_utf8(stdout).context("invalid utf8 on stdout")?)
}

/// Execute a command in a specific directory
pub async fn exec_in<I: Image>(c: &ContainerAsync<I>, cwd: &str, cmd: &str) -> Result<String> {
    let script = format!("cd {} && {}", escape(cwd.into()), cmd);
    exec_bash(c, &script).await
}
