use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn version_spellings_report_the_build_without_a_checkout_or_external_tools() {
    let directory = tempfile::tempdir().unwrap();
    for argument in ["version", "--version", "-V"] {
        cargo_bin_cmd!()
            .arg(argument)
            .current_dir(directory.path())
            .env("DOTFILES_DIR", "invalid-relative-path")
            .env("DOTFILES_BUILD_REVISION", "runtime-value-must-not-be-used")
            .env_remove("HOME")
            .env("PATH", "")
            .assert()
            .success()
            .stdout(concat!("dotfiles ", env!("DOTFILES_BUILD_REVISION"), "\n"))
            .stderr("");
    }
}

#[test]
fn help_lists_help_and_version_without_a_checkout() {
    let mut expected = None;
    for argument in ["help", "--help", "-h"] {
        let output = cargo_bin_cmd!()
            .arg(argument)
            .env("DOTFILES_DIR", "invalid-relative-path")
            .assert()
            .success()
            .stdout(predicate::str::contains("help").and(predicate::str::contains("version")))
            .stderr("")
            .get_output()
            .stdout
            .clone();
        assert_eq!(*expected.get_or_insert_with(|| output.clone()), output);
    }
}

#[test]
fn help_can_describe_version_and_nested_commands() {
    for (arguments, usage) in [
        (vec!["help", "version"], "Usage: dotfiles version"),
        (vec!["version", "--help"], "Usage: dotfiles version"),
        (
            vec!["help", "agents", "build"],
            "Usage: dotfiles agents build",
        ),
    ] {
        cargo_bin_cmd!()
            .args(arguments)
            .env("DOTFILES_DIR", "invalid-relative-path")
            .assert()
            .success()
            .stdout(predicate::str::contains(usage))
            .stderr("");
    }
}

#[test]
fn version_rejects_unexpected_arguments() {
    cargo_bin_cmd!()
        .args(["version", "unexpected"])
        .assert()
        .code(2)
        .stdout("")
        .stderr(predicate::str::contains("unexpected argument"));
}
