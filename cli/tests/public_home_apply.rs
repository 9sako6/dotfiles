//! Public-command coverage with a fake Nix evaluator, never fake deployment.
//!
//! The production CLI deliberately uses absolute system paths. On Linux,
//! bubblewrap supplies private /etc and /run fixtures while making the host
//! filesystem read-only. Run explicitly with --test public_home_apply -- --ignored.
//! Other platforms retain the portable lifecycle unit tests. No network calls
//! are made; child commands retain the enclosing runtime network restrictions.
#![cfg(target_os = "linux")]

use assert_cmd::cargo::cargo_bin;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::Duration;

const CLI_FILES: &[(&str, &str)] = &[
    ("cli/Cargo.lock", "fixture lock\n"),
    ("cli/Cargo.toml", "fixture manifest\n"),
    ("cli/build.rs", "fixture build inputs\n"),
];
const RUST_VERSION: &str = "1.2.3";
const PROMPT: &[u8] = b"Apply this system plan? Type yes: ";

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
}

fn executable(path: &Path, contents: &str) {
    write(path, contents);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn fixture_identity() -> String {
    let mut digest = Sha256::new();
    digest.update(b"dotfiles-rootless-cli-v1\0");
    digest.update(RUST_VERSION.as_bytes());
    for (name, contents) in CLI_FILES {
        digest.update((name.len() as u64).to_be_bytes());
        digest.update(name.as_bytes());
        digest.update(0o644_u32.to_be_bytes());
        digest.update(Sha256::digest(contents.as_bytes()));
    }
    format!("source-{:x}", digest.finalize())
}

fn public_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            assert_ne!(
                unsafe { libc::geteuid() },
                0,
                "public home tests require a non-root user"
            );
            let available = Command::new("bwrap")
                .args([
                    "--unshare-user",
                    "--ro-bind",
                    "/",
                    "/",
                    "--dev",
                    "/dev",
                    "/bin/true",
                ])
                .output();
            assert!(
                available
                    .as_ref()
                    .is_ok_and(|output| output.status.success()),
                "unprivileged bubblewrap is required: {available:?}"
            );
            // Compile the real entrypoint with a fixture source identity. This
            // avoids triggering CLI self-refresh, without adding a runtime hook
            // or substituting a script for the binary under test. A separate,
            // cached target also avoids the enclosing cargo test's build lock.
            let target = cargo_bin!("dotfiles")
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("public-home-cli");
            let output = Command::new(env!("CARGO"))
                .args([
                    "build",
                    "--offline",
                    "--locked",
                    "--bin",
                    "dotfiles",
                    "--manifest-path",
                ])
                .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml"))
                .arg("--target-dir")
                .arg(&target)
                .env("DOTFILES_BUILD_REVISION", fixture_identity())
                .output()
                .unwrap();
            assert_success(&output);
            target.join("debug/dotfiles")
        })
        .as_path()
}

struct Fixture {
    temp: tempfile::TempDir,
    binary: &'static Path,
    copy: Vec<String>,
}

