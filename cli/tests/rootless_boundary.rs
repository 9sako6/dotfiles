//! End-to-end ownership-boundary coverage through the actual public binary.
//!
//! Only platform/Nix executables are fixtures. Home deployment, confirmation,
//! planning, cache validation and ownership journals use production code. The
//! fake system activation can update only an isolated /etc and /run; /nix/store
//! and /bin/launchctl are likewise private mounts. No host activation is run.
#![cfg(target_os = "linux")]

use assert_cmd::cargo::cargo_bin;
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;
use std::time::Instant;

const CLI_FILES: &[(&str, &str)] = &[
    ("cli/Cargo.lock", "fixture lock\n"),
    ("cli/Cargo.toml", "fixture manifest\n"),
    ("cli/build.rs", "fixture build inputs\n"),
];
const ANKI: &str = "Library/Application Support/Anki2/addons21/anki-connect";
const STORE: &str = "/nix/store";
const ROOT: &str = "22222222222222222222222222222222-artifacts";
const ANKI_PACKAGE: &str = "33333333333333333333333333333333-anki-connect";
const NIGHTLIGHT_PACKAGE: &str = "44444444444444444444444444444444-nightlight";

fn write(path: &Path, contents: impl AsRef<[u8]>) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o644)).unwrap();
}

fn executable(path: &Path, contents: &str) {
    write(path, contents);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn public_binary() -> &'static Path {
    static BINARY: OnceLock<PathBuf> = OnceLock::new();
    BINARY
        .get_or_init(|| {
            assert_ne!(unsafe { libc::geteuid() }, 0, "requires a non-root user");
            assert_success(
                &Command::new("bwrap")
                    .args(["--unshare-user", "--ro-bind", "/", "/", "/bin/true"])
                    .output()
                    .expect("unprivileged bubblewrap is required"),
            );
            let mut digest = Sha256::new();
            digest.update(b"dotfiles-rootless-cli-v1\0");
            digest.update(b"1.2.3");
            for (name, contents) in CLI_FILES {
                digest.update((name.len() as u64).to_be_bytes());
                digest.update(name.as_bytes());
                digest.update(0o644_u32.to_be_bytes());
                digest.update(Sha256::digest(contents.as_bytes()));
            }
            let target = cargo_bin!("dotfiles")
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("rootless-boundary-cli");
            assert_success(
                &Command::new(env!("CARGO"))
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
                    .env(
                        "DOTFILES_BUILD_REVISION",
                        format!("source-{:x}", digest.finalize()),
                    )
                    .output()
                    .unwrap(),
            );
            target.join("debug/dotfiles")
        })
        .as_path()
}

struct Fixture {
    root: tempfile::TempDir,
    binary: &'static Path,
}

