use super::*;
use std::cell::Cell;
use std::os::unix::fs::{symlink, MetadataExt};
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
    let identity = inputs::identity_v2(&source, root, &user(), home, None, &None).unwrap();
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
            "nix/host-flake.nix",
            "cli/src/main.rs",
            "home/resource",
        ] {
            write(&root.join(name), "initial");
        }
        write(&root.join("dotfiles.toml"), "copy = ['resource']\n");
        write(&root.join("nix/localllm/catalog.json"), "{}");
        write(&root.join("home/.config/mise/config.toml"), "[tools]\n");
        write(&root.join("home/.config/mise/mise.lock"), "");
        write(&root.join("home/apm.yml"), "dependencies:\n  apm: []\n");
        let nix = state.join("nix");
        executable(
            &nix,
            &format!(
                r#"#!/bin/sh
set -eu
shift 2
printf '%s\n' "$*" >> '{state}/nix-calls'
case "$1" in
  store)
    [ "$2" = add-path ] || exit 77
    for path do :; done
    printf '%s\n' "$path"
    ;;
  hash)
    [ "$2" = path ] || exit 77
    printf '%s\n' 'sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA='
    ;;
  eval)
    case "${{DOTFILES_INPUT_OPERATION:-}}" in
      artifacts) printf '%s\n' '{{"manifestData":{{"schemaVersion":1,"artifacts":[]}},"manifest":"/nix/store/00000000000000000000000000000000-fixture-manifest.drv","root":"/nix/store/00000000000000000000000000000000-fixture-root.drv","output":"/nix/store/00000000000000000000000000000000-fixture-root"}}' ;;
      *) exit 77 ;;
    esac
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
            launchctl: self.state.join("launchctl-never-called"),
            mise: self.state.join("mise"),
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
            public: snapshot::Snapshot::capture(&self.root, false).unwrap(),
            private: Some(snapshot::Snapshot::capture(&private.root, true).unwrap()),
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
        let confirmed = Rc::new(Cell::new(false));
        let observed = confirmed.clone();
        let runtime = fixture.runtime(move || {
            observed.set(true);
            confirm_apply(&mut answer.as_bytes(), &mut Vec::new())
        });
        assert!(run_with(Mode::Apply, &fixture.root, false, runtime).is_err());
        assert!(
            confirmed.get(),
            "cancellation must reach the reviewed confirmation"
        );
        assert_eq!(fingerprint(&fixture.root).unwrap(), before);
        assert!(!fixture.state.join("activation").exists());
        assert!(!fixture.state.join("home/resource").exists());
        assert!(!fixture
            .state
            .join("home/.local/state/dotfiles/home.json")
            .exists());
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
    assert!(!fixture.state.join("nix-calls").exists());
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
        assert!(!fixture.state.join("nix-calls").exists());
        assert!(!private.state.join("nix-calls").exists());
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
    let error = snapshot::Snapshot::capture(&fixture.root, true)
        .err()
        .unwrap();
    assert!(error
        .to_string()
        .contains("must be committed and unchanged"));
    assert!(!fixture.state.join("nix-calls").exists());
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
        user_settings: None,
        artifacts: artifacts::Plan::empty_for_test(&fixture.state.join("home")),
        user_services: user_services::Plan::default(),
        tools: tools::Plan::capture(
            &inputs.public.source,
            &fixture.state.join("home"),
            &fixture.state.join("mise"),
        )
        .unwrap(),
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
    assert!(plan.system.has_system_changes());
    assert!(!fixture.state.join("home/resource").exists());
}

