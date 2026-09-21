use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::os::unix::fs::PermissionsExt;

#[test]
fn complete_copy_uses_the_frozen_source_and_preserves_unowned_files() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home/shared/skills")).unwrap();
    fs::create_dir_all(home.join(".claude/skills")).unwrap();
    fs::write(source.join("home/shared/skills/new"), "frozen").unwrap();
    fs::write(home.join(".claude/skills/old"), "old").unwrap();
    fs::write(home.join(".claude/history"), "keep").unwrap();
    let paths = temp.path().join("paths.json");
    fs::write(&paths, r#"{".claude/skills": "shared/skills"}"#).unwrap();
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

#[test]
fn shared_source_is_copied_to_each_declared_destination() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home")).unwrap();
    fs::create_dir_all(home.join(".pi/agent")).unwrap();
    fs::write(home.join(".pi/agent/settings.json"), "runtime").unwrap();
    let paths = temp.path().join("paths.json");
    fs::write(
        &paths,
        r#"{".pi/agent/AGENTS.md": "AGENTS.md", "custom/INSTRUCTIONS.md": "AGENTS.md"}"#,
    )
    .unwrap();
    for content in ["instructions", "updated instructions"] {
        fs::write(source.join("home/AGENTS.md"), content).unwrap();
        cargo_bin_cmd!()
            .arg("complete-apply")
            .arg(&source)
            .arg(&paths)
            .arg(&home)
            .assert()
            .success();
        for target in [".pi/agent/AGENTS.md", "custom/INSTRUCTIONS.md"] {
            let destination = home.join(target);
            assert_eq!(fs::read_to_string(&destination).unwrap(), content);
            assert!(!destination.is_symlink());
        }
        assert_eq!(
            fs::read_to_string(home.join(".pi/agent/settings.json")).unwrap(),
            "runtime"
        );
    }
}

#[test]
fn invalid_mapping_fails_before_changing_destinations() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home")).unwrap();
    fs::create_dir_all(&home).unwrap();
    fs::write(source.join("home/instructions"), "new").unwrap();
    fs::write(home.join("a"), "existing").unwrap();
    let paths = temp.path().join("paths.json");
    for invalid in [
        r#"{"a": "instructions", "b": "missing"}"#,
        r#"{"a": "instructions", "b": "../outside"}"#,
        r#"{"a": "instructions", "../outside": "instructions"}"#,
        r#"{"a": "instructions", "a/b": "instructions"}"#,
    ] {
        fs::write(&paths, invalid).unwrap();
        cargo_bin_cmd!()
            .arg("complete-apply")
            .arg(&source)
            .arg(&paths)
            .arg(&home)
            .assert()
            .failure();
        assert_eq!(fs::read_to_string(home.join("a")).unwrap(), "existing");
    }
}

#[test]
fn repeated_copy_from_readonly_sources_preserves_owner_write_and_execute_permissions() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    fs::create_dir_all(source.join("home/mybin")).unwrap();
    fs::create_dir_all(&home).unwrap();
    for (path, mode) in [(".gitconfig", 0o444), ("mybin/tool", 0o555)] {
        let source = source.join("home").join(path);
        fs::write(&source, "frozen").unwrap();
        fs::set_permissions(source, fs::Permissions::from_mode(mode)).unwrap();
    }
    let backup = home.join(".gitconfig.pre-home-manager");
    fs::write(&backup, "original backup").unwrap();
    fs::set_permissions(&backup, fs::Permissions::from_mode(0o444)).unwrap();
    let paths = temp.path().join("paths.json");
    fs::write(&paths, r#"{".gitconfig": ".gitconfig", "mybin": "mybin"}"#).unwrap();
    for _ in 0..2 {
        cargo_bin_cmd!()
            .arg("complete-apply")
            .arg(&source)
            .arg(&paths)
            .arg(&home)
            .assert()
            .success()
            .stdout("");
        for (path, mode) in [(".gitconfig", 0o644), ("mybin/tool", 0o755)] {
            let destination = home.join(path);
            assert!(!destination.is_symlink());
            assert_eq!(fs::read_to_string(&destination).unwrap(), "frozen");
            assert_eq!(
                fs::metadata(destination).unwrap().permissions().mode() & 0o777,
                mode
            );
        }
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original backup");
        assert_eq!(
            fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
            0o444
        );
    }
}