impl Fixture {
    fn new() -> Self {
        let f = Self {
            root: tempfile::tempdir().unwrap(),
            binary: public_binary(),
        };
        for path in [
            "checkout",
            "home",
            "tmp",
            "bin",
            "nix/store",
            "run",
            "generation",
            "etc/nix-darwin",
        ] {
            fs::create_dir_all(f.path(path)).unwrap();
        }
        for path in ["group", "nsswitch.conf", "passwd"] {
            fs::copy(Path::new("/etc").join(path), f.path(&format!("etc/{path}"))).unwrap();
        }
        for (name, content) in CLI_FILES {
            write(&f.path(&format!("checkout/{name}")), content);
        }
        for name in [
            "flake.lock",
            "flake.nix",
            "nix/artifacts.nix",
            "nix/host-flake.nix",
            "nix/host-input.nix",
            "nix/system.nix",
        ] {
            write(
                &f.path(&format!("checkout/{name}")),
                "fixture declaration\n",
            );
        }
        write(&f.path("checkout/nix/localllm/catalog.json"), "{}");
        write(&f.path("checkout/dotfiles.toml"), "copy = ['managed']\n");
        write(&f.path("checkout/home/managed"), "original copy\n");
        write(&f.path("checkout/home/.zshrc"), "original live\n");
        write(
            &f.path("checkout/home/apm.yml"),
            "dependencies:\n  apm: []\n",
        );
        write(
            &f.path("checkout/home/.config/mise/config.toml"),
            "[tools]\nrust = '1.2.3'\n",
        );
        write(
            &f.path("checkout/home/.config/mise/mise.lock"),
            "fixture lock\n",
        );
        write(&f.path("checkout/user-services.toml"), "agents = []\n");
        write(
            &f.path("checkout/user-settings.toml"),
            "# no settings initially\n",
        );
        write(
            &f.path("nightlight-state.json"),
            r#"{"schedule":"off","temperature":50}"#,
        );
        executable(&f.path("bin/platform-fixture"), PLATFORM_FIXTURE);
        for name in [
            "launchctl",
            "mise",
            "nix",
            "sudo",
            "darwin-rebuild",
            "nix-env",
        ] {
            symlink(
                f.path("bin/platform-fixture"),
                f.path(&format!("bin/{name}")),
            )
            .unwrap();
        }
        executable(
            &f.path("checkout/bin/system-backend.sh"),
            "#!/bin/sh\nexec \"$FIXTURE/bin/platform-fixture\" backend \"$@\"\n",
        );
        let manifest = serde_json::json!({
            "schemaVersion": 1,
            "artifacts": [
                {"id": "anki-connect", "kind": "anki-addon", "storePath": format!("{STORE}/{ANKI_PACKAGE}"), "relativePath": "share/anki/addons/anki-connect", "homeTarget": ANKI},
                {"id": "nightlight", "kind": "executable", "storePath": format!("{STORE}/{NIGHTLIGHT_PACKAGE}"), "relativePath": "bin/nightlight", "homeTarget": ".local/bin/nightlight"}
            ]
        });
        write(
            &f.path(&format!("nix/store/{ROOT}/manifest.json")),
            manifest.to_string(),
        );
        write(
            &f.path(&format!(
                "nix/store/{ANKI_PACKAGE}/share/anki/addons/anki-connect/__init__.py"
            )),
            "# tiny addon fixture\n",
        );
        executable(
            &f.path(&format!("nix/store/{NIGHTLIGHT_PACKAGE}/bin/nightlight")),
            "#!/bin/sh\nexec \"$FIXTURE/bin/platform-fixture\" nightlight \"$@\"\n",
        );
        fs::create_dir_all(f.path(&format!("nix/store/{ROOT}/artifacts"))).unwrap();
        for (id, package) in [
            ("anki-connect", ANKI_PACKAGE),
            ("nightlight", NIGHTLIGHT_PACKAGE),
        ] {
            symlink(
                format!("{STORE}/{package}"),
                f.path(&format!("nix/store/{ROOT}/artifacts/{id}")),
            )
            .unwrap();
        }
        write(
            &f.path("artifacts.json"),
            serde_json::json!({
                "manifestData": manifest,
                "manifest": format!("{STORE}/11111111111111111111111111111111-manifest.drv"),
                "output": format!("{STORE}/{ROOT}"),
                "root": format!("{STORE}/00000000000000000000000000000000-artifacts.drv")
            })
            .to_string(),
        );
        write(
            &f.path("generation/dotfiles-inventory.json"),
            f.inventory().to_string(),
        );
        symlink(
            f.path("checkout/flake.nix"),
            f.path("etc/nix-darwin/flake.nix"),
        )
        .unwrap();
        symlink(f.path("generation"), f.path("run/current-system")).unwrap();
        f.git(&["init", "-q"]);
        f.git(&["add", "."]);
        f.git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture",
        ]);
        f
    }

    fn path(&self, path: &str) -> PathBuf {
        self.root.path().join(path)
    }

    fn inventory(&self) -> serde_json::Value {
        serde_json::json!({"schemaVersion": 2, "source": self.path("checkout"), "packages": [], "system": [], "services": [], "tools": [], "homeManagerTargets": [], "timeZone": "UTC", "localllm": {"enabled": false, "default_model": null}})
    }

    fn git(&self, args: &[&str]) {
        assert_success(
            &Command::new("git")
                .args(args)
                .current_dir(self.path("checkout"))
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .output()
                .unwrap(),
        );
    }

    fn command(&self, action: &str) -> Command {
        let mut command = Command::new("bwrap");
        command
            .args(["--die-with-parent", "--unshare-user", "--uid"])
            .arg(unsafe { libc::getuid() }.to_string())
            .arg("--gid")
            .arg(unsafe { libc::getgid() }.to_string());
        // Start with an empty namespace root so machines without /nix and
        // launchctl do not need host-side mount targets to be created.
        for path in [
            "/usr",
            "/lib",
            "/lib64",
            "/workspace",
            "/tmp",
            "/home",
            "/proc",
        ] {
            if Path::new(path).exists() {
                command.args(["--ro-bind", path, path]);
            }
        }
        command
            .args([
                "--dir",
                "/bin",
                "--symlink",
                "/usr/bin/sh",
                "/bin/sh",
                "--dev",
                "/dev",
                "--bind",
            ])
            .arg(self.root.path())
            .arg(self.root.path());
        for (source, target) in [("etc", "/etc"), ("run", "/run"), ("nix", "/nix")] {
            command.arg("--bind").arg(self.path(source)).arg(target);
        }
        command
            .arg("--ro-bind")
            .arg(self.path("bin/platform-fixture"))
            .arg("/bin/launchctl")
            .arg("--chdir")
            .arg(self.path("home"))
            .arg("--")
            .arg(self.binary)
            .arg(action)
            .env("DOTFILES_DIR", self.path("checkout"))
            .env("FIXTURE", self.root.path())
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
        child.wait_with_output().unwrap()
    }

    fn commands(&self) -> String {
        fs::read_to_string(self.path("commands")).unwrap_or_default()
    }

    fn clear_commands(&self) {
        write(&self.path("commands"), "");
    }

    fn seed(&self) {
        assert_success(&self.run("apply", "yes\n"));
        let commands = self.commands();
        assert!(commands.contains("nix "), "{commands}");
        assert!(commands.contains("backend activate"), "{commands}");
        assert!(self
            .path("home/.local/state/dotfiles/artifact-evaluation.json")
            .is_file());
        assert!(self
            .path("home/.local/state/dotfiles/artifacts.json")
            .is_file());
        assert!(self.path(&format!("home/{ANKI}")).is_symlink());
        assert!(self.path("home/.zshrc").is_symlink());
        assert_eq!(
            fs::read(self.path("home/managed")).unwrap(),
            b"original copy\n"
        );
        self.clear_commands();
    }

    fn rootless(&self) {
        write(&self.path("forbid-nix"), "");
        self.clear_commands();
    }

    fn assert_rootless(&self) {
        let commands = self.commands();
        for prefix in ["nix ", "backend ", "sudo ", "darwin-rebuild ", "nix-env "] {
            assert!(
                !commands.lines().any(|line| line.starts_with(prefix)),
                "forbidden command: {commands}"
            );
        }
        assert!(!self.path("etc/nix-darwin/flake.nix.apply.lock").exists());
    }
}

