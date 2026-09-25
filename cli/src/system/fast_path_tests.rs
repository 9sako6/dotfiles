use super::*;
use std::cell::Cell;
use std::os::unix::fs::symlink;
use std::rc::Rc;

fn write(path: &Path, contents: &str) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

fn executable(path: &Path, contents: &str) {
    write(path, contents);
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn user() -> String {
    String::from_utf8(
        Command::new("/usr/bin/id")
            .arg("-un")
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .trim()
    .to_owned()
}

fn marker(root: &Path, home: &Path, generation: &Path, copy: &[String]) {
    let source = inputs::SystemSource::inspect(root, copy).unwrap();
    let inputs = Inputs {
        directory: root.into(),
        local_file: None,
        private_flake: None,
        public_flake: "unused".into(),
        public_revision: "unused".into(),
        public_source: root.into(),
        resource_flake: "unused".into(),
        system_inputs: None,
        user: user(),
    };
    let identity = inputs::identity(&source, &inputs, &None, home).unwrap();
    write(&generation.join("dotfiles-system-inputs"), &identity);
}

struct Fixture {
    _temporary: tempfile::TempDir,
    root: PathBuf,
    state: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temporary = tempfile::tempdir().unwrap();
        let base = temporary.path().canonicalize().unwrap();
        let root = base.join("checkout");
        let state = base.join("state");
        fs::create_dir_all(state.join("home")).unwrap();
        for name in [
            "flake.nix",
            "flake.lock",
            "dotfiles.toml",
            "nix/home.nix",
            "cli/src/main.rs",
            "home/resource",
        ] {
            write(&root.join(name), "initial");
        }
        let metadata = serde_json::json!({"path": root, "locked": {"narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}});
        let nix = state.join("nix");
        executable(
            &nix,
            &format!(
                r#"#!/bin/sh
shift 2
case "$1" in
  flake) printf '%s\n' '{metadata}' ;;
  eval)
    [ "$DOTFILES_INPUT_OPERATION" = configuration ] || exit 77
    printf '%s\n' '{{"errors":[],"config":{{"copy":["resource"],"private":{{"path":null}}}}}}'
    ;;
  build) [ "$2" = --offline ] || exit 77 ;;
  *) exit 77 ;;
esac
"#
            ),
        );
        executable(
            &root.join("bin/system-backend.sh"),
            &format!(
                r#"#!/bin/sh
case "$1" in
  ensure-nix|require-nix) printf '%s\n' '{}' ;;
  activate) printf '%s\n' "$@" > '{}' ;;
  *) exit 77 ;;
esac
"#,
                nix.display(),
                state.join("activation").display()
            ),
        );
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
            assert!(Command::new("git")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .arg("-C")
                .arg(&root)
                .args(args)
                .status()
                .unwrap()
                .success());
        }
        marker(
            &root,
            &state.join("home"),
            &state.join("generation"),
            &["resource".into()],
        );
        symlink(state.join("generation"), state.join("current-system")).unwrap();
        symlink(root.join("flake.nix"), state.join("selection")).unwrap();
        Self {
            _temporary: temporary,
            root,
            state,
        }
    }

    fn runtime(&self, confirm: impl FnOnce() -> Result<()> + 'static) -> Runtime {
        Runtime {
            home: self.state.join("home"),
            selection: self.state.join("selection"),
            current_generation: self.state.join("current-system"),
            executable: self.state.join("unused-cli"),
            lock: self.state.join("user-apply.lock"),
            confirm: Box::new(confirm),
        }
    }
}

