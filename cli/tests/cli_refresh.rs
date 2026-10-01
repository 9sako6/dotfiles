use assert_cmd::cargo::cargo_bin;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

struct Fixture {
    _temp: tempfile::TempDir,
    root: PathBuf,
    home: PathBuf,
    bin: PathBuf,
}

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn executable(path: &Path, contents: &str) {
    write(path, contents);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("checkout");
        let home = temp.path().join("home");
        let bin = temp.path().join("bin");
        for path in [
            "flake.nix",
            "cli/Cargo.toml",
            "cli/Cargo.lock",
            "cli/build.rs",
        ] {
            write(
                &root.join(path),
                "new source schema, not understood by old CLI",
            );
        }
        write(
            &root.join("home/.config/mise/config.toml"),
            "[tools]\nrust = '1.2.3'\n",
        );
        write(&home.join("success-record"), "old successful generation");
        write(
            &home.join("inventory.json"),
            "{\"schemaVersion\":2,\"lookup\":{}}\n",
        );
        for args in [vec!["init", "-q"], vec!["add", "."]] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(&root)
                .status()
                .unwrap()
                .success());
        }
        for command in ["nix", "sudo", "darwin-rebuild"] {
            executable(
                &bin.join(command),
                "#!/bin/sh\necho unexpected-system-command >&2\nexit 99\n",
            );
        }
        Self {
            _temp: temp,
            root,
            home,
            bin,
        }
    }

    fn command(&self, action: &str) -> Command {
        let mut command = Command::new(cargo_bin!("dotfiles"));
        command
            .args([action, "--show-trace"])
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env("DOTFILES_DIR", &self.root)
            .env("PRESERVED_VALUE", "spaces and 日本語")
            .env(
                "PATH",
                format!("{}:{}", self.bin.display(), std::env::var("PATH").unwrap()),
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }
}

#[test]
fn plan_and_apply_refresh_before_reading_new_schema_and_preserve_process_contract() {
    for action in ["plan", "apply"] {
        let fixture = Fixture::new();
        executable(
            &fixture.bin.join("mise"),
            r#"#!/bin/sh
set -eu
mkdir -p "$CARGO_TARGET_DIR/release"
printf '#!/bin/sh\nif [ "$1" = --version ]; then echo "dotfiles %s"; exit; fi\n' "$DOTFILES_BUILD_REVISION" > "$CARGO_TARGET_DIR/release/dotfiles"
cat >> "$CARGO_TARGET_DIR/release/dotfiles" <<'BODY'
printf '%s\n' "$@" "$PWD" "$PRESERVED_VALUE" "$$"
exit 23
BODY
chmod +x "$CARGO_TARGET_DIR/release/dotfiles"
"#,
        );
        let child = fixture.command(action).spawn().unwrap();
        let pid = child.id();
        let output = child.wait_with_output().unwrap();
        assert_eq!(
            output.status.code(),
            Some(23),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            format!(
                "{action}\n--show-trace\n{}\nspaces and 日本語\n{pid}\n",
                fixture.home.display()
            )
        );
        assert!(output.stderr.is_empty());
        assert_eq!(
            fs::read_to_string(fixture.home.join("success-record")).unwrap(),
            "old successful generation"
        );
    }
}

#[test]
fn failed_refresh_keeps_old_binary_and_success_record_without_planning() {
    let fixture = Fixture::new();
    let installed = fixture.home.join(".local/bin/dotfiles");
    executable(&installed, "#!/bin/sh\necho 'dotfiles old'\n");
    let old = fs::read(&installed).unwrap();
    executable(&fixture.bin.join("mise"), "#!/bin/sh\nexit 41\n");
    let output = fixture.command("apply").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("CLI build failed; installed binary was retained"));
    assert!(output.stdout.is_empty());
    assert_eq!(fs::read(&installed).unwrap(), old);
    executable(
        &fixture.bin.join("mise"),
        r#"#!/bin/sh
set -eu
mkdir -p "$CARGO_TARGET_DIR/release"
printf '#!/bin/sh\ncase "$0" in */release/dotfiles) echo "dotfiles %s";; *) exit 41;; esac\n' "$DOTFILES_BUILD_REVISION" > "$CARGO_TARGET_DIR/release/dotfiles"
chmod +x "$CARGO_TARGET_DIR/release/dotfiles"
"#,
    );
    let output = fixture.command("apply").output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("replacement CLI cannot run from its destination"));
    assert_eq!(fs::read(&installed).unwrap(), old);

    assert_eq!(
        fs::read_to_string(fixture.home.join("success-record")).unwrap(),
        "old successful generation"
    );
}
