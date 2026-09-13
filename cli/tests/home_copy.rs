use std::fs;

use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;

#[test]
fn rejects_invalid_toml_before_running_system_commands() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let home = temp.path().join("home");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join("flake.nix"), "{}\n").unwrap();

    for (raw, diagnostic) in [
        ("copy = [", "invalid TOML config"),
        ("copi = []", "unknown field"),
        ("copy = []\ncopy = []", "duplicate key"),
        ("copy = ['../secret']", "invalid copy entry"),
    ] {
        fs::write(repo.join("dotfiles.toml"), raw).unwrap();
        for command in ["plan", "apply"] {
            cargo_bin_cmd!()
                .env_clear()
                .env("DOTFILES_DIR", &repo)
                .env("HOME", &home)
                .arg(command)
                .assert()
                .failure()
                .stdout("")
                .stderr(
                    predicate::str::contains("dotfiles.toml")
                        .and(predicate::str::contains(diagnostic)),
                );
            assert!(!home.exists());
        }
    }
}
