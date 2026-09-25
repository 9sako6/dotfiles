use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use assert_cmd::cargo::{cargo_bin, cargo_bin_cmd};
use predicates::prelude::*;
use tempfile::TempDir;

const PLUGINS: [&str; 3] = [
    "momo-lab/zsh-abbrev-alias",
    "zsh-users/zsh-syntax-highlighting",
    "zsh-users/zsh-autosuggestions",
];

struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    plugins: PathBuf,
    config: PathBuf,
    revisions: Vec<String>,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().canonicalize().unwrap().join("home with spaces");
        let plugins = home.join(".local/share/zinit/plugins");
        let config = home.join(".config/mise/config.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        let mut fixture = Self {
            _temp: temp,
            home,
            plugins,
            config,
            revisions: Vec::new(),
        };
        for index in 0..PLUGINS.len() {
            let directory = fixture.plugin(index);
            fs::create_dir_all(&directory).unwrap();
            fixture.git(index, &["init", "-q"]);
            fs::write(directory.join("plugin.zsh"), "plugin contents\n").unwrap();
            fixture.git(index, &["add", "."]);
            fixture.git(index, &["commit", "-qm", "fixture"]);
            let revision = fixture.git(index, &["rev-parse", "HEAD"]);
            fixture.revisions.push(revision.trim().to_owned());
        }
        fixture.write_config("inline");
        fixture
    }

    fn plugin(&self, index: usize) -> PathBuf {
        self.plugins.join(PLUGINS[index].replace('/', "---"))
    }

    fn git(&self, index: usize, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(self.plugin(index))
            .args([
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
            ])
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .env("HOME", &self.home)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        String::from_utf8(output.stdout).unwrap()
    }

    fn write_config(&self, format: &str) {
        let mut text = if format == "tables" {
            String::new()
        } else {
            "[bootstrap.repos]\n".to_owned()
        };
        for (index, plugin) in PLUGINS.iter().enumerate() {
            let directory = format!(
                "~/.local/share/zinit/plugins/{}",
                plugin.replace('/', "---")
            );
            let revision = &self.revisions[index];
            text.push_str(&match format {
                "compact" => format!("\"{directory}\"={{ref=\"{revision}\"}}\n"),
                "single" => format!("'{directory}' = {{ ref = '{revision}' }}\n"),
                "tables" => format!("[bootstrap.repos.\"{directory}\"]\nref = \"{revision}\"\n"),
                "inline" => format!("\"{directory}\" = {{ ref = \"{revision}\" }}\n"),
                _ => panic!("unknown format"),
            });
        }
        fs::write(&self.config, text).unwrap();
    }

    fn command(&self) -> assert_cmd::Command {
        let mut command = cargo_bin_cmd!();
        command
            .args(["zinit", "verify", "--plugins-dir"])
            .arg(&self.plugins)
            .current_dir(&self.home)
            .env("HOME", &self.home)
            .env_remove("XDG_CONFIG_HOME")
            .env("DOTFILES_DIR", "invalid-relative-path")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1");
        command
    }

    fn shell(&self) -> Output {
        self.shell_with_cli(cargo_bin!("dotfiles"))
    }

    fn shell_with_cli(&self, cli: &Path) -> Output {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("home/.zshrc");
        fs::copy(source, self.home.join(".zshrc")).unwrap();
        fs::write(self.home.join(".zshenv"), "").unwrap();
        let bin = self.home.join(".local/bin");
        fs::create_dir_all(&bin).unwrap();
        executable(&bin.join("mise"), "#!/bin/sh\n[ \"$1\" = activate ]\n");
        symlink(cli, bin.join("dotfiles")).unwrap();
        let zinit = self.home.join(".local/share/zinit/zinit.git");
        fs::create_dir_all(&zinit).unwrap();
        fs::write(
            zinit.join("zinit.zsh"),
            "typeset -gA ZINIT\nZINIT[PLUGINS_DIR]=\"$HOME/.local/share/zinit/plugins\"\nzinit() {\n  if [[ \"$1\" = light ]]; then\n    print -r -- \"$2\" >> \"$HOME/loaded\"\n  fi\n}\n",
        )
        .unwrap();
        Command::new("zsh")
            .args(["-f", "-i", "-c", "source \"$HOME/.zshrc\""])
            .env("HOME", &self.home)
            .env(
                "PATH",
                format!("{}:{}", bin.display(), std::env::var("PATH").unwrap()),
            )
            .env("DOTFILES_NO_BANNER", "1")
            .env("DOTFILES_DIR", "invalid-relative-path")
            .env("XDG_CONFIG_HOME", "")
            .env_remove("XDG_DATA_HOME")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .current_dir(&self.home)
            .output()
            .unwrap()
    }
}

