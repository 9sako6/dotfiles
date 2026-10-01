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
        write(&root.join("home/apm.yml"), "dependencies:\n  apm: []\n");
        let source = state.join("frozen");
        let metadata = serde_json::json!({"path": source, "locked": {"narHash": "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="}});
        let nix = state.join("nix");
        executable(
            &nix,
            &format!(
                r#"#!/bin/sh
set -eu
shift 2
case "$1" in
  flake)
    mkdir -p '{source}'
    git -C '{root}' ls-files | while IFS= read -r name; do
      mkdir -p '{source}'/"$(dirname "$name")"
      cp -P '{root}'/"$name" '{source}'/"$name"
    done
    printf '%s\n' '{metadata}'
    ;;
  eval)
    [ "$DOTFILES_INPUT_OPERATION" = configuration ] || exit 77
    printf '%s\n' '{{"errors":[],"config":{{"copy":["resource"],"private":{{"path":null}}}}}}'
    ;;
  build)
    [ "$2" = --offline ] || exit 77
    if [ -e '{state}/change-during-preview' ]; then
      printf changed > '{root}/home/resource'
    fi
    ;;
  *) exit 77 ;;
esac
"#,
                source = source.display(),
                root = root.display(),
                state = state.display(),
            ),
        );
        executable(
            &root.join("bin/system-backend.sh"),
            &format!(
                r#"#!/bin/sh
case "$1" in
  require-nix) printf '%s\n' '{}' ;;
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

    fn inputs(&self, private: &Fixture) -> PlanInputs {
        let local_path = self.root.join("dotfiles.local.toml");
        let local = read_local(&local_path).unwrap();
        let workspace = tempfile::tempdir().unwrap();
        freeze_local(workspace.path(), &local).unwrap();
        PlanInputs {
            public: Snapshot::capture(&self.state.join("nix"), &self.root, false).unwrap(),
            private: Some(
                Snapshot::capture(&private.state.join("nix"), &private.root, true).unwrap(),
            ),
            local_path,
            local,
            selection: self.state.join("selection"),
            previous: Some(self.root.join("flake.nix")),
            current_generation: self.state.join("current-system"),
            previous_generation: Some(self.state.join("generation")),
            workspace,
        }
    }
}

#[test]
fn cancelled_apply_does_not_install_dependencies_or_change_managed_state() {
    for answer in ["no\n", ""] {
        let fixture = Fixture::new();
        let before = fingerprint(&fixture.root).unwrap();
        let runtime =
            fixture.runtime(move || confirm_apply(&mut answer.as_bytes(), &mut Vec::new()));
        assert!(run_with(Mode::Apply, &fixture.root, false, runtime).is_err());
        assert_eq!(fingerprint(&fixture.root).unwrap(), before);
        assert!(!fixture.state.join("activation").exists());
        assert!(!fixture.state.join("home/resource").exists());
        assert_eq!(
            fs::read_link(fixture.state.join("selection")).unwrap(),
            fixture.root.join("flake.nix")
        );
        assert!(acquire_lock(&fixture.state.join("user-apply.lock")).is_ok());
    }
}

#[test]
fn plan_does_not_take_an_apply_lock_or_modify_managed_state() {
    let fixture = Fixture::new();
    let before = fingerprint(&fixture.root).unwrap();
    let runtime = fixture.runtime(|| panic!("plan must not confirm"));
    run_with(Mode::Plan, &fixture.root, false, runtime).unwrap();
    assert_eq!(fingerprint(&fixture.root).unwrap(), before);
    assert!(!fixture.state.join("user-apply.lock").exists());
    assert!(!fixture.state.join("activation").exists());
    assert!(!fixture.state.join("home/resource").exists());
}