impl Fixture {
    fn new(copy: &[&str]) -> Self {
        let fixture = Self {
            binary: public_binary(),
            temp: tempfile::tempdir().unwrap(),
            copy: copy.iter().map(|path| (*path).to_owned()).collect(),
        };
        for directory in [
            "checkout",
            "home",
            "tmp",
            "frozen",
            "generation",
            "run",
            "etc/nix-darwin",
        ] {
            fs::create_dir_all(fixture.path(directory)).unwrap();
        }
        for name in ["group", "nsswitch.conf", "passwd"] {
            fs::copy(
                Path::new("/etc").join(name),
                fixture.path(&format!("etc/{name}")),
            )
            .unwrap();
        }
        for (name, contents) in CLI_FILES {
            write(&fixture.path(&format!("checkout/{name}")), contents);
        }
        for name in [
            "flake.lock",
            "flake.nix",
            "nix/host-flake.nix",
            "nix/host-input.nix",
        ] {
            write(
                &fixture.path(&format!("checkout/{name}")),
                "frozen fixture\n",
            );
        }
        write(
            &fixture.path("checkout/home/.config/mise/config.toml"),
            format!("[tools]\nrust = '{RUST_VERSION}'\n"),
        );
        write(
            &fixture.path("checkout/home/.config/mise/mise.lock"),
            "fixture lock\n",
        );
        write(
            &fixture.path("checkout/home/apm.yml"),
            "dependencies:\n  apm: []\n",
        );
        write(
            &fixture.path("checkout/dotfiles.toml"),
            format!("copy = {}\n", serde_json::to_string(&fixture.copy).unwrap()),
        );
        executable(
            &fixture.path("checkout/bin/system-backend.sh"),
            r#"#!/bin/sh
set -eu
printf 'backend %s\n' "$*" >> "$FIXTURE/commands"
[ "$1" = require-nix ] || { echo forbidden-system-activation >&2; exit 99; }
printf '%s/bin/nix\n' "$FIXTURE"
"#,
        );
        executable(
            &fixture.path("bin/mise"),
            r#"#!/bin/sh
set -eu
printf 'mise %s\n' "$*" >> "$FIXTURE/commands"
[ "$*" = 'ls --json --locked' ] || { echo forbidden-tool-install >&2; exit 99; }
printf '{"rust":[{"version":"1.2.3","requested_version":"1.2.3","installed":true,"source":{"path":"%s"}}]}\n' "$MISE_CONFIG_FILE"
"#,
        );
        executable(
            &fixture.path("bin/nix"),
            r#"#!/bin/sh
set -eu
shift 2
printf 'nix %s\n' "$*" >> "$FIXTURE/commands"
case "$1 $2" in
  'flake metadata') cat "$FIXTURE/metadata.json" ;;
  'store add-path') for last do :; done; printf '%s\n' "$last" ;;
  'hash path') printf 'sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\n' ;;
  'build --offline') : ;;
  'build --dry-run') cat "$FIXTURE/builds.json" ;;
  'eval --impure')
    [ "$DOTFILES_INPUT_OPERATION" = configuration ] || exit 98
    cat "$FIXTURE/configuration.json" ;;
  'eval --raw') cat "$FIXTURE/inventory.json" ;;
  *) echo forbidden-nix-build >&2; exit 99 ;;
