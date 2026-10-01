use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::os::unix::fs::PermissionsExt;

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
        .env("XDG_STATE_HOME", temp.path().join("state"))
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
    fs::write(&paths, "[\".gitconfig\", \"mybin\"]").unwrap();
    for _ in 0..2 {
        cargo_bin_cmd!()
            .arg("complete-apply")
            .arg(&source)
            .arg(&paths)
            .arg(&home)
            .env("XDG_STATE_HOME", temp.path().join("state"))
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

struct Fixture {
    root: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let fixture = Self {
            root: tempfile::tempdir().unwrap(),
        };
        fs::create_dir_all(fixture.path("source/home")).unwrap();
        fs::create_dir_all(fixture.path("home")).unwrap();
        fixture
    }

    fn path(&self, relative: &str) -> std::path::PathBuf {
        self.root.path().join(relative)
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.path(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn apply(&self, paths: &[&str]) -> assert_cmd::assert::Assert {
        fs::write(self.path("paths.json"), serde_json::to_vec(paths).unwrap()).unwrap();
        cargo_bin_cmd!()
            .arg("complete-apply")
            .args([
                self.path("source"),
                self.path("paths.json"),
                self.path("home"),
            ])
            .env("XDG_STATE_HOME", self.path("state"))
            .assert()
    }

    fn recorded(&self) -> serde_json::Value {
        serde_json::from_slice(&fs::read(self.path("state/dotfiles/home.json")).unwrap()).unwrap()
    }
}

#[test]
fn removed_declarations_clean_up_only_unchanged_successful_copies() {
    use std::os::unix::fs::symlink;

    let fixture = Fixture::new();
    fixture.write("source/home/changed/old", "owned");
    fixture.write("source/home/removed/old", "owned");
    fixture.write("source/home/replaced", "owned");
    fixture.write("home/unmanaged", "keep");
    fixture.apply(&["changed", "removed", "replaced"]).success();
    fixture.write("home/changed/runtime", "keep");
    fs::remove_file(fixture.path("home/replaced")).unwrap();
    fixture.write("home-manager-source", "new owner");
    symlink(
        fixture.path("home-manager-source"),
        fixture.path("home/replaced"),
    )
    .unwrap();

    fixture.apply(&[]).success();
    assert!(!fixture.path("home/removed").exists());
    assert_eq!(
        fs::read_to_string(fixture.path("home/changed/runtime")).unwrap(),
        "keep"
    );
    assert_eq!(
        fs::read_to_string(fixture.path("home/unmanaged")).unwrap(),
        "keep"
    );
    assert!(fixture.path("home/replaced").is_symlink());
    assert_eq!(
        fs::read_to_string(fixture.path("home-manager-source")).unwrap(),
        "new owner"
    );
    assert!(fixture.recorded()["copies"].as_object().unwrap().is_empty());
    fixture.apply(&[]).success();
    assert!(fixture.path("home/changed/old").exists());
}

#[test]
fn missing_or_invalid_state_never_infers_ownership() {
    let fixture = Fixture::new();
    fixture.write("source/home/managed", "new");
    fixture.write("home/unmanaged", "keep");
    fixture.apply(&[]).success();
    assert_eq!(
        fs::read_to_string(fixture.path("home/unmanaged")).unwrap(),
        "keep"
    );
    fixture.apply(&["managed"]).success();
    let recorded = fixture.recorded();
    fs::remove_file(fixture.path("state/dotfiles/home.json")).unwrap();
    fixture.apply(&[]).success();
    assert!(fixture.path("home/managed").exists());
    for invalid in [
        "{".to_owned(),
        serde_json::to_string(&serde_json::json!({"version": 2, "home": fixture.path("home"), "copies": recorded["copies"]})).unwrap(),
        serde_json::to_string(&serde_json::json!({"version": 1, "home": fixture.path("home"), "copies": {"../unmanaged": "0".repeat(64)}})).unwrap(),
    ] {
        fixture.write("state/dotfiles/home.json", &invalid);
        fixture.apply(&[]).failure();
        assert_eq!(fs::read_to_string(fixture.path("home/managed")).unwrap(), "new");
        assert_eq!(fs::read_to_string(fixture.path("home/unmanaged")).unwrap(), "keep");
        assert_eq!(fs::read_to_string(fixture.path("state/dotfiles/home.json")).unwrap(), invalid);
    }
}

#[test]
fn partial_failure_records_completed_resources_and_reapply_converges() {
    let fixture = Fixture::new();
    fixture.write("source/home/first", "first");
    fixture.write("source/home/second/resource", "second");
    fs::create_dir(fixture.path("home/second")).unwrap();
    fs::set_permissions(
        fixture.path("home/second"),
        fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    let result = fixture.apply(&["first", "second/resource"]);
    fs::set_permissions(
        fixture.path("home/second"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    result.failure();
    let copies = fixture.recorded()["copies"].as_object().unwrap().clone();
    assert!(copies.contains_key("first"));
    assert!(!copies.contains_key("second/resource"));
    assert_eq!(
        fs::read_to_string(fixture.path("home/first")).unwrap(),
        "first"
    );
    assert!(!fixture.path("home/second/resource").exists());

    fixture.apply(&["first", "second/resource"]).success();
    assert_eq!(fixture.recorded()["copies"].as_object().unwrap().len(), 2);
    assert_eq!(
        fs::read_to_string(fixture.path("home/second/resource")).unwrap(),
        "second"
    );
    fixture.apply(&[]).success();
    assert!(!fixture.path("home/first").exists());
    assert!(!fixture.path("home/second/resource").exists());
}