fn executable(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

fn listed(repositories: &[&str]) -> String {
    format!("{}\n", repositories.join("\n"))
}

#[test]
fn equivalent_toml_forms_preserve_verified_loading_order_without_a_checkout() {
    let fixture = Fixture::new();
    for format in ["inline", "compact", "single", "tables"] {
        fixture.write_config(format);
        fixture
            .command()
            .args(PLUGINS)
            .assert()
            .success()
            .stdout(listed(&PLUGINS))
            .stderr("");
    }
    fixture
        .command()
        .args([PLUGINS[2], PLUGINS[0], PLUGINS[2]])
        .assert()
        .success()
        .stdout(listed(&[PLUGINS[2], PLUGINS[0]]))
        .stderr("");
}

#[test]
fn rejects_unregistered_and_mismatched_plugins_but_keeps_valid_plugins() {
    let mut fixture = Fixture::new();
    fixture.revisions[1] = "0".repeat(40);
    fixture.write_config("inline");
    fixture
        .command()
        .args(PLUGINS)
        .arg("unknown/plugin")
        .assert()
        .failure()
        .stdout(listed(&[PLUGINS[0], PLUGINS[2]]))
        .stderr(
            predicate::str::contains(format!("refusing {}", PLUGINS[1]))
                .and(predicate::str::contains("refusing unknown/plugin")),
        );
}

#[test]
fn rejects_tracked_staged_and_untracked_changes() {
    for change in ["tracked", "staged", "untracked"] {
        let fixture = Fixture::new();
        let path = if change == "untracked" {
            "stray.zwc"
        } else {
            "plugin.zsh"
        };
        fs::write(fixture.plugin(1).join(path), "changed\n").unwrap();
        if change == "staged" {
            fixture.git(1, &["add", "."]);
        }
        fixture
            .command()
            .args(PLUGINS)
            .assert()
            .failure()
            .stdout(listed(&[PLUGINS[0], PLUGINS[2]]))
            .stderr(predicate::str::contains("checkout has uncommitted changes"));
    }
}

#[test]
fn rejects_invalid_missing_or_ambiguous_pins() {
    let fixture = Fixture::new();
    let path = fixture.plugin(0);
    let revision = &fixture.revisions[0];
    for config in [
        "[bootstrap.repos]\n".to_owned(),
        format!("[bootstrap.repos]\n\"{}\" = {{ ref = 42 }}\n", path.display()),
        format!("[bootstrap.repos]\n\"{}\" = {{ ref = \"main\" }}\n", path.display()),
        format!("[bootstrap.repos]\n\"{}\" = {{ ref = \"{}\" }}\n", path.display(), revision.to_uppercase()),
        format!("[bootstrap.repos]\n\"{}\" = {{ ref = \"{revision}\" }}\n\"{}/../{}\" = {{ ref = \"{revision}\" }}\n", path.display(), path.display(), path.file_name().unwrap().to_str().unwrap()),
    ] {
        fs::write(&fixture.config, config).unwrap();
        fixture.command().arg(PLUGINS[0]).assert().failure().stdout("");
    }
}

#[test]
fn config_errors_never_emit_plugins() {
    let fixture = Fixture::new();
    for config in ["[invalid", "[bootstrap]\n", "bootstrap = 12"] {
        fs::write(&fixture.config, config).unwrap();
        fixture
            .command()
            .args(PLUGINS)
            .assert()
            .failure()
            .stdout("");
    }
    fs::remove_file(&fixture.config).unwrap();
    fixture
        .command()
        .args(PLUGINS)
        .assert()
        .failure()
        .stdout("");
}

#[test]
fn resolves_xdg_and_explicit_config_paths() {
    let fixture = Fixture::new();
    fixture
        .command()
        .env("XDG_CONFIG_HOME", "")
        .args(PLUGINS)
        .assert()
        .success()
        .stdout(listed(&PLUGINS));
    let root = fixture.home.join("other config");
    fs::create_dir_all(root.join("mise")).unwrap();
    let config = root.join("mise/config.toml");
    fs::rename(&fixture.config, &config).unwrap();
    fixture
        .command()
        .env("XDG_CONFIG_HOME", &root)
        .args(PLUGINS)
        .assert()
        .success()
        .stdout(listed(&PLUGINS));
    fixture
        .command()
        .arg("--config")
        .arg(config)
        .args(PLUGINS)
        .assert()
        .success()
        .stdout(listed(&PLUGINS));
}

#[test]
fn missing_checkout_and_invalid_repository_names_are_rejected() {
    let fixture = Fixture::new();
    fs::remove_dir_all(fixture.plugin(0).join(".git")).unwrap();
    fixture
        .command()
        .args([
            PLUGINS[0],
            "../escape",
            "bad",
            "owner/../escape",
            "owner/name\ninjected",
        ])
        .assert()
        .failure()
        .stdout("");
}

#[test]
fn inherited_git_directory_cannot_redirect_plugin_verification() {
    let fixture = Fixture::new();
    fs::write(fixture.plugin(1).join("plugin.zsh"), "dirty\n").unwrap();
    fixture
        .command()
        .arg(PLUGINS[1])
        .env("GIT_DIR", fixture.plugin(0).join(".git"))
        .env("GIT_WORK_TREE", fixture.plugin(0))
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("uncommitted changes"));
}