esac
"#,
        );
        for name in ["darwin-rebuild", "nix-env", "sudo"] {
            executable(
                &fixture.path(&format!("bin/{name}")),
                "#!/bin/sh\nprintf 'forbidden %s\\n' \"$0\" >> \"$FIXTURE/commands\"\nexit 99\n",
            );
        }
        symlink(
            fixture.path("checkout/flake.nix"),
            fixture.path("etc/nix-darwin/flake.nix"),
        )
        .unwrap();
        symlink(
            fixture.path("generation"),
            fixture.path("run/current-system"),
        )
        .unwrap();
        fixture
    }

    fn path(&self, name: &str) -> PathBuf {
        self.temp.path().join(name)
    }

    fn freeze(&self) {
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
            vec![
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        ] {
            assert_success(
                &Command::new("git")
                    .env("GIT_CONFIG_GLOBAL", "/dev/null")
                    .env("GIT_CONFIG_NOSYSTEM", "1")
                    .args(args)
                    .current_dir(self.path("checkout"))
                    .output()
                    .unwrap(),
            );
        }
        let tracked = Command::new("git")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .args(["ls-files", "-z"])
            .current_dir(self.path("checkout"))
            .output()
            .unwrap();
        assert_success(&tracked);
        for name in tracked
            .stdout
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = std::str::from_utf8(name).unwrap();
            let source = self.path("checkout").join(name);
            let frozen = self.path("frozen").join(name);
            fs::create_dir_all(frozen.parent().unwrap()).unwrap();
            let metadata = fs::symlink_metadata(&source).unwrap();
            if metadata.file_type().is_symlink() {
                symlink(fs::read_link(source).unwrap(), frozen).unwrap();
            } else {
                fs::copy(source, &frozen).unwrap();
                fs::set_permissions(frozen, fs::Permissions::from_mode(metadata.mode() & 0o555))
                    .unwrap();
            }
        }
        let inventory = serde_json::json!({
            "schemaVersion": 2, "source": self.path("frozen"),
            "packages": [], "system": [], "services": [], "tools": [],
            "homeManagerTargets": [], "timeZone": "UTC",
            "localllm": {"enabled": false, "default_model": null}
        });
        for name in ["inventory.json", "generation/dotfiles-inventory.json"] {
            write(&self.path(name), inventory.to_string());
        }
        write(&self.path("configuration.json"), serde_json::json!({"errors": [], "config": {"copy": self.copy, "private": {"path": null}}}).to_string());
        write(&self.path("metadata.json"), serde_json::json!({"path": self.path("frozen"), "locked": {"narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}}).to_string());
        write(
            &self.path("builds.json"),
            serde_json::json!([
                {"drvPath": self.path("system.drv"), "outputs": {"out": self.path("generation")}},
                {"drvPath": self.path("brewfile.drv"), "outputs": {"out": self.path("brewfile")}}
            ])
            .to_string(),
        );
    }

    fn command(&self, action: &str) -> Command {
        let mut command = Command::new("bwrap");
        command
            .args(["--die-with-parent", "--unshare-user", "--uid"])
            .arg(unsafe { libc::getuid() }.to_string())
            .arg("--gid")
            .arg(unsafe { libc::getgid() }.to_string())
            .args(["--ro-bind", "/", "/", "--dev", "/dev", "--bind"])
            .arg(self.temp.path())
            .arg(self.temp.path())
            .arg("--ro-bind")
            .arg(self.path("etc"))
            .arg("/etc")
            .arg("--ro-bind")
            .arg(self.path("run"))
            .arg("/run")
            .arg("--ro-bind")
            .arg(self.path("frozen"))
            .arg(self.path("frozen"))
            .arg("--chdir")
            .arg(self.path("home"))
            .arg("--")
            .arg(self.binary)
            .arg(action)
            .env("DOTFILES_DIR", self.path("checkout"))
            .env("FIXTURE", self.temp.path())
            .env("HOME", self.path("home"))
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.path("bin").display()),
            )
            .env("TMPDIR", self.path("tmp"))
            .env_remove("XDG_STATE_HOME")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn run(&self, action: &str, answer: &str) -> Output {
        let mut child = self.command(action).spawn().unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(answer.as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert!(
            self.path("commands").exists(),
            "public CLI did not reach the fixture backend: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        self.assert_no_activation();
        output
    }

    fn assert_no_activation(&self) {
        let commands = fs::read_to_string(self.path("commands")).unwrap();
        assert!(!commands.contains("activate"), "{commands}");
        assert!(!commands.contains("forbidden"), "{commands}");
        assert!(!self.path("generation/sw/bin/dotfiles").exists());
        assert!(!self.path("etc/nix-darwin/flake.nix.apply.lock").exists());
        assert_eq!(
            fs::read_link(self.path("etc/nix-darwin/flake.nix")).unwrap(),
            self.path("checkout/flake.nix")
        );
        assert_eq!(
            fs::read_link(self.path("run/current-system")).unwrap(),
            self.path("generation")
        );
    }

    fn recorded(&self) -> serde_json::Value {
        serde_json::from_slice(
            &fs::read(self.path("home/.local/state/dotfiles/home.json")).unwrap(),
        )
        .unwrap()
    }
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[derive(Debug, PartialEq)]
struct Entry {
    inode: u64,
    mode: u32,
    modified: std::time::SystemTime,
    content: Vec<u8>,
}

fn tree(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(root: &Path, path: &Path, entries: &mut BTreeMap<PathBuf, Entry>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let content = if metadata.file_type().is_symlink() {
            fs::read_link(path)
                .unwrap()
                .as_os_str()
                .as_encoded_bytes()
                .to_vec()
        } else if metadata.is_file() {
            fs::read(path).unwrap()
        } else {
            Vec::new()
        };
        entries.insert(
            path.strip_prefix(root).unwrap().to_owned(),
            Entry {
                inode: metadata.ino(),
                mode: metadata.mode(),
                modified: metadata.modified().unwrap(),
                content,
            },
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), entries);
            }
        }
    }
    let mut entries = BTreeMap::new();
    visit(root, root, &mut entries);
    entries
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn public_plan_and_rejected_apply_leave_home_and_ledger_untouched() {
    let fixture = Fixture::new(&["managed"]);
    write(&fixture.path("checkout/home/managed"), "desired");
    write(&fixture.path("home/managed"), "original");
    write(&fixture.path("home/sibling"), "keep");
    fixture.freeze();
    let before = tree(&fixture.path("home"));
    let source = tree(&fixture.path("frozen"));
    let planned = fixture.run("plan", "");
    assert_success(&planned);
    assert!(String::from_utf8_lossy(&planned.stdout).contains("managed"));
    assert!(!String::from_utf8_lossy(&planned.stdout).contains("Type yes"));
    assert_eq!(tree(&fixture.path("home")), before);
    let cancelled = fixture.run("apply", "no\n");
    assert!(!cancelled.status.success());
    assert!(String::from_utf8_lossy(&cancelled.stderr).contains("system apply cancelled"));
    assert_eq!(tree(&fixture.path("home")), before);
    assert_eq!(tree(&fixture.path("frozen")), source);
    assert!(!fixture
        .path("home/.local/state/dotfiles/home.json")
        .exists());
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn public_apply_atomically_repairs_content_and_mode_preserving_unchanged_files_and_siblings() {
    let fixture = Fixture::new(&[".claude/skills"]);
    for (name, contents) in [
        ("content", "new"),
        ("steady", "unchanged"),
        ("tool", "executable"),
    ] {
        write(
            &fixture.path(&format!("checkout/home/.claude/skills/{name}")),
            contents,
        );
        write(
            &fixture.path(&format!("home/.claude/skills/{name}")),
            if name == "content" { "old" } else { contents },
        );
    }
    fs::set_permissions(
        fixture.path("checkout/home/.claude/skills/tool"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    write(&fixture.path("home/.claude/history"), "unowned sibling");
    fixture.freeze();
    let source = tree(&fixture.path("frozen"));
    let before = tree(&fixture.path("home/.claude"));
    let mut old_content = fs::File::open(fixture.path("home/.claude/skills/content")).unwrap();
    let old_mode_file = fs::File::open(fixture.path("home/.claude/skills/tool")).unwrap();
    assert_success(&fixture.run("apply", "yes\n"));
    let after = tree(&fixture.path("home/.claude"));
    assert_eq!(after[Path::new("skills/content")].content, b"new");
    assert_ne!(
        after[Path::new("skills/content")].inode,
        before[Path::new("skills/content")].inode
    );
    let mut retained = String::new();
    old_content.read_to_string(&mut retained).unwrap();
    assert_eq!(
        retained, "old",
        "open old inode must survive atomic replacement"
    );
    assert_eq!(after[Path::new("skills/tool")].mode & 0o777, 0o755);
    assert_ne!(
        after[Path::new("skills/tool")].inode,
        old_mode_file.metadata().unwrap().ino()
    );
    assert_eq!(
        after[Path::new("skills/steady")],
        before[Path::new("skills/steady")]
    );
    assert_eq!(after[Path::new("history")], before[Path::new("history")]);
    assert!(fixture.recorded()["copies"][".claude/skills"].is_string());
    assert_eq!(tree(&fixture.path("frozen")), source);
    let converged = tree(&fixture.path("home"));
    let unchanged = fixture.run("apply", "");
    assert_success(&unchanged);
    assert!(unchanged.stdout.is_empty());
    assert_eq!(tree(&fixture.path("home")), converged);
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn public_commands_refuse_top_level_and_nested_source_symlinks_before_deployment() {
    for nested in [false, true] {
        let fixture = Fixture::new(&["managed"]);
        write(&fixture.path("outside"), "must remain untouched");
        let link = fixture.path(if nested {
            "checkout/home/managed/link"
        } else {
            "checkout/home/managed"
        });
        fs::create_dir_all(link.parent().unwrap()).unwrap();
        symlink(fixture.path("outside"), link).unwrap();
        write(&fixture.path("home/managed"), "old destination");
        fixture.freeze();
        let before = tree(&fixture.path("home"));
        for action in ["plan", "apply"] {
            let output = fixture.run(action, "yes\n");
            assert!(!output.status.success());
            assert!(String::from_utf8_lossy(&output.stderr).contains("symlink"));
            assert_eq!(tree(&fixture.path("home")), before);
            assert_eq!(
                fs::read_to_string(fixture.path("outside")).unwrap(),
                "must remain untouched"
            );
        }
    }
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn public_apply_checkpoints_partial_copy_failure_and_retry_converges() {
    let fixture = Fixture::new(&["first", "second/resource"]);
    write(&fixture.path("checkout/home/first"), "first");
    write(&fixture.path("checkout/home/second/resource"), "second");
    fs::create_dir(fixture.path("home/second")).unwrap();
    fixture.freeze();
    fs::set_permissions(
        fixture.path("home/second"),
        fs::Permissions::from_mode(0o555),
    )
    .unwrap();
    let failed = fixture.run("apply", "yes\n");
    fs::set_permissions(
        fixture.path("home/second"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("failed to copy second/resource"));
    assert_eq!(
        fs::read_to_string(fixture.path("home/first")).unwrap(),
        "first"
    );
    assert!(!fixture.path("home/second/resource").exists());
    assert!(fixture.recorded()["copies"]["first"].is_string());
    assert!(fixture.recorded()["copies"]
        .get("second/resource")
        .is_none());
    let first = tree(&fixture.path("home/first"));
    assert_success(&fixture.run("apply", "yes\n"));
    assert_eq!(tree(&fixture.path("home/first")), first);
    assert_eq!(
        fs::read_to_string(fixture.path("home/second/resource")).unwrap(),
        "second"
    );
    assert_eq!(fixture.recorded()["copies"].as_object().unwrap().len(), 2);
    let converged = tree(&fixture.path("home"));
    let unchanged = fixture.run("apply", "");
    assert_success(&unchanged);
    assert!(unchanged.stdout.is_empty());
    assert_eq!(tree(&fixture.path("home")), converged);
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn public_apply_revalidates_checkout_after_confirmation_without_changing_frozen_source() {
    let fixture = Fixture::new(&["managed"]);
    write(&fixture.path("checkout/home/managed"), "reviewed");
    write(&fixture.path("home/managed"), "original");
    fixture.freeze();
    let before = tree(&fixture.path("home"));
    let frozen = tree(&fixture.path("frozen"));
    let mut child = fixture.command("apply").spawn().unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let (send, receive) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut byte = [0];
        while stdout.read(&mut byte).unwrap() != 0 {
            bytes.push(byte[0]);
            if bytes.ends_with(PROMPT) {
                send.send(()).unwrap();
            }
        }
        bytes
    });
    if receive.recv_timeout(Duration::from_secs(30)).is_err() {
        child.kill().unwrap();
        let output = child.wait_with_output().unwrap();
        panic!(
            "public apply never requested confirmation: {}\n{}",
            String::from_utf8_lossy(&reader.join().unwrap()),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    write(
        &fixture.path("checkout/home/managed"),
        "changed after preview",
    );
    child.stdin.take().unwrap().write_all(b"yes\n").unwrap();
    let output = child.wait_with_output().unwrap();
    reader.join().unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("inputs changed after preview"));
    assert_eq!(tree(&fixture.path("home")), before);
    assert_eq!(tree(&fixture.path("frozen")), frozen);
    fixture.assert_no_activation();
}
