use assert_cmd::Command;
use predicates::prelude::{PredicateBooleanExt, predicate};

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("git-autosnap").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage").or(predicate::str::contains("USAGE")));
}
