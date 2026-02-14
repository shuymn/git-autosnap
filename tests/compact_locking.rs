#![cfg(feature = "container-tests")]
#![allow(clippy::future_not_send)]

use anyhow::{Context, Result};
use testcontainers::{GenericImage, core::WaitFor, runners::AsyncRunner};

#[path = "support/mod.rs"]
mod support;
use support::tc_exec::{exec_bash, exec_in};

fn parse_millis(output: &str) -> Result<u64> {
    output
        .trim()
        .parse::<u64>()
        .with_context(|| format!("failed to parse elapsed milliseconds: {output}"))
}

#[tokio::test]
async fn compact_waits_for_existing_ops_lock() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;
    exec_in(&container, "/repo", "echo initial > file.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    let elapsed_output = exec_in(
        &container,
        "/repo",
        r#"bash -lc '
set -euo pipefail
rm -f /tmp/ops_lock_ready
(
  exec 9>.autosnap/autosnap.ops.lock
  flock -x 9
  echo ready > /tmp/ops_lock_ready
  sleep 2
) &
holder=$!
for _ in $(seq 1 40); do
  [ -f /tmp/ops_lock_ready ] && break
  sleep 0.05
done
start=$(date +%s%3N)
git autosnap compact --days 1 >/tmp/compact-output.txt
end=$(date +%s%3N)
wait "$holder"
echo $((end-start))
'"#,
    )
    .await?;

    let elapsed = parse_millis(&elapsed_output)?;
    assert!(
        elapsed >= 1500,
        "compact should wait for lock (elapsed={elapsed}ms)"
    );

    Ok(())
}

#[tokio::test]
async fn snapshot_waits_for_existing_ops_lock() -> Result<()> {
    let image = GenericImage::new("git-autosnap-test", "latest")
        .with_wait_for(WaitFor::message_on_stdout("ready"));
    let container = image.start().await?;

    exec_bash(&container, "mkdir -p /repo && git init /repo").await?;
    exec_in(&container, "/repo", "git autosnap init").await?;
    exec_in(&container, "/repo", "echo initial > file.txt").await?;
    exec_in(&container, "/repo", "git autosnap once").await?;

    let elapsed_output = exec_in(
        &container,
        "/repo",
        r#"bash -lc '
set -euo pipefail
echo changed > file.txt
rm -f /tmp/ops_lock_ready
(
  exec 9>.autosnap/autosnap.ops.lock
  flock -x 9
  echo ready > /tmp/ops_lock_ready
  sleep 2
) &
holder=$!
for _ in $(seq 1 40); do
  [ -f /tmp/ops_lock_ready ] && break
  sleep 0.05
done
start=$(date +%s%3N)
git autosnap once >/tmp/once-output.txt
end=$(date +%s%3N)
wait "$holder"
test -s /tmp/once-output.txt
echo $((end-start))
'"#,
    )
    .await?;

    let elapsed = parse_millis(&elapsed_output)?;
    assert!(
        elapsed >= 1500,
        "snapshot should wait for lock (elapsed={elapsed}ms)"
    );

    Ok(())
}