#[test]
fn concurrent_apply_is_rejected_before_snapshotting_or_confirmation() {
    let fixture = Fixture::new();
    let lock = acquire_lock(&fixture.state.join("user-apply.lock")).unwrap();
    let runtime = fixture.runtime(|| panic!("contending apply must not confirm"));
    let error = run_with(Mode::Apply, &fixture.root, false, runtime).unwrap_err();
    assert!(error.to_string().contains("already running"));
    assert!(!fixture.state.join("frozen").exists());
    assert!(!fixture.state.join("activation").exists());
    drop(lock);
}

#[test]
fn shared_snapshot_freezes_and_verifies_public_private_and_local_inputs() {
    for changed in ["public", "private", "local"] {
        let fixture = Fixture::new();
        let private = Fixture::new();
        let local = "[private]\npath = 'fixture'\n";
        write(&fixture.root.join("dotfiles.local.toml"), local);
        let inputs = fixture.inputs(&private);
        inputs.verify().unwrap();
        match changed {
            "public" => write(&fixture.root.join("home/resource"), "new public contents"),
            "private" => write(&private.root.join("home/resource"), "new private contents"),
            "local" => write(&fixture.root.join("dotfiles.local.toml"), ""),
            _ => unreachable!(),
        }
        assert!(inputs.verify().is_err(), "{changed}");
        assert_eq!(
            fs::read_to_string(inputs.public.source.join("home/resource")).unwrap(),
            "initial"
        );
        assert_eq!(
            fs::read_to_string(
                inputs
                    .private
                    .as_ref()
                    .unwrap()
                    .source
                    .join("home/resource")
            )
            .unwrap(),
            "initial"
        );
        assert_eq!(
            fs::read_to_string(inputs.workspace.path().join("dotfiles.local.toml")).unwrap(),
            local
        );
    }
}

#[test]
fn private_snapshot_rejects_an_uncommitted_lock_file() {
    let fixture = Fixture::new();
    write(&fixture.root.join("flake.lock"), "changed lock");
    let error = Snapshot::capture(&fixture.state.join("nix"), &fixture.root, true)
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("must be committed and unchanged"));
    assert!(!fixture.state.join("frozen").exists());
}

#[test]
fn inputs_changed_during_preparation_are_rejected_before_showing_a_plan() {
    for mode in [Mode::Plan, Mode::Apply] {
        let fixture = Fixture::new();
        write(&fixture.state.join("change-during-preview"), "");
        let runtime = fixture.runtime(|| panic!("stale plan must not confirm"));
        let error = run_with(mode, &fixture.root, false, runtime).unwrap_err();
        assert!(error.to_string().contains("inputs changed"));
        assert!(!fixture.state.join("activation").exists());
        assert!(!fixture.state.join("home/resource").exists());
    }
}

#[test]
fn home_and_system_changes_share_one_review() {
    let fixture = Fixture::new();
    let private = Fixture::new();
    let inputs = fixture.inputs(&private);
    let home = home_copy::plan(
        &inputs.public.source,
        &fixture.state.join("home"),
        &["resource".into()],
    )
    .unwrap();
    let inventory = serde_json::from_value(serde_json::json!({
        "source": inputs.public.source,
        "packages": [],
        "system": [{"key": "fixture", "group": "system", "name": "fixture", "value": true}],
        "services": [], "tools": [], "timeZone": "UTC",
        "localllm": {"enabled": false, "default_model": null}
    }))
    .unwrap();
    let system = crate::inventory::Preview::from_inventory(None, inventory).unwrap();
    let mut plan = Plan {
        inputs,
        home,
        system,
    };
    let mut confirmations = 0;
    assert!(matches!(
        plan.review(Mode::Apply, || {
            confirmations += 1;
            Ok(())
        })
        .unwrap(),
        Review::Apply
    ));
    assert_eq!(confirmations, 1);
    assert_eq!(plan.system.copy_changes.len(), 1);
    assert!(!fixture.state.join("home/resource").exists());
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
        let nix = resolve_nix(&root).unwrap();
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
    let nix = resolve_nix(root).unwrap();
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
