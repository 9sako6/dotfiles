use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;

#[test]
fn removed_source_interfaces_explain_migration() {
    let temp = tempfile::tempdir().unwrap();
    fs::write(temp.path().join("flake.nix"), "{}").unwrap();
    for argument in ["--default", "https://example.invalid/private.git"] {
        cargo_bin_cmd!()
            .env("DOTFILES_DIR", temp.path())
            .args(["plan", argument])
            .assert()
            .failure()
            .stderr(predicate::str::contains("dotfiles.local.toml"));
    }
}

#[test]
fn complete_copy_uses_the_frozen_source_and_preserves_unowned_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home/.claude/skills")).unwrap();
    fs::create_dir_all(home.join(".claude/skills")).unwrap();
    fs::write(source.join("home/.claude/skills/new"), "frozen").unwrap();
    fs::write(home.join(".claude/skills/old"), "old").unwrap();
    fs::write(home.join(".claude/history"), "keep").unwrap();
    let paths = temp.path().join("paths.json");
    fs::write(&paths, "[\".claude/skills\"]").unwrap();
    cargo_bin_cmd!()
        .arg("complete-apply")
        .arg(source)
        .arg(paths)
        .arg(&home)
        .assert()
        .success();
    assert_eq!(
        fs::read_to_string(home.join(".claude/skills/new")).unwrap(),
        "frozen"
    );
    assert!(!home.join(".claude/skills/old").exists());
    assert_eq!(
        fs::read_to_string(home.join(".claude/history")).unwrap(),
        "keep"
    );
}