const PLATFORM_FIXTURE: &str = r##"#!/usr/bin/python3
import hashlib, json, os, pathlib, shutil, sys
f = pathlib.Path(os.environ['FIXTURE'])
name = pathlib.Path(sys.argv[0]).name
args = sys.argv[1:]
if name == 'platform-fixture':
    name, args = args[0], args[1:]
with (f / 'commands').open('a') as log:
    log.write(name + ' ' + ' '.join(args) + '\n')
def emit(value): print(json.dumps(value))
def replace_link(target, path):
    path = pathlib.Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.unlink(missing_ok=True)
    path.symlink_to(target)
def inventory():
    result = json.loads((f / 'generation/dotfiles-inventory.json').read_text())
    result['source'] = str(f / 'checkout')
    if (f / 'host-inputs.json').exists():
        identity = json.loads((f / 'host-inputs.json').read_text())['systemInputs']
        result['packages'] = [{'name': 'fixture-system', 'manager': 'nix', 'declared': identity}]
    return result
def generation():
    inputs = json.loads((f / 'host-inputs.json').read_text())
    path = f / ('generation-' + inputs['systemInputs'][:16])
    path.mkdir(exist_ok=True)
    (path / 'dotfiles-system-inputs').write_text(inputs['systemInputs'])
    (path / 'dotfiles-inventory.json').write_text(json.dumps(inventory()))
    return path
if name == 'backend':
    if (f / 'forbid-nix').exists(): sys.exit(97)
    if args[0] == 'require-nix': print(f / 'bin/nix')
    elif args[0] == 'activate':
        assert args[-1] == '--system-only', 'system backend must not re-plan home deployment'
        if (f / 'expect-rootless-first').exists():
            assert (f / 'home/managed').read_text() == 'mixed change\n'
            assert (f / 'tools-installed').exists()
            assert (f / 'service-loaded').exists()
        if (f / 'fail-activation').exists(): sys.exit(42)
        replace_link(str(pathlib.Path(args[3]).resolve()), f / 'run/current-system')
        replace_link(args[5], f / 'etc/nix-darwin/flake.nix')
    else: raise RuntimeError('unexpected backend operation')
elif name == 'nix':
    if (f / 'forbid-nix').exists(): sys.exit(97)
    args = args[2:]
    if args[:2] == ['store', 'add-path']:
        source = pathlib.Path(args[-1])
        destination = pathlib.Path('/nix/store') / (hashlib.sha256(str(source).encode()).hexdigest()[:32] + '-source')
        shutil.copytree(source, destination, symlinks=True, dirs_exist_ok=True)
        if (source / 'inputs.json').exists(): shutil.copyfile(source / 'inputs.json', f / 'host-inputs.json')
        print(destination)
    elif args[:2] == ['hash', 'path']: print('sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=')
    elif args[:2] == ['eval', '--impure']:
        assert os.environ['DOTFILES_INPUT_OPERATION'] == 'artifacts'
        emit(json.loads((f / 'artifacts.json').read_text()))
    elif args[:2] == ['eval', '--raw']: emit(inventory())
    elif args[:2] == ['build', '--dry-run']:
        emit([{'drvPath': str(f / 'system.drv'), 'outputs': {'out': str(generation())}}, {'drvPath': str(f / 'brewfile.drv'), 'outputs': {'out': str(f / 'brewfile')}}])
    elif args[0] == 'build' and '--out-link' in args:
        link = args[args.index('--out-link') + 1]
        if '--json' in args:
            artifact = json.loads((f / 'artifacts.json').read_text())
            replace_link(artifact['output'], link)
            emit([{'drvPath': artifact['root'], 'outputs': {'out': artifact['output']}}])
        elif args[-1].endswith('system.drv^*'): replace_link(str(generation()), link)
        elif args[-1].endswith('brewfile.drv^*'): raise RuntimeError('native fallback must not run')
        else: replace_link(args[-1], link)
    else: raise RuntimeError('unexpected Nix operation: ' + repr(args))