#[test]
fn resource_apply_and_no_change_reapply_skip_system_evaluation() {
    let fixture = Fixture::new();
    let selection_before = fs::symlink_metadata(fixture.state.join("selection")).unwrap();
    assert!(!fixture.state.join("generation/sw/bin/dotfiles").exists());
    assert!(!fixture.state.join("selection.apply.lock").exists());
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
    let selection_after = fs::symlink_metadata(fixture.state.join("selection")).unwrap();
    assert_eq!(selection_before.ino(), selection_after.ino());
    assert!(!fixture.state.join("selection.apply.lock").exists());
    assert!(!fixture.state.join("activation").exists());
    assert_eq!(
        fs::read_to_string(fixture.state.join("home/resource")).unwrap(),
        "initial"
    );
    let home_state = fixture.state.join("home/.local/state/dotfiles/home.json");
    fs::remove_file(&home_state).unwrap();
    let resource = fixture.state.join("home/resource");
    let before = fs::metadata(&resource).unwrap();
    let runtime = fixture.runtime(|| panic!("unchanged plan must not ask for confirmation"));
    assert_eq!(
        run_with(Mode::Plan, &fixture.root, false, runtime).unwrap(),
        ExitCode::SUCCESS
    );
    assert!(!home_state.exists());
    let runtime = fixture.runtime(|| panic!("unchanged apply must not ask for confirmation"));
    assert_eq!(
        run_with(Mode::Apply, &fixture.root, false, runtime).unwrap(),
        ExitCode::SUCCESS
    );
    assert!(!fixture.state.join("activation").exists());
    let recorded: serde_json::Value =
        serde_json::from_slice(&fs::read(&home_state).unwrap()).unwrap();
    assert!(recorded["copies"].get("resource").is_some());
    let after = fs::metadata(&resource).unwrap();
    assert_eq!(
        (before.ino(), before.mtime(), before.mtime_nsec()),
        (after.ino(), after.mtime(), after.mtime_nsec())
    );
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
fn live_link_changes_use_the_rootless_path_without_system_builds() {
    let fixture = Fixture::new();
    home_copy::plan_live(
        &fixture.root,
        &fixture.root,
        &fixture.state.join("home"),
        &["resource".into()],
    )
    .unwrap()
    .apply()
    .unwrap();
    write(&fixture.root.join("home/.config/new/config"), "live");
    assert!(Command::new("git")
        .args(["add", "home/.config/new/config"])
        .current_dir(&fixture.root)
        .status()
        .unwrap()
        .success());
    let confirmed = Rc::new(Cell::new(0));
    let count = confirmed.clone();
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(move || {
            count.set(count.get() + 1);
            Ok(())
        }),
    )
    .unwrap();
    assert_eq!(confirmed.get(), 1);
    assert!(!fixture.state.join("activation").exists());
    assert!(!fixture.state.join("installs").exists());
    assert_eq!(
        fs::read_link(fixture.state.join("home/.config/new/config")).unwrap(),
        fixture.root.join("home/.config/new/config")
    );
    write(
        &fixture.root.join("home/.config/new/config"),
        "immediate edit",
    );
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(|| panic!("live content needs no deployment")),
    )
    .unwrap();
    assert!(!fixture.state.join("activation").exists());
    assert_eq!(
        fs::read_to_string(fixture.state.join("home/.config/new/config")).unwrap(),
        "immediate edit"
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
fn cli_only_changes_preserve_system_identity_and_skip_host_evaluation() {
    let fixture = Fixture::new();
    write(
        &fixture.root.join("cli/src/main.rs"),
        "new CLI implementation",
    );
    run_with(
        Mode::Plan,
        &fixture.root,
        false,
        fixture.runtime(|| panic!("plan cannot confirm")),
    )
    .unwrap();
    let calls = fs::read_to_string(fixture.state.join("nix-calls")).unwrap();
    assert!(!calls.contains("#inventory"));
    assert!(!calls.contains("--dry-run"));
    assert!(!fixture.state.join("activation").exists());
    assert!(!fixture.state.join("home/resource").exists());
}

#[test]
fn nix_changes_and_legacy_generations_require_system_evaluation() {
    for changed in ["nix/home.nix", "legacy", "selection"] {
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
            error
                .to_string()
                .contains("cannot evaluate managed resources"),
            "{error:#}"
        );
        assert!(!fixture.state.join("activation").exists());
        assert!(!fixture.state.join("home/resource").exists());
    }
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
        .join("nix/system.nix")
        .is_file());
}

