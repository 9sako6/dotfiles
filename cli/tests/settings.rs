use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn settings_accepts_no_filter_or_output_options() {
    for argument in ["--all", "--json", "--json-extended", "--local", "localllm"] {
        cargo_bin_cmd!()
            .args(["settings", argument])
            .assert()
            .code(2)
            .stdout("")
            .stderr(predicate::str::contains("unexpected argument"));
    }
}