elif name == 'mise':
    if args == ['install', '--locked']:
        if (f / 'fail-tools').exists(): sys.exit(42)
        (f / 'tools-installed').write_text('yes')
    elif args == ['ls', '--json', '--locked']:
        config = pathlib.Path(os.environ['MISE_CONFIG_FILE']).read_text()
        installed = (f / 'tools-installed').exists()
        tools = {'rust': [{'version': '1.2.3', 'requested_version': '1.2.3', 'installed': True, 'source': {'path': os.environ['MISE_CONFIG_FILE']}}]}
        if 'fixture-tool' in config:
            tools['fixture-tool'] = [{'version': '2.0.0', 'requested_version': '2.0.0', 'installed': installed, 'source': {'path': os.environ['MISE_CONFIG_FILE']}}]
        emit(tools)
    else: raise RuntimeError('unexpected mise operation')
elif name == 'launchctl':
    uid = os.geteuid()
    state = f / 'service-loaded'
    if args[0] == 'print':
        if args[1] == 'gui/' + str(uid): print('GUI domain')
        elif state.exists():
            print(args[1] + ' = {\n\tpath = ' + state.read_text() + '\n\ttype = LaunchAgent\n}')
        else:
            print('Could not find service "' + args[1].split('/')[-1] + '" in domain for user gui: ' + str(uid), file=sys.stderr)
            sys.exit(113)
    elif args[0] == 'bootstrap': state.write_text(args[2])
    elif args[0] == 'bootout': state.unlink()
    else: raise RuntimeError('unexpected launchctl operation')
elif name == 'nightlight':
    path = f / 'nightlight-state.json'
    state = json.loads(path.read_text())
    if args == ['help']: print('nightlight v1.0.0')
    elif args == ['schedule']: print(state['schedule'])
    elif args == ['temp']: print(state['temperature'])
    elif args[0] == 'schedule': state['schedule'] = args[1] + ' to ' + args[2]; path.write_text(json.dumps(state))
    elif args[0] == 'temp': state['temperature'] = int(args[1]); path.write_text(json.dumps(state))
    else: raise RuntimeError('unexpected nightlight operation')