fn services_fixture() -> Fixture {
    let fixture = Fixture::new();
    let mut declaration = fs::read_to_string(fixture.root.join("dotfiles.toml")).unwrap();
    declaration.push('\n');
    declaration.push_str(
        "[[services.agents]]\nlabel = 'com.example.fixture'\nargv = ['~/resource']\nrun_at_load = true\n",
    );
    write(&fixture.root.join("dotfiles.toml"), &declaration);
    assert!(Command::new("git")
        .arg("-C")
        .arg(&fixture.root)
        .args(["add", "dotfiles.toml"])
        .status()
        .unwrap()
        .success());
    fs::create_dir(fixture.state.join("loaded")).unwrap();
    executable(
        &fixture.state.join("launchctl"),
        include_str!("../../tests/fixtures/launchctl.sh"),
    );
    write(
        &fixture.state.join("uid"),
        &unsafe { libc::geteuid() }.to_string(),
    );
    write(
        &fixture.state.join("prerequisite"),
        fixture.state.join("home/resource").to_str().unwrap(),
    );
    marker(
        &fixture.root,
        &fixture.state.join("home"),
        &fixture.state.join("generation"),
        &["resource".into()],
    );
    fixture
}

fn services_runtime(fixture: &Fixture, confirm: impl FnOnce() -> Result<()> + 'static) -> Runtime {
    let mut runtime = fixture.runtime(confirm);
    runtime.launchctl = fixture.state.join("launchctl");
    runtime
}

#[test]
fn unified_service_plan_confirm_home_dependency_and_noop() {
    let fixture = services_fixture();
    run_with(
        Mode::Plan,
        &fixture.root,
        false,
        services_runtime(&fixture, || panic!("plan cannot confirm")),
    )
    .unwrap();
    assert!(!fixture.state.join("home/resource").exists());
    assert!(!fixture
        .state
        .join("home/Library/LaunchAgents/com.example.fixture.plist")
        .exists());
    assert!(run_with(
        Mode::Apply,
        &fixture.root,
        false,
        services_runtime(&fixture, || bail!("cancelled"))
    )
    .is_err());
    assert!(!fixture.state.join("home/resource").exists());
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        services_runtime(&fixture, move || {
            observed.set(observed.get() + 1);
            Ok(())
        }),
    )
    .unwrap();
    assert_eq!(count.get(), 1);
    assert!(fixture.state.join("loaded/com.example.fixture").exists());
    assert!(fixture.state.join("home/resource").exists());
    assert!(!fixture.state.join("activation").exists());
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        services_runtime(&fixture, || panic!("converged apply cannot confirm")),
    )
    .unwrap();
    let calls = fs::read_to_string(fixture.state.join("calls")).unwrap();
    assert_eq!(
        calls
            .lines()
            .filter(|line| line.starts_with("bootstrap "))
            .count(),
        1
    );
    assert!(!calls.contains("bootout"));
}

#[test]
fn unified_failed_service_bootstrap_retains_home_then_retries_only_service() {
    let fixture = services_fixture();
    write(&fixture.state.join("fail-bootstrap"), "");
    let result = run_with(
        Mode::Apply,
        &fixture.root,
        false,
        services_runtime(&fixture, || Ok(())),
    );
    assert!(result.is_err());
    let home = fixture.state.join("home/resource");
    let inode = fs::metadata(&home).unwrap().ino();
    assert!(fixture
        .state
        .join("home/.local/state/dotfiles/user-services.json")
        .exists());
    assert!(!fixture.state.join("loaded/com.example.fixture").exists());
    fs::remove_file(fixture.state.join("fail-bootstrap")).unwrap();
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        services_runtime(&fixture, || Ok(())),
    )
    .unwrap();
    assert_eq!(fs::metadata(&home).unwrap().ino(), inode);
    assert!(fixture.state.join("loaded/com.example.fixture").exists());
    assert!(!fixture.state.join("activation").exists());
}

