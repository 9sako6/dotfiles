use assert_cmd::cargo::cargo_bin;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, text).unwrap();
}
fn executable(path: &Path, text: &str) {
    write(path, text);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn public_skill_removal_keeps_source_and_generated_output_on_compile_failure_then_retries() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path();
    write(&path.join("flake.nix"), "{}");
    write(&path.join("dotfiles.toml"), "");
    let home = path.join("home");
    write(&home.join("AGENTS.md"), "source instructions\n");
    write(
        &home.join(".codex/AGENTS.md"),
        "old generated instructions\n",
    );
    write(&home.join(".apm/skills/local/SKILL.md"), "local skill\n");
    write(
        &home.join("apm.yml"),
        "dependencies:\n  apm:\n    - owner/remote\n    - ./.apm/skills/local\n",
    );
    write(&home.join("apm.lock.yaml"), "generated_at: old\nvalue: 1\n");
    executable(
        &path.join("bin/mise"),
        "#!/bin/sh\nprintf '%s\n' \"$FIXTURE/apm\"\n",
    );
    executable(
        &path.join("apm"),
        r#"#!/bin/sh
set -eu
case "$1" in
  install)
    if [ "${2:-}" != --frozen ]; then
      printf 'dependencies:\n  apm:\n    - owner/remote\n    - ./.apm/skills/local\n' > apm.yml
    fi
    ;;
  uninstall)
    printf 'dependencies:\n  apm:\n    - owner/remote\n' > apm.yml
    printf 'generated_at: new\nvalue: 1\n' > apm.lock.yaml
    ;;
  compile)
    [ ! -e "$FIXTURE/fail-compile" ] || exit 42
    printf 'new generated instructions\n' > .codex/AGENTS.md
    ;;
  *) exit 99 ;;
esac
"#,
    );
    let run = || {
        Command::new(cargo_bin!("dotfiles"))
            .args(["agents", "remove-local", "local"])
            .env("DOTFILES_DIR", path)
            .env("HOME", &home)
            .env("FIXTURE", path)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", path.join("bin").display()),
            )
            .output()
            .unwrap()
    };
    write(&path.join("fail-compile"), "");
    let failed = run();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("compile"));
    assert_eq!(
        fs::read_to_string(home.join(".apm/skills/local/SKILL.md")).unwrap(),
        "local skill\n"
    );
    assert_eq!(
        fs::read_to_string(home.join(".codex/AGENTS.md")).unwrap(),
        "old generated instructions\n"
    );
    assert_eq!(
        fs::read_to_string(home.join("apm.lock.yaml")).unwrap(),
        "generated_at: old\nvalue: 1\n"
    );
    assert!(fs::read_to_string(home.join("apm.yml"))
        .unwrap()
        .contains("owner/remote"));
    fs::remove_file(path.join("fail-compile")).unwrap();
    let retried = run();
    assert!(
        retried.status.success(),
        "{}",
        String::from_utf8_lossy(&retried.stderr)
    );
    assert!(!home.join(".apm/skills/local").exists());
    assert_eq!(
        fs::read_to_string(home.join(".codex/AGENTS.md")).unwrap(),
        "new generated instructions\n"
    );
    let dependencies = fs::read_to_string(home.join("apm.yml")).unwrap();
    assert!(dependencies.contains("owner/remote"));
    assert!(!dependencies.contains("./.apm/skills/local"));
}