#[test]
fn failed_git_status_is_not_treated_as_a_clean_checkout() {
    let fixture = Fixture::new();
    let bin = fixture.home.join("fake-bin");
    fs::create_dir_all(&bin).unwrap();
    executable(&bin.join("git"), &format!(
        "#!/bin/sh\nif [ \"$3\" = rev-parse ]; then\n  printf '%s\\n' '{}'\nelse\n  exit 1\nfi\n", fixture.revisions[0]
    ));
    fixture
        .command()
        .env("PATH", bin)
        .arg(PLUGINS[0])
        .assert()
        .failure()
        .stdout("")
        .stderr(predicate::str::contains("Git status failed"));
}

#[test]
#[ignore = "requires repository home/.zshrc and zsh; run explicitly in repository CI"]
fn shell_loads_verified_plugins_in_order_for_equivalent_toml_forms() {
    for format in ["inline", "compact", "single", "tables"] {
        let fixture = Fixture::new();
        fixture.write_config(format);
        let output = fixture.shell();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            fs::read_to_string(fixture.home.join("loaded")).unwrap(),
            listed(&PLUGINS)
        );
        assert!(!String::from_utf8_lossy(&output.stderr).contains("refusing"));
    }
}

#[test]
#[ignore = "requires repository home/.zshrc and zsh; run explicitly in repository CI"]
fn shell_keeps_valid_plugins_when_other_plugins_are_rejected() {
    let mut fixture = Fixture::new();
    fixture.revisions[1] = "0".repeat(40);
    fixture.write_config("tables");
    fs::write(fixture.plugin(2).join("stray.zwc"), "dirty\n").unwrap();
    let output = fixture.shell();
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        fs::read_to_string(fixture.home.join("loaded")).unwrap(),
        listed(&[PLUGINS[0]])
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(&format!("refusing {}", PLUGINS[1])));
    assert!(stderr.contains(&format!("refusing {}", PLUGINS[2])));
}

#[test]
#[ignore = "requires repository home/.zshrc and zsh; run explicitly in repository CI"]
fn shell_does_not_load_any_plugin_when_config_cannot_be_verified() {
    let fixture = Fixture::new();
    fs::write(&fixture.config, "[bootstrap.repos]\n").unwrap();
    let output = fixture.shell();
    assert!(output.status.success(), "{output:?}");
    assert!(!fixture.home.join("loaded").exists());
}

#[test]
#[ignore = "requires repository home/.zshrc and zsh; run explicitly in repository CI"]
fn shell_does_not_load_plugins_when_installed_cli_cannot_verify() {
    let fixture = Fixture::new();
    let old_cli = fixture.home.join("old-dotfiles");
    executable(
        &old_cli,
        "#!/bin/sh\nprintf 'unsupported command\\n' >&2\nexit 2\n",
    );
    let output = fixture.shell_with_cli(&old_cli);
    assert!(output.status.success(), "{output:?}");
    assert!(!fixture.home.join("loaded").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported command"));
}