#[test]
fn merged_rootless_declaration_changes_after_review_stop_before_deployment() {
    for local in [false, true] {
        let fixture = services_fixture();
        let declaration = fixture.root.join(if local {
            "dotfiles.local.toml"
        } else {
            "dotfiles.toml"
        });
        let result = run_with(
            Mode::Apply,
            &fixture.root,
            false,
            services_runtime(&fixture, move || {
                write(&declaration, "[services]\nagents = []\n");
                Ok(())
            }),
        );
        assert!(result.unwrap_err().to_string().contains("changed"));
        assert!(!fixture.state.join("home/resource").exists());
        assert!(!fixture.state.join("loaded/com.example.fixture").exists());
    }
}

#[test]
fn mixed_activation_validation_requires_expected_generation_and_source() {
    let fixture = Fixture::new();
    let private = Fixture::new();
    let inputs = fixture.inputs(&private);
    let generation = fixture.state.join("generation").canonicalize().unwrap();
    let record = fixture.root.join("flake.nix");
    inputs.verify_activated(&record, &generation).unwrap();
    assert!(inputs
        .verify_activated(&record, &fixture.state.join("other-generation"))
        .is_err());
    assert!(inputs
        .verify_activated(&fixture.root.join("other-flake.nix"), &generation)
        .is_err());
}

fn tools_fixture() -> Fixture {
    let fixture = Fixture::new();
    write(
        &fixture.root.join("home/.config/mise/config.toml"),
        "[tools]\nalpha = '1.0.0'\nawscli = '1.0.0'\nbeta = '2.0.0'\n[env]\nAGY_CLI_DISABLE_AUTO_UPDATE = 'true'\n[settings]\nauto_install = false\ndisable_tools = ['awscli']\n",
    );
    executable(
        &fixture.state.join("mise"),
        &format!(
            r#"#!/bin/sh
set -eu
[ "$MISE_DISABLE_TOOLS" = awscli ]
[ "$MISE_STATE_DIR" = "$PWD/state" ]
case "$*" in
  'ls --json --locked')
    alpha=false; beta=false
    [ ! -f "$HOME/alpha-installed" ] || alpha=true
    [ ! -f "$HOME/beta-installed" ] || beta=true
    printf '{{"alpha":[{{"version":"1.0.0","requested_version":"1.0.0","installed":%s,"source":{{"path":"%s"}}}}],"beta":[{{"version":"2.0.0","requested_version":"2.0.0","installed":%s,"source":{{"path":"%s"}}}}]}}\n' "$alpha" "$MISE_GLOBAL_CONFIG_FILE" "$beta" "$MISE_GLOBAL_CONFIG_FILE"
    ;;
  'install --locked')
    printf install >> '{state}/installs'
    [ ! -e '{state}/nonconverging' ] || exit 0
    : > "$HOME/alpha-installed"
    if [ -e '{state}/change-input' ]; then
      printf '\n' >> '{root}/home/.config/mise/config.toml'
    fi
    if [ -e '{state}/rewrite-lock' ]; then
      printf '\n' >> "$PWD/mise.lock"
    fi
    [ ! -e '{state}/partial-failure' ] || exit 19
    : > "$HOME/beta-installed"
    ;;
  *) exit 77 ;;
esac
"#,
            state = fixture.state.display(),
            root = fixture.root.display(),
        ),
    );
    marker(
        &fixture.root,
        &fixture.state.join("home"),
        &fixture.state.join("generation"),
        &["resource".into()],
    );
    fixture
}

