use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn prints_help() {
    let mut cmd = Command::cargo_bin("git-autosnap").unwrap();
    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage").or(predicate::str::contains("USAGE")));
}

#[test]
fn sum_works() {
    let mut cmd = Command::cargo_bin("git-autosnap").unwrap();
    cmd.args(["sum", "1", "2", "3"])
        .assert()
        .success()
        .stdout("6\n");
}
