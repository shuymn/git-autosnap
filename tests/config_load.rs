use assert_cmd::Command;
use tempfile::tempdir;
use git_autosnap::config::AutosnapConfig;

#[test]
fn config_loads_from_repo_config() {
    let td = tempdir().unwrap();
    let root = td.path();

    // Init a real git repo to have a .git/config
    let mut cmd = Command::new("git");
    cmd.current_dir(root).args(["init"]);
    cmd.assert().success();

    // Set our autosnap.* keys
    let mut cmd = Command::new("git");
    cmd.current_dir(root).args(["config", "autosnap.debounce-ms", "321"]);
    cmd.assert().success();
    let mut cmd = Command::new("git");
    cmd.current_dir(root).args(["config", "autosnap.gc.prune-days", "5"]);
    cmd.assert().success();

    let cfg = AutosnapConfig::load(root).expect("load config");
    assert_eq!(cfg.debounce_ms, 321);
    assert_eq!(cfg.prune_days, 5);
}