#[test]
fn tools_plan_and_apply_share_one_review_and_reapply_converges() {
    let fixture = tools_fixture();
    let before = fingerprint(&fixture.root).unwrap();
    run_with(
        Mode::Plan,
        &fixture.root,
        false,
        fixture.runtime(|| panic!("plan cannot confirm")),
    )
    .unwrap();
    assert!(!fixture.state.join("installs").exists());
    let confirmed = Rc::new(Cell::new(0));
    let count = confirmed.clone();
    let state = fixture.state.clone();
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(move || {
            assert!(!state.join("installs").exists());
            count.set(count.get() + 1);
            Ok(())
        }),
    )
    .unwrap();
    assert_eq!(confirmed.get(), 1);
    assert!(fixture.state.join("home/alpha-installed").exists());
    assert!(fixture.state.join("home/beta-installed").exists());
    assert!(!fixture.state.join("activation").exists());
    assert_eq!(
        fs::read_to_string(fixture.state.join("home/resource")).unwrap(),
        "initial"
    );
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(|| panic!("no changes cannot confirm")),
    )
    .unwrap();
    assert_eq!(
        fs::read_to_string(fixture.state.join("installs")).unwrap(),
        "install"
    );
    assert!(!fixture.state.join("activation").exists());
    assert_eq!(fingerprint(&fixture.root).unwrap(), before);
}

#[test]
fn partial_tools_failure_keeps_progress_and_stops_activation_until_retry() {
    let fixture = tools_fixture();
    let before = fingerprint(&fixture.root).unwrap();
    home_copy::plan_live(
        &fixture.root,
        &fixture.root,
        &fixture.state.join("home"),
        &["resource".into()],
    )
    .unwrap()
    .apply()
    .unwrap();
    fs::remove_file(fixture.state.join("home/.local/state/dotfiles/home.json")).unwrap();
    write(&fixture.state.join("partial-failure"), "");
    let error = run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(|| Ok(())),
    )
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("installed in this attempt: alpha; still missing: beta"),
        "{error:#}"
    );
    assert!(!fixture.state.join("activation").exists());
    assert!(fixture
        .state
        .join("home/.local/state/dotfiles/home.json")
        .exists());
    fs::remove_file(fixture.state.join("partial-failure")).unwrap();
    run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(|| Ok(())),
    )
    .unwrap();
    assert!(fixture.state.join("home/beta-installed").exists());
    assert!(!fixture.state.join("activation").exists());
    assert_eq!(fingerprint(&fixture.root).unwrap(), before);
    assert_eq!(
        fs::read_link(fixture.state.join("selection")).unwrap(),
        fixture.root.join("flake.nix")
    );
}

#[test]
fn changed_inputs_changed_lock_and_nonconverging_tools_stop_before_activation() {
    let fixture = tools_fixture();
    let config = fixture.root.join("home/.config/mise/config.toml");
    let error = run_with(
        Mode::Apply,
        &fixture.root,
        false,
        fixture.runtime(move || {
            write(&config, "changed after confirmation");
            Ok(())
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("inputs changed"));
    assert!(!fixture.state.join("installs").exists());
    for (marker, message) in [
        ("change-input", "inputs changed"),
        ("rewrite-lock", "frozen config or lock"),
        ("nonconverging", "still missing: alpha, beta"),
    ] {
        let fixture = tools_fixture();
        write(&fixture.state.join(marker), "");
        let error = run_with(
            Mode::Apply,
            &fixture.root,
            false,
            fixture.runtime(|| Ok(())),
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains(message), "{error:#}");
        assert!(!fixture.state.join("activation").exists());
        assert_eq!(
            fs::read_to_string(fixture.state.join("home/resource")).unwrap(),
            "initial"
        );
        assert!(fixture
            .state
            .join("home/.local/state/dotfiles/home.json")
            .exists());
        assert_eq!(
            fs::read_link(fixture.state.join("selection")).unwrap(),
            fixture.root.join("flake.nix")
        );
        assert_eq!(
            fs::read_link(fixture.state.join("current-system")).unwrap(),
            fixture.state.join("generation")
        );
    }
}
