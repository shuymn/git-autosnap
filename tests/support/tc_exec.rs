#![cfg(feature = "container-tests")]

// Drop-in helpers for executing commands inside testcontainers.
// Requires dev-deps when running with `--features container-tests`:
// - anyhow = "1"
// - shell-escape = "0.1"
// - testcontainers = "<your-version>"

use anyhow::{bail, Context, Result};
use shell_escape::unix::escape;
use testcontainers::{Container, Image};

#[cfg(feature = "container-tests")]
pub fn exec_bash<I: Image>(c: &Container<'_, I>, cmd: &str) -> Result<String> {
    // Adjust to your testcontainers version if fields differ
    let out = c.exec(vec!["bash", "-lc", cmd]);
    if out.exit_code != 0 {
        let stderr = String::from_utf8_lossy(&out.stderr);
        bail!("command failed (code {}): {}", out.exit_code, stderr);
    }
    Ok(String::from_utf8(out.stdout).context("invalid utf8 on stdout")?)
}

#[cfg(feature = "container-tests")]
pub fn exec_in<I: Image>(c: &Container<'_, I>, cwd: &str, cmd: &str) -> Result<String> {
    let script = format!("cd {} && {}", escape(cwd.into()), cmd);
    exec_bash(c, &script)
}

// Async variants (if using async runners). Keep here for reference; gate as needed.
// use testcontainers::core::ExecCommand;
// pub async fn exec_bash_async<I: Image>(c: &Container<'_, I>, cmd: &str) -> Result<String> {
//     let out = c
//         .exec(ExecCommand { cmd: vec!["bash".into(), "-lc".into(), cmd.into()], ..Default::default() })
//         .await
//         .context("container exec failed")?;
//     if out.exit_code != 0 {
//         let stderr = String::from_utf8_lossy(&out.stderr);
//         bail!("command failed (code {}): {}", out.exit_code, stderr);
//     }
//     Ok(String::from_utf8_lossy(&out.stdout).into_owned())
// }
// pub async fn exec_in_async<I: Image>(c: &Container<'_, I>, cwd: &str, cmd: &str) -> Result<String> {
//     let script = format!("cd {} && {}", escape(cwd.into()), cmd);
//     exec_bash_async(c, &script).await
// }

