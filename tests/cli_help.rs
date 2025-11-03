use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::{PredicateBooleanExt, predicate};

#[test]
fn prints_help() {
    let mut cmd = cargo_bin_cmd!("git-autosnap");
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage").or(predicate::str::contains("USAGE")));
}