else: raise RuntimeError('forbidden platform command: ' + name)
"##;

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn cold_plan_and_cancel_do_not_realize_or_deploy_resources() {
    let f = Fixture::new();
    let generation = fs::read_link(f.path("run/current-system")).unwrap();
    assert_success(&f.run("plan", ""));
    let cancelled = f.run("apply", "no\n");
    assert!(!cancelled.status.success());
    assert!(String::from_utf8_lossy(&cancelled.stderr).contains("system apply cancelled"));
    assert_eq!(fs::read_dir(f.path("home")).unwrap().count(), 0);
    assert_eq!(
        fs::read_link(f.path("run/current-system")).unwrap(),
        generation
    );
    let commands = f.commands();
    assert!(commands.contains("nix "), "{commands}");
    for forbidden in [
        "backend activate",
        "build --json",
        "mise install",
        "launchctl bootstrap",
    ] {
        assert!(!commands.contains(forbidden), "{commands}");
    }
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn settings_is_read_only_and_nix_free_with_cold_and_warm_artifact_cache() {
    let f = Fixture::new();
    write(&f.path("checkout/user-services.toml"), "[[agents]]\nlabel = 'com.example.report'\nargv = ['/usr/bin/true', 'DO_NOT_PRINT_PUBLIC_ARGV']\nstart_interval = 60\n");
    write(
        &f.path("checkout/user-settings.toml"),
        "[night_shift]\nstart = '22:00'\nend = '07:00'\ntemperature = 65\n",
    );
    let mut active = f.inventory();
    active["packages"] =
        serde_json::json!([{"name":"private-tool", "manager":"nix", "declared":"8.7.6"}]);
    active["services"] = serde_json::json!([{"name":"private.daemon", "scope":"system", "config":{"RunAtLoad":true, "ProgramArguments":["DO_NOT_PRINT_PRIVATE_ARGV"], "EnvironmentVariables":{"TOKEN":"DO_NOT_PRINT_PRIVATE_ENV"}}}]);
    write(
        &f.path("generation/dotfiles-inventory.json"),
        active.to_string(),
    );
    f.rootless();
    let cold = f.run("settings", "");
    assert_success(&cold);
    let report = String::from_utf8(cold.stdout).unwrap();
    for expected in [
        "settings",
        "managed",
        "rust",
        "1.2.3",
        "com.example.report",
        "launchd",
        "Night Shift",
        "22:00",
        "07:00",
        "private-tool",
        "8.7.6",
        "private.daemon",
        "Active generation:",
        "last-applied records",
    ] {
        assert!(report.contains(expected), "{report}");
    }
    assert!(!report.contains("DO_NOT_PRINT"), "{report}");
    assert_eq!(fs::read_dir(f.path("home")).unwrap().count(), 0);
    assert!(f.commands().is_empty(), "{}", f.commands());
    fs::remove_file(f.path("forbid-nix")).unwrap();
    f.seed();
    f.rootless();
    let cache = f.path("home/.local/state/dotfiles/artifact-evaluation.json");
    let bytes = fs::read(&cache).unwrap();
    let metadata = fs::metadata(&cache).unwrap();
    assert_success(&f.run("settings", ""));
    assert_eq!(fs::read(&cache).unwrap(), bytes);
    assert_eq!(fs::metadata(&cache).unwrap().ino(), metadata.ino());
    assert_eq!(
        fs::metadata(&cache).unwrap().modified().unwrap(),
        metadata.modified().unwrap()
    );
    assert!(f.commands().is_empty(), "{}", f.commands());
    f.assert_rootless();
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn warm_home_and_tool_changes_use_no_nix_or_system_activation() {
    let f = Fixture::new();
    f.seed();
    let generation = fs::read_link(f.path("run/current-system")).unwrap();
    let source = fs::read_link(f.path("etc/nix-darwin/flake.nix")).unwrap();
    let artifact_ledger = fs::read(f.path("home/.local/state/dotfiles/artifacts.json")).unwrap();
    write(&f.path("checkout/home/managed"), "changed copy\n");
    write(&f.path("checkout/home/.zshrc"), "changed live\n");
    write(
        &f.path("checkout/home/.config/mise/config.toml"),
        "[tools]\nfixture-tool = '2.0.0'\nrust = '1.2.3'\n",
    );
    f.rootless();
    let planned = f.run("plan", "");
    assert_success(&planned);
    assert!(String::from_utf8_lossy(&planned.stdout).contains("managed"));
    assert_eq!(
        fs::read(f.path("home/managed")).unwrap(),
        b"original copy\n"
    );
    assert!(!f.path("tools-installed").exists());
    let cancelled = f.run("apply", "no\n");
    assert!(!cancelled.status.success());
    assert!(!f.path("tools-installed").exists());
    assert_eq!(
        fs::read(f.path("home/managed")).unwrap(),
        b"original copy\n"
    );
    assert_success(&f.run("apply", "yes\n"));
    assert_eq!(fs::read(f.path("home/managed")).unwrap(), b"changed copy\n");
    assert_eq!(
        fs::read_link(f.path("home/.zshrc")).unwrap(),
        f.path("checkout/home/.zshrc")
    );
    assert!(f.commands().contains("mise install --locked"));
    f.assert_rootless();
    f.git(&["add", "home"]);
    f.git(&[
        "-c",
        "user.name=Fixture",
        "-c",
        "user.email=fixture@example.invalid",
        "commit",
        "-qm",
        "public user resources only",
    ]);
    f.clear_commands();
    let converged = f.run("apply", "");
    assert_success(&converged);
    assert!(converged.stdout.is_empty());
    f.assert_rootless();
    assert!(!f.commands().contains("mise install"));
    assert_eq!(
        fs::read_link(f.path("run/current-system")).unwrap(),
        generation
    );
    assert_eq!(
        fs::read_link(f.path("etc/nix-darwin/flake.nix")).unwrap(),
        source
    );
    assert_eq!(
        fs::read(f.path("home/.local/state/dotfiles/artifacts.json")).unwrap(),
        artifact_ledger
    );
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn warm_service_and_setting_changes_use_no_nix_or_sudo() {
    let f = Fixture::new();
    f.seed();
    write(&f.path("checkout/user-services.toml"), "[[agents]]\nlabel = 'com.example.boundary'\nargv = ['/usr/bin/true']\nrun_at_load = true\n");
    write(
        &f.path("checkout/user-settings.toml"),
        "[night_shift]\nend = '07:00'\nstart = '22:00'\ntemperature = 65\n",
    );
    f.rootless();
    assert_success(&f.run("plan", ""));
    assert!(!f.path("service-loaded").exists());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(
            &fs::read(f.path("nightlight-state.json")).unwrap()
        )
        .unwrap()["temperature"],
        50
    );
    assert_success(&f.run("apply", "yes\n"));
    assert!(f
        .path("home/Library/LaunchAgents/com.example.boundary.plist")
        .is_file());
    assert!(f.path("service-loaded").is_file());
    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("nightlight-state.json")).unwrap()).unwrap();
    assert_eq!(settings["temperature"], 65);
    assert_eq!(settings["schedule"], "22:00 to 07:00");
    f.assert_rootless();
    f.clear_commands();
    assert_success(&f.run("apply", ""));
    f.assert_rootless();
    let commands = f.commands();
    assert!(!commands.contains("launchctl bootstrap"), "{commands}");
    assert!(!commands.contains("nightlight temp 65"), "{commands}");
    assert!(
        !commands.contains("nightlight schedule 22:00"),
        "{commands}"
    );
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn missing_artifact_cache_uses_nix_but_does_not_reactivate_system() {
    let f = Fixture::new();
    f.seed();
    fs::remove_file(f.path("home/.local/state/dotfiles/artifact-evaluation.json")).unwrap();
    write(
        &f.path("checkout/home/managed"),
        "repair cache while applying home\n",
    );
    assert_success(&f.run("apply", "yes\n"));
    let commands = f.commands();
    assert!(commands.contains("nix "), "{commands}");
    assert!(!commands.contains("backend activate"), "{commands}");
    assert!(f
        .path("home/.local/state/dotfiles/artifact-evaluation.json")
        .is_file());
    f.rootless();
    assert_success(&f.run("apply", ""));
    f.assert_rootless();
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn warm_artifact_copy_handoffs_reuse_the_root_without_nix() {
    let f = Fixture::new();
    f.seed();
    let addon = f.path(&format!("home/{ANKI}"));
    let source = f.path(&format!("checkout/home/{ANKI}/__init__.py"));
    write(&source, "# explicitly copied addon\n");
    write(
        &f.path("checkout/dotfiles.toml"),
        format!("copy = ['{ANKI}', 'managed']\n"),
    );
    f.git(&["add", "home", "dotfiles.toml"]);
    f.rootless();
    assert_success(&f.run("apply", "yes\n"));
    assert!(addon.is_dir());
    assert!(!addon.is_symlink());
    assert_eq!(
        fs::read(addon.join("__init__.py")).unwrap(),
        b"# explicitly copied addon\n"
    );
    f.assert_rootless();
    fs::remove_file(source).unwrap();
    write(&f.path("checkout/dotfiles.toml"), "copy = ['managed']\n");
    f.clear_commands();
    assert_success(&f.run("apply", "yes\n"));
    assert_eq!(
        fs::read_link(addon).unwrap(),
        Path::new(STORE)
            .join(ANKI_PACKAGE)
            .join("share/anki/addons/anki-connect")
    );
    f.assert_rootless();
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn artifact_initially_owned_by_copy_is_restored_from_warm_full_candidate_root() {
    let f = Fixture::new();
    let source = f.path(&format!("checkout/home/{ANKI}/__init__.py"));
    write(&source, "# original copied addon\n");
    write(
        &f.path("checkout/dotfiles.toml"),
        format!("copy = ['{ANKI}', 'managed']\n"),
    );
    f.git(&["add", "home", "dotfiles.toml"]);
    assert_success(&f.run("apply", "yes\n"));
    let addon = f.path(&format!("home/{ANKI}"));
    assert!(addon.is_dir());
    assert!(!addon.is_symlink());
    assert!(f
        .path("home/.local/state/dotfiles/artifact-evaluation.json")
        .exists());
    fs::remove_file(source).unwrap();
    write(&f.path("checkout/dotfiles.toml"), "copy = ['managed']\n");
    f.rootless();
    assert_success(&f.run("apply", "yes\n"));
    assert_eq!(
        fs::read_link(addon).unwrap(),
        Path::new(STORE)
            .join(ANKI_PACKAGE)
            .join("share/anki/addons/anki-connect")
    );
    f.assert_rootless();
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn all_artifacts_copy_owned_still_establish_a_warm_nix_free_plan() {
    let f = Fixture::new();
    write(
        &f.path(&format!("checkout/home/{ANKI}/__init__.py")),
        "# copied addon\n",
    );
    executable(
        &f.path("checkout/home/.local/bin/nightlight"),
        "#!/bin/sh\nexit 0\n",
    );
    write(
        &f.path("checkout/dotfiles.toml"),
        format!("copy = ['.local/bin/nightlight', '{ANKI}', 'managed']\n"),
    );
    f.git(&["add", "home", "dotfiles.toml"]);
    assert_success(&f.run("apply", "yes\n"));
    f.rootless();
    assert_success(&f.run("plan", ""));
    assert_success(&f.run("apply", ""));
    f.assert_rootless();
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn corrupt_cache_and_missing_gc_root_recover_through_nix_without_activation() {
    for fault in ["root", "json", "empty-manifest"] {
        let f = Fixture::new();
        f.seed();
        if fault == "root" {
            let name = format!("{:x}", Sha256::digest(format!("{STORE}/{ROOT}").as_bytes()));
            fs::remove_file(f.path(&format!("home/.local/state/dotfiles/artifact-roots/{name}")))
                .unwrap();
        } else if fault == "empty-manifest" {
            let path = f.path("home/.local/state/dotfiles/artifact-evaluation.json");
            let mut cache: serde_json::Value =
                serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            cache["evaluation"]["manifestData"]["artifacts"] = serde_json::json!([]);
            write(&path, cache.to_string());
        } else {
            write(
                &f.path("home/.local/state/dotfiles/artifact-evaluation.json"),
                "{ broken JSON",
            );
        }
        write(
            &f.path("checkout/home/managed"),
            "changed during cache recovery\n",
        );
        assert_success(&f.run("apply", "yes\n"));
        let commands = f.commands();
        assert!(commands.contains("nix "), "{commands}");
        assert!(!commands.contains("backend activate"), "{commands}");
        assert_eq!(
            fs::read(f.path("home/managed")).unwrap(),
            b"changed during cache recovery\n"
        );
        f.rootless();
        assert_success(&f.run("apply", ""));
        f.assert_rootless();
    }
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn cold_cache_fallback_refuses_to_overwrite_modified_artifact_target() {
    let f = Fixture::new();
    f.seed();
    fs::remove_file(f.path("home/.local/state/dotfiles/artifact-evaluation.json")).unwrap();
    let target = f.path(&format!("home/{ANKI}"));
    fs::remove_file(&target).unwrap();
    write(&target.join("personal.py"), "# unmanaged addon content\n");
    let ledger = fs::read(f.path("home/.local/state/dotfiles/artifacts.json")).unwrap();
    let output = f.run("apply", "yes\n");
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unmanaged or modified"));
    assert_eq!(
        fs::read(target.join("personal.py")).unwrap(),
        b"# unmanaged addon content\n"
    );
    assert_eq!(
        fs::read(f.path("home/.local/state/dotfiles/artifacts.json")).unwrap(),
        ledger
    );
    let commands = f.commands();
    assert!(commands.contains("nix "), "{commands}");
    assert!(!commands.contains("build --json"), "{commands}");
    assert!(!commands.contains("backend activate"), "{commands}");
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn artifact_only_revision_retains_both_roots_without_system_activation() {
    let f = Fixture::new();
    f.seed();
    let generation = fs::read_link(f.path("run/current-system")).unwrap();
    let mut evaluation: serde_json::Value =
        serde_json::from_slice(&fs::read(f.path("artifacts.json")).unwrap()).unwrap();
    let package = "66666666666666666666666666666666-anki-connect-v2";
    let root = "77777777777777777777777777777777-artifacts-v2";
    evaluation["manifestData"]["artifacts"][0]["storePath"] = format!("{STORE}/{package}").into();
    evaluation["output"] = format!("{STORE}/{root}").into();
    evaluation["root"] = format!("{STORE}/{root}.drv").into();
    write(&f.path("artifacts.json"), evaluation.to_string());
    write(
        &f.path(&format!("nix/store/{root}/manifest.json")),
        evaluation["manifestData"].to_string(),
    );
    write(
        &f.path(&format!(
            "nix/store/{package}/share/anki/addons/anki-connect/__init__.py"
        )),
        "# updated artifact\n",
    );
    fs::create_dir_all(f.path(&format!("nix/store/{root}/artifacts"))).unwrap();
    for (id, package) in [
        ("anki-connect", package),
        ("nightlight", NIGHTLIGHT_PACKAGE),
    ] {
        symlink(
            format!("{STORE}/{package}"),
            f.path(&format!("nix/store/{root}/artifacts/{id}")),
        )
        .unwrap();
    }
    write(
        &f.path("checkout/nix/artifacts.nix"),
        "updated artifact declaration\n",
    );
    assert_success(&f.run("apply", "yes\n"));
    assert_eq!(
        fs::read_link(f.path(&format!("home/{ANKI}"))).unwrap(),
        Path::new(STORE)
            .join(package)
            .join("share/anki/addons/anki-connect")
    );
    let commands = f.commands();
    assert!(commands.contains("build --json"), "{commands}");
    assert!(!commands.contains("backend activate"), "{commands}");
    for output in [ROOT, root] {
        let output = format!("{STORE}/{output}");
        let name = format!("{:x}", Sha256::digest(output.as_bytes()));
        assert_eq!(
            fs::read_link(f.path(&format!("home/.local/state/dotfiles/artifact-roots/{name}")))
                .unwrap(),
            Path::new(&output)
        );
    }
    assert_eq!(
        fs::read_link(f.path("run/current-system")).unwrap(),
        generation
    );
    f.rootless();
    assert_success(&f.run("apply", ""));
    f.assert_rootless();
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn system_input_change_uses_nix_and_requires_confirmation_before_activation() {
    let f = Fixture::new();
    f.seed();
    let generation = fs::read_link(f.path("run/current-system")).unwrap();
    write(
        &f.path("checkout/nix/system.nix"),
        "changed system declaration\n",
    );
    assert_success(&f.run("plan", ""));
    let commands = f.commands();
    assert!(commands.contains("nix "), "{commands}");
    assert!(!commands.contains("backend activate"), "{commands}");
    assert_eq!(
        fs::read_link(f.path("run/current-system")).unwrap(),
        generation
    );
    assert_success(&f.run("apply", "yes\n"));
    assert!(f.commands().contains("backend activate"));
    assert_ne!(
        fs::read_link(f.path("run/current-system")).unwrap(),
        generation
    );
}

#[test]
#[ignore = "requires Linux, non-root unprivileged bubblewrap, and an offline Cargo cache"]
fn mixed_apply_finishes_rootless_work_before_system_and_retries_partial_failures() {
    for failure in ["fail-tools", "fail-activation"] {
        let f = Fixture::new();
        f.seed();
        let generation = fs::read_link(f.path("run/current-system")).unwrap();
        let selection = fs::read_link(f.path("etc/nix-darwin/flake.nix")).unwrap();
        write(&f.path("checkout/nix/system.nix"), "changed system\n");
        write(&f.path("checkout/home/managed"), "mixed change\n");
        write(
            &f.path("checkout/home/.config/mise/config.toml"),
            "[tools]\nrust = '1.2.3'\nfixture-tool = '2.0.0'\n",
        );
        write(&f.path("checkout/user-services.toml"), "[[agents]]\nlabel = 'com.example.mixed'\nargv = ['~/.local/bin/nightlight', 'help']\nrun_at_load = true\n");
        write(&f.path("expect-rootless-first"), "");
        write(&f.path(failure), "");
        let failed = f.run("apply", "yes\n");
        assert!(!failed.status.success(), "{failure}");
        assert_eq!(
            fs::read_link(f.path("run/current-system")).unwrap(),
            generation
        );
        assert_eq!(
            fs::read_link(f.path("etc/nix-darwin/flake.nix")).unwrap(),
            selection
        );
        assert_eq!(fs::read(f.path("home/managed")).unwrap(), b"mixed change\n");
        if failure == "fail-tools" {
            assert!(!f.commands().contains("backend activate"));
        }
        fs::remove_file(f.path(failure)).unwrap();
        f.clear_commands();
        assert_success(&f.run("apply", "yes\n"));
        assert!(f.commands().contains("backend activate"));
        assert_ne!(
            fs::read_link(f.path("run/current-system")).unwrap(),
            generation
        );
    }
}

#[test]
#[ignore = "explicit fixture timing benchmark; requires Linux, bubblewrap, and an offline Cargo cache"]
fn benchmark_public_rootless_boundaries() {
    let f = Fixture::new();
    f.seed();
    for case in [
        "cache-miss-no-op",
        "no-op",
        "home",
        "tools",
        "services",
        "system",
    ] {
        let mut elapsed = Vec::new();
        let mut nix_calls = Vec::new();
        let mut activations = Vec::new();
        for sample in 0..5 {
            match case {
                "cache-miss-no-op" => fs::remove_file(f.path("home/.local/state/dotfiles/artifact-evaluation.json")).unwrap(),
                "no-op" => (),
                "home" => write(&f.path("checkout/home/managed"), format!("benchmark copy {sample}\n")),
                "tools" => {
                    write(&f.path("checkout/home/.config/mise/config.toml"), "[tools]\nfixture-tool = '2.0.0'\nrust = '1.2.3'\n");
                    match fs::remove_file(f.path("tools-installed")) {
                        Ok(()) => (),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                        Err(error) => panic!("cannot reset tool fixture: {error}"),
                    }
                }
                "services" => write(&f.path("checkout/user-services.toml"), format!("[[agents]]\nlabel = 'com.example.benchmark'\nargv = ['/usr/bin/true']\nstart_interval = {}\n", 60 + sample)),
                "system" => write(&f.path("checkout/nix/system.nix"), format!("benchmark system {sample}\n")),
                _ => unreachable!(),
            }
            f.clear_commands();
            let started = Instant::now();
            assert_success(&f.run("apply", if case == "no-op" { "" } else { "yes\n" }));
            elapsed.push(started.elapsed().as_secs_f64() * 1000.0);
            let commands = f.commands();
            let nix = commands
                .lines()
                .filter(|line| line.starts_with("nix "))
                .count();
            let activation = commands
                .lines()
                .filter(|line| line.starts_with("backend activate "))
                .count();
            if case == "system" {
                assert!(nix > 0, "{commands}");
                assert_eq!(activation, 1, "{commands}");
            } else if case == "cache-miss-no-op" {
                assert!(nix > 0, "{commands}");
                assert_eq!(activation, 0, "{commands}");
            } else {
                f.assert_rootless();
            }
            nix_calls.push(nix);
            activations.push(activation);
        }
        let mut sorted = elapsed.clone();
        sorted.sort_by(f64::total_cmp);
        println!("rootless-boundary benchmark {case}: median_ms={:.3}; samples_ms={elapsed:?}; nix_calls={nix_calls:?}; system_activations={activations:?}", sorted[sorted.len() / 2]);
    }
}