#[test]
fn resource_apply_and_no_change_reapply_skip_system_evaluation() {
    let fixture = Fixture::new();
    let confirmed = Rc::new(Cell::new(false));
    let observed = confirmed.clone();
    let runtime = fixture.runtime(move || {
        observed.set(true);
        Ok(())
    });
    assert_eq!(
        run_with(Mode::Apply, &fixture.root, false, runtime).unwrap(),
        ExitCode::SUCCESS
    );
    assert!(confirmed.get());
    let activation = fs::read_to_string(fixture.state.join("activation")).unwrap();
    assert!(activation.lines().any(|line| line == "--copy-only"));
    assert!(activation.contains(fixture.state.join("generation").to_str().unwrap()));
    fs::remove_file(fixture.state.join("activation")).unwrap();
    home_copy::plan(
        &fixture.root,
        &fixture.state.join("home"),
        &["resource".into()],
    )
    .unwrap()
    .apply()
    .unwrap();
    let runtime = fixture.runtime(|| panic!("unchanged apply must not ask for confirmation"));
    assert_eq!(
        run_with(Mode::Apply, &fixture.root, false, runtime).unwrap(),
        ExitCode::SUCCESS
    );
    assert!(!fixture.state.join("activation").exists());
    write(&fixture.root.join("home/resource"), "changed resource");
    let runtime = fixture.runtime(|| panic!("plan must not ask for confirmation"));
    assert_eq!(
        run_with(Mode::Plan, &fixture.root, false, runtime).unwrap(),
        ExitCode::SUCCESS
    );
    assert_eq!(
        fs::read_to_string(fixture.state.join("home/resource")).unwrap(),
        "initial"
    );
}

#[test]
fn input_and_generation_changes_after_copy_preview_stop_before_activation() {
    for changed in [
        "home/resource",
        "dotfiles.local.toml",
        "current-system",
        "selection",
    ] {
        let fixture = Fixture::new();
        let root = fixture.root.clone();
        let state = fixture.state.clone();
        let runtime = fixture.runtime(move || {
            match changed {
                "current-system" => {
                    fs::create_dir(state.join("other-generation"))?;
                    fs::remove_file(state.join("current-system"))?;
                    symlink(state.join("other-generation"), state.join("current-system"))?;
                }
                "selection" => {
                    fs::remove_file(state.join("selection"))?;
                    symlink("/other/flake.nix", state.join("selection"))?;
                }
                name => fs::write(root.join(name), "changed after confirmation")?,
            }
            Ok(())
        });
        let error = run_with(Mode::Apply, &fixture.root, false, runtime).unwrap_err();
        assert!(
            error.to_string().contains("changed"),
            "{changed}: {error:#}"
        );
        assert!(!fixture.state.join("activation").exists());
        assert!(!fixture.state.join("home/resource").exists());
    }
}

#[test]
fn cli_or_nix_changes_and_legacy_generations_require_system_evaluation() {
    for changed in ["cli/src/main.rs", "nix/home.nix", "legacy", "selection"] {
        let fixture = Fixture::new();
        if changed == "legacy" {
            fs::remove_file(fixture.state.join("generation/dotfiles-system-inputs")).unwrap();
        } else if changed == "selection" {
            fs::remove_file(fixture.state.join("selection")).unwrap();
        } else {
            write(&fixture.root.join(changed), "changed system input");
        }
        let runtime =
            fixture.runtime(|| panic!("failed system preparation must not reach confirmation"));
        let error = run_with(Mode::Apply, &fixture.root, false, runtime).unwrap_err();
        assert!(
            error.to_string().contains("cannot freeze system inputs"),
            "{error:#}"
        );
        assert!(!fixture.state.join("activation").exists());
        assert!(!fixture.state.join("home/resource").exists());
    }
}

#[test]
#[ignore = "requires an isolated checkout, fixture backend and real Lix"]
fn benchmark_fixture() {
    let root = PathBuf::from(env::var_os("DOTFILES_BENCH_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    let state = PathBuf::from(env::var_os("DOTFILES_BENCH_STATE").unwrap())
        .canonicalize()
        .unwrap();
    let operation = env::var("DOTFILES_BENCH_OPERATION").unwrap();
    if operation == "setup" {
        assert!(
            git(&root, &["remote"]).unwrap().is_empty(),
            "benchmark requires an isolated repository without remotes"
        );
        assert!(!root.join("dotfiles.local.toml").exists());
        assert!(PathBuf::from(env::var_os("DOTFILES_BENCH_NIX").unwrap()).is_file());
        fs::create_dir_all(state.join("home")).unwrap();
        executable(
            &root.join("bin/system-backend.sh"),
            r#"#!/bin/sh
set -eu
case "$1" in
  require-nix|ensure-nix) printf '%s\n' "$DOTFILES_BENCH_NIX" ;;
  activate)
    shift
    case " $* " in *" --copy-only "*) ;; *) exit 99 ;; esac
    state="$(CDPATH= cd -- "$DOTFILES_BENCH_STATE" && pwd -P)"
    [ "$3" = "$state/generation" ] || exit 99
    nix="$1"; user="$2"; system="$3"; expected="$4"; desired="$5"; cli="$6"
    shift 6
    exec "$cli" apply-built "$nix" "$user" "$system" "$state/selection" "$expected" "$desired" "$@"
    ;;
  *) exit 99 ;;
