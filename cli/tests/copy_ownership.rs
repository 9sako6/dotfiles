use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn linked_or_file_parents_leave_every_entry_and_unowned_file_unchanged() {
    for linked in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let home = temp.path().join("home");
        let outside = temp.path().join("outside");
        write(&source, "home/.agents/skills/new", "new");
        write(&source, "home/.claude/rules/new", "new");
        write(&source, "home/.claude/settings.json", "new");
        write(&source, "home/.claude/skills/new", "new");
        write(&home, ".agents/skills/old", "keep");
        write(&outside, "history", "keep");
        if linked {
            symlink(&outside, home.join(".claude")).unwrap();
        } else {
            fs::write(home.join(".claude"), "keep-parent").unwrap();
        }
        let paths = temp.path().join("paths.json");
        fs::write(
            &paths,
            r#"[".agents/skills", ".claude/rules", ".claude/settings.json", ".claude/skills"]"#,
        )
        .unwrap();
        cargo_bin_cmd!()
            .arg("complete-apply")
            .arg(&source)
            .arg(&paths)
            .arg(&home)
            .assert()
            .failure();
        assert_eq!(
            fs::read_to_string(home.join(".agents/skills/old")).unwrap(),
            "keep"
        );
        assert!(!home.join(".agents/skills/new").exists());
        assert_eq!(fs::read_to_string(outside.join("history")).unwrap(), "keep");
        assert!(!outside.join("rules").exists());
        if linked {
            assert_eq!(fs::read_link(home.join(".claude")).unwrap(), outside);
        } else {
            assert_eq!(
                fs::read_to_string(home.join(".claude")).unwrap(),
                "keep-parent"
            );
        }
    }
}

#[test]
fn all_declared_entries_are_readable_after_repeated_copy() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    let home = temp.path().join("home");
    let files = [
        ".agents/skills/new",
        ".claude/rules/new",
        ".claude/settings.json",
        ".claude/skills/new",
        ".codex/AGENTS.md",
        ".gitconfig",
    ];
    for path in files {
        write(&source, &format!("home/{path}"), path);
    }
    write(&home, ".claude/history", "keep");
    write(&home, ".claude/skills/obsolete", "old");
    let paths = temp.path().join("paths.json");
    fs::write(
        &paths,
        r#"[".agents/skills", ".claude/rules", ".claude/settings.json", ".claude/skills", ".codex/AGENTS.md", ".gitconfig"]"#,
    )
    .unwrap();
    for _ in 0..2 {
        cargo_bin_cmd!()
            .arg("complete-apply")
            .arg(&source)
            .arg(&paths)
            .arg(&home)
            .assert()
            .success();
        for path in files {
            assert_eq!(fs::read_to_string(home.join(path)).unwrap(), path);
            assert!(!home.join(path).is_symlink());
        }
        assert_eq!(
            fs::read_to_string(home.join(".claude/history")).unwrap(),
            "keep"
        );
        assert!(!home.join(".claude/skills/obsolete").exists());
    }
}
