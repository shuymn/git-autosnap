#![cfg(feature = "container-tests")]

use anyhow::Result;
use testcontainers::{clients::Cli, images::generic::GenericImage};

// Reuse helpers from tests/support/
#[path = "support/mod.rs"]
mod support;
use support::tc_exec::exec_bash;

#[test]
fn container_exec_echo() -> Result<()> {
    let docker = Cli::default();
    // Minimal image for a quick smoke test
    let image = GenericImage::new("alpine", "3.19");
    let container = docker.run(image);

    let out = exec_bash(&container, "echo hello")?;
    assert_eq!(out.trim(), "hello");
    Ok(())
}

