use assert_cmd::cargo::cargo_bin_cmd;
use serde_json::{json, Value};
use std::fs;

#[test]
fn snapshot_normalizes_compiled_skills_without_apm_or_other_commands() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    for name in ["local", "remote"] {
        let skill = source.join("home/.agents/skills").join(name);
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\nname: {name}\ndescription: {name} description\n---\n# Body"),
        )
        .unwrap();
    }
    let input = root.path().join("inventory.json");
    fs::write(
        &input,
        serde_json::to_vec(&json!({"source": source, "schemaVersion": 3, "packages": []}))
            .unwrap(),
    )
    .unwrap();
    for manifest in [
        "dependencies:\n  apm:\n    - owner/repo/skills/remote#fixed\n    - ./.apm/skills/local\n",
        "dependencies: {apm: ['./.apm/skills/local', 'owner/repo/skills/remote#fixed']}\n",
    ] {
        fs::write(source.join("home/apm.yml"), manifest).unwrap();
        let output = cargo_bin_cmd!()
            .arg("complete-inventory")
            .arg(&input)
            .env("PATH", "")
            .env("DOTFILES_DIR", "invalid-relative-path")
            .assert()
            .success()
            .stderr("")
            .get_output()
            .stdout
            .clone();
        let value: Value = serde_json::from_slice(&output).unwrap();
        assert_eq!(value["schemaVersion"], 4);
        assert_eq!(value["packages"], json!([]));
        assert_eq!(value["skills"], json!([
            {"name": "local", "origin": "local", "description": "local description"},
            {"name": "remote", "origin": "owner/repo", "description": "remote description"}
        ]));
    }
}

#[test]
fn incomplete_skill_snapshot_never_emits_partial_inventory() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source");
    fs::create_dir_all(source.join("home")).unwrap();
    let input = root.path().join("inventory.json");
    fs::write(&input, serde_json::to_vec(&json!({"source": source})).unwrap()).unwrap();
    for manifest in ["not: [valid", "dependencies: {apm: ['./.apm/skills/missing']}"] {
        fs::write(source.join("home/apm.yml"), manifest).unwrap();
        cargo_bin_cmd!().arg("complete-inventory").arg(&input)
            .assert().failure().stdout("");
    }
}
