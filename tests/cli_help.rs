use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::{PredicateBooleanExt, predicate};

#[test]
fn prints_help() {
    let has_compact = predicate::str::is_match(r"(?m)^\s*compact\b")
        .expect("valid regex for compact")
        .and(
            predicate::str::is_match(r"(?m)^\s*gc\b")
                .expect("valid regex for gc")
                .not(),
        );

    let mut cmd = cargo_bin_cmd!("git-autosnap");
    cmd.arg("--help").assert().success().stdout(
        predicate::str::contains("Usage")
            .or(predicate::str::contains("USAGE"))
            .and(has_compact),
    );
}