esac
"#,
        );
        let (_, inventory) = load_settings(&root).unwrap();
        let (inventory, source): (serde_json::Value, _) = inventory.load().unwrap();
        write(
            &state.join("generation/dotfiles-inventory.json"),
            &serde_json::to_string(&inventory).unwrap(),
        );
        let nix = resolve_nix(&root, false).unwrap();
        let manifest = state.join("configuration.json");
        write(
            &manifest,
            &serde_json::json!({"publicSource": source, "localFile": null}).to_string(),
        );
        let configuration: Configuration =
            evaluate_configuration(&nix, &source, &manifest, "configuration", true).unwrap();
        home_copy::plan(&source, &state.join("home"), &configuration.copy)
            .unwrap()
            .apply()
            .unwrap();
        let system_source = inputs::SystemSource::inspect(&source, &configuration.copy).unwrap();
        let inputs = Inputs {
            directory: root.clone(),
            local_file: None,
            private_flake: None,
            public_flake: "unused".into(),
            public_revision: "unused".into(),
            public_source: source,
            resource_flake: "unused".into(),
            system_inputs: None,
            user: user(),
        };
        write(
            &state.join("generation/dotfiles-system-inputs"),
            &inputs::identity(&system_source, &inputs, &None, &state.join("home")).unwrap(),
        );
        symlink(state.join("generation"), state.join("current-system")).unwrap();
        symlink(root.join("flake.nix"), state.join("selection")).unwrap();
        write(&state.join("selection.apply.lock"), "");
        return;
    }
    let runtime = Runtime {
        home: state.join("home"),
        selection: state.join("selection"),
        current_generation: state.join("current-system"),
        executable: PathBuf::from(env::var_os("DOTFILES_BENCH_CLI").unwrap()),
        lock: state.join("user-apply.lock"),
        confirm: Box::new(|| Ok(())),
    };
    let mode = if operation == "apply" {
        Mode::Apply
    } else {
        Mode::Plan
    };
    assert_eq!(
        run_with(mode, &root, true, runtime).unwrap(),
        ExitCode::SUCCESS
    );
}

#[test]
#[ignore = "requires the repository checkout and real Lix"]
fn nix_generation_contains_the_inputs_used_by_the_copy_fast_path() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let nix = resolve_nix(root, false).unwrap();
    let expression = r#"
      let
        public = builtins.getFlake ("git+file://" + builtins.getEnv "DOTFILES_TEST_REPOSITORY");
        host = public.lib.mkHost {
          dotfilesDirectory = "/fixture";
          primaryUser = "fixture";
          systemInputs = "fixture-system-inputs";
        };
      in host.pkgs.runCommand "dotfiles-input-record-fixture" {} ''
        mkdir -p "$out"
        ${host.config.system.systemBuilderCommands}
      ''
    "#;
    let result: Vec<BuildResult> = evaluate_json(
        nix_command(&nix)
            .args([
                "build",
                "--no-link",
                "--json",
                "--impure",
                "--no-write-lock-file",
                "--no-update-lock-file",
                "--expr",
                expression,
            ])
            .env("DOTFILES_TEST_REPOSITORY", root),
        "cannot build generation metadata fixture",
        true,
    )
    .unwrap();
    let generation = &result[0].outputs.out;
    assert!(inputs::matches_generation(Some(generation), "fixture-system-inputs").unwrap());
    assert!(!inputs::matches_generation(Some(generation), "different-system-inputs").unwrap());
    assert!(generation.join("dotfiles-system-inputs").is_symlink());
    let inventory: serde_json::Value =
        serde_json::from_slice(&fs::read(generation.join("dotfiles-inventory.json")).unwrap())
            .unwrap();
    assert!(Path::new(inventory["source"].as_str().unwrap())
        .join("home/apm.yml")
        .is_file());
}
