use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tabled::builder::Builder;
use tabled::settings::peaker::PriorityMax;
use tabled::settings::{Style, Width};

#[derive(Deserialize)]
struct Configuration {
    tools: BTreeMap<String, Request>,
    #[serde(default)]
    settings: Settings,
}

#[derive(Default, Deserialize)]
struct Settings {
    #[serde(default)]
    disable_tools: BTreeSet<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Version {
    String(String),
    Options { version: String },
}

impl Version {
    fn requested(&self) -> &str {
        match self {
            Self::String(version) | Self::Options { version } => version,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Request {
    Single(Version),
    Multiple(Vec<Version>),
}

impl Request {
    fn versions(&self) -> Vec<&str> {
        match self {
            Self::Single(version) => vec![version.requested()],
            Self::Multiple(versions) => versions.iter().map(Version::requested).collect(),
        }
    }
}

#[derive(Deserialize)]
struct InstalledVersion {
    version: String,
    requested_version: Option<String>,
    source: Option<Source>,
    installed: bool,
}

#[derive(Deserialize)]
struct Source {
    path: PathBuf,
}

#[derive(Debug, PartialEq)]
enum Status {
    Disabled,
    Installed,
    Missing,
    VersionChange,
}

struct Tool {
    name: String,
    installed: Vec<String>,
    desired: Vec<String>,
    status: Status,
}

/// Current declarations only; this does not invoke mise or inspect installs.
pub(super) fn declared_inventory(source: &Path) -> Result<Vec<serde_json::Value>> {
    let bytes = super::snapshot::read_regular(
        source,
        Path::new("home/.config/mise/config.toml"),
        "global mise configuration",
    )?;
    super::snapshot::read_regular(
        source,
        Path::new("home/.config/mise/mise.lock"),
        "global mise lock",
    )?;
    let configuration: Configuration = toml::from_str(
        std::str::from_utf8(&bytes).context("invalid global mise configuration encoding")?,
    )
    .map_err(|_| anyhow::anyhow!("cannot parse global mise configuration"))?;
    Ok(configuration
        .tools
        .iter()
        .map(|(name, request)| {
            serde_json::json!({
                "name": name, "manager": "mise", "declared": request.versions().join(", "),
                "disabled": configuration.settings.disable_tools.contains(name)
            })
        })
        .collect())
}

pub(super) struct Plan {
    configuration: Configuration,
    files: [(PathBuf, Vec<u8>); 2],
    home: PathBuf,
    mise: PathBuf,
    tools: Vec<Tool>,
    workspace: tempfile::TempDir,
}

impl Plan {
    pub(super) fn capture(source: &Path, home: &Path, mise: &Path) -> Result<Self> {
        let workspace = tempfile::Builder::new()
            .prefix("dotfiles-tools-")
            .tempdir_in(env::temp_dir().canonicalize()?)?;
        let mut files = [
            (PathBuf::from("config.toml"), Vec::new()),
            (PathBuf::from("mise.lock"), Vec::new()),
        ];
        for (name, bytes) in &mut files {
            *bytes = super::snapshot::read_regular(
                source,
                &Path::new("home/.config/mise").join(&name),
                &format!("global mise {}", name.display()),
            )?;
            fs::write(workspace.path().join(name), &bytes)?;
        }
        let configuration: Configuration = toml::from_str(std::str::from_utf8(&files[0].1)?)
            .map_err(|_| anyhow::anyhow!("cannot parse global mise configuration"))?;
        let mut plan = Self {
            configuration,
            files,
            home: home.to_path_buf(),
            mise: mise.to_path_buf(),
            tools: Vec::new(),
            workspace,
        };
        plan.tools = plan.inspect()?;
        Ok(plan)
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.mise);
        for (name, _) in env::vars_os() {
            let text = name.to_string_lossy();
            if text.starts_with("MISE_") && text.ends_with("_VERSION") {
                command.env_remove(name);
            }
        }
        command
            .current_dir(self.workspace.path())
            .env("HOME", &self.home)
            .env("MISE_AUTO_INSTALL", "false")
            .env("MISE_CEILING_PATHS", self.workspace.path())
            .env("MISE_COLOR", "false")
            .env("MISE_GLOBAL_CONFIG_ROOT", &self.home)
            .env("MISE_STATE_DIR", self.workspace.path().join("state"))
            .env(
                "MISE_CONFIG_FILE",
                self.workspace.path().join("config.toml"),
            )
            .env(
                "MISE_GLOBAL_CONFIG_FILE",
                self.workspace.path().join("config.toml"),
            )
            .env(
                "MISE_SYSTEM_CONFIG_FILE",
                self.workspace.path().join("absent.toml"),
            )
            .env(
                "MISE_DISABLE_TOOLS",
                self.configuration
                    .settings
                    .disable_tools
                    .iter()
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(","),
            )
            .env_remove("MISE_ENV")
            .env_remove("MISE_ENV_FILE")
            .env_remove("MISE_IGNORED_CONFIG_PATHS");
        command
    }

    fn verify(&self) -> Result<()> {
        for (name, bytes) in &self.files {
            if fs::read(self.workspace.path().join(name))? != *bytes {
                bail!("mise changed the frozen config or lock; run plan/apply again");
            }
        }
        Ok(())
    }

    fn inspect(&self) -> Result<Vec<Tool>> {
        self.verify()?;
        let versions: BTreeMap<String, Vec<InstalledVersion>> = if self
            .configuration
            .tools
            .keys()
            .all(|name| self.configuration.settings.disable_tools.contains(name))
        {
            BTreeMap::new()
        } else {
            let output = super::capture(
                self.command().args(["ls", "--json", "--locked"]),
                "cannot inspect mise tools",
            )?;
            serde_json::from_slice(&output).context("invalid mise tools JSON")?
        };
        self.verify()?;
        let config_path = self.workspace.path().join("config.toml");
        self.configuration.tools.iter().map(|(name, request)| {
            let requested = request.versions();
            if requested.is_empty() {
                bail!("mise tool {name} has no requested version");
            }
            if self.configuration.settings.disable_tools.contains(name) {
                return Ok(Tool {
                    name: name.clone(),
                    installed: Vec::new(),
                    desired: requested.into_iter().map(str::to_owned).collect(),
                    status: Status::Disabled,
                });
            }
            let records = versions.get(name).map(Vec::as_slice).unwrap_or_default();
            let desired = requested.iter().map(|requested| {
                let matches = records.iter().filter(|record| {
                    record.requested_version.as_deref() == Some(*requested)
                        && record.source.as_ref().is_some_and(|source| source.path == config_path)
                }).collect::<Vec<_>>();
                match matches.as_slice() {
                    [record] => Ok(*record),
                    _ => bail!("mise did not resolve {name}@{requested} from the global config and lock"),
                }
            }).collect::<Result<Vec<_>>>()?;
            let installed = records.iter().filter(|record| record.installed)
                .map(|record| record.version.clone()).collect::<BTreeSet<_>>()
                .into_iter().collect::<Vec<_>>();
            let status = if desired.iter().all(|record| record.installed) {
                Status::Installed
            } else if installed.is_empty() {
                Status::Missing
            } else {
                Status::VersionChange
            };
            Ok(Tool {
                name: name.clone(),
                installed,
                desired: desired.iter().map(|record| record.version.clone()).collect(),
                status,
            })
        }).collect()
    }

    pub(super) fn apply(&mut self, lock: &File) -> Result<()> {
        self.verify()?;
        if !self.has_changes() {
            return Ok(());
        }
        let pending = self
            .tools
            .iter()
            .filter(|tool| matches!(tool.status, Status::Missing | Status::VersionChange))
            .map(|tool| tool.name.clone())
            .collect::<BTreeSet<_>>();
        let mut command = self.command();
        command.args(["install", "--locked"]);
        super::retain_apply_lock(&mut command, lock);
        let status = command.status();
        self.verify().context("tools may be partially installed")?;
        self.tools = self.inspect().context("cannot verify tools after installation; some tools may be installed. Run plan/apply again")?;
        if !status.context("cannot start mise install")?.success() || self.has_changes() {
            let completed = self
                .tools
                .iter()
                .filter(|tool| pending.contains(&tool.name) && tool.status == Status::Installed)
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>();
            let remaining = self
                .tools
                .iter()
                .filter(|tool| matches!(tool.status, Status::Missing | Status::VersionChange))
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>();
            bail!("tools install did not complete; installed in this attempt: {}; still missing: {}. Some tools may have changed. Run plan/apply again",
                if completed.is_empty() { "none".into() } else { completed.join(", ") },
                if remaining.is_empty() { "none".into() } else { remaining.join(", ") });
        }
        Ok(())
    }

    pub(super) fn has_changes(&self) -> bool {
        self.tools
            .iter()
            .any(|tool| matches!(tool.status, Status::Missing | Status::VersionChange))
    }

    pub(super) fn render(&self, width: Option<usize>) -> String {
        if self.tools.is_empty() {
            return String::new();
        }
        let mut builder = Builder::default();
        builder.push_record(["name", "installed", "desired", "status"]);
        for tool in &self.tools {
            builder.push_record([
                tool.name.clone(),
                if tool.installed.is_empty() {
                    "—".into()
                } else {
                    tool.installed.join(", ")
                },
                tool.desired.join(", "),
                match tool.status {
                    Status::Disabled => "disabled",
                    Status::Installed => "installed",
                    Status::Missing => "missing",
                    Status::VersionChange => "version change",
                }
                .into(),
            ]);
        }
        let mut table = builder.build();
        table.with(Style::empty());
        if let Some(width) = width {
            table.with(Width::wrap(width).priority(PriorityMax::default()));
        }
        format!(
            "tools\n{}",
            table
                .to_string()
                .lines()
                .map(str::trim_end)
                .collect::<Vec<_>>()
                .join("\n")
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        _temporary: tempfile::TempDir,
        source: PathBuf,
        home: PathBuf,
        mise: PathBuf,
    }

    impl Fixture {
        fn new(config: &str, json: &str) -> Self {
            let temporary = tempfile::tempdir().unwrap();
            let source = temporary.path().join("source");
            let home = temporary.path().join("home");
            fs::create_dir_all(source.join("home/.config/mise")).unwrap();
            fs::create_dir_all(&home).unwrap();
            fs::write(source.join("home/.config/mise/config.toml"), config).unwrap();
            fs::write(source.join("home/.config/mise/mise.lock"), "[tools]\n").unwrap();
            let output = temporary.path().join("output.json");
            fs::write(&output, json).unwrap();
            let mise = temporary.path().join("mise");
            fs::write(
                &mise,
                format!(
                    r#"#!/bin/sh
set -eu
[ "$*" = 'ls --json --locked' ]
[ "$MISE_CONFIG_FILE" = "$MISE_GLOBAL_CONFIG_FILE" ]
[ "$PWD" = "$MISE_CEILING_PATHS" ]
[ "$MISE_AUTO_INSTALL" = false ]
[ -f "$PWD/mise.lock" ]
sed "s|CONFIG|$MISE_GLOBAL_CONFIG_FILE|g" '{}'
"#,
                    output.display()
                ),
            )
            .unwrap();
            fs::set_permissions(&mise, fs::Permissions::from_mode(0o755)).unwrap();
            Self {
                _temporary: temporary,
                source,
                home,
                mise,
            }
        }

        fn capture(&self) -> Result<Plan> {
            Plan::capture(&self.source, &self.home, &self.mise)
        }
    }

    #[test]
    fn json_distinguishes_installed_missing_version_changes_and_disabled_tools() {
        let fixture = Fixture::new(
            r#"[tools]
a-disabled = "1.0.0"
b-installed = { version = "2.0.0", components = ["extra"] }
c-missing = "3.0.0"
d-updated = "4.0.0"
[settings]
disable_tools = ["a-disabled"]
"#,
            r#"{
  "b-installed": [{"version":"2.0.0","requested_version":"2.0.0","installed":true,"source":{"path":"CONFIG"}}],
  "c-missing": [{"version":"3.0.0","requested_version":"3.0.0","installed":false,"source":{"path":"CONFIG"}}],
  "d-updated": [{"version":"3.0.0","installed":true},{"version":"4.0.0","requested_version":"4.0.0","installed":false,"source":{"path":"CONFIG"}}],
  "unmanaged": [{"version":"5.0.0","installed":true}]
}"#,
        );
        let config = fs::read(fixture.source.join("home/.config/mise/config.toml")).unwrap();
        let lock = fs::read(fixture.source.join("home/.config/mise/mise.lock")).unwrap();
        let plan = fixture.capture().unwrap();
        assert!(plan.has_changes());
        assert_eq!(
            plan.tools
                .iter()
                .map(|tool| &tool.status)
                .collect::<Vec<_>>(),
            vec![
                &Status::Disabled,
                &Status::Installed,
                &Status::Missing,
                &Status::VersionChange
            ]
        );
        assert_eq!(plan.tools[3].installed, ["3.0.0"]);
        assert_eq!(plan.tools[3].desired, ["4.0.0"]);
        let rendered = plan.render(None);
        for value in [
            "a-disabled",
            "b-installed",
            "c-missing",
            "d-updated",
            "disabled",
            "installed",
            "missing",
            "version change",
        ] {
            assert!(rendered.contains(value), "{rendered}");
        }
        assert!(!rendered.contains("unmanaged"));
        assert_eq!(
            fs::read(fixture.source.join("home/.config/mise/config.toml")).unwrap(),
            config
        );
        assert_eq!(
            fs::read(fixture.source.join("home/.config/mise/mise.lock")).unwrap(),
            lock
        );
    }

    #[test]
    fn incomplete_or_unrelated_resolution_is_not_a_no_op() {
        for json in [
            "{}",
            r#"{"node":[{"version":"1.0.0","requested_version":"1.0.0","installed":true,"source":{"path":"elsewhere"}}]}"#,
            r#"{"node":[{"version":"1.0.0","requested_version":"1.0.0","installed":true,"source":{"path":"CONFIG"}}]}"#,
        ] {
            let fixture = Fixture::new("[tools]\nnode = ['1.0.0', '2.0.0']\n", json);
            assert!(fixture
                .capture()
                .err()
                .unwrap()
                .to_string()
                .contains("did not resolve"));
        }
        let fixture = Fixture::new("[tools]\nnode = '1.0.0'\n", "not JSON");
        assert!(fixture
            .capture()
            .err()
            .unwrap()
            .to_string()
            .contains("invalid mise tools JSON"));
    }

    #[test]
    fn installed_and_disabled_tools_do_not_need_action() {
        let fixture = Fixture::new(
            "[tools]\nnode = '1.0.0'\n",
            r#"{"node":[{"version":"0.9.0","installed":true},{"version":"1.0.0","requested_version":"1.0.0","installed":true,"source":{"path":"CONFIG"}}]}"#,
        );
        assert!(!fixture.capture().unwrap().has_changes());
        let fixture = Fixture::new(
            "[tools]\nnode = '1.0.0'\n[settings]\ndisable_tools = ['node']\n",
            "not JSON",
        );
        fs::remove_file(&fixture.mise).unwrap();
        let plan = fixture.capture().unwrap();
        assert!(!plan.has_changes());
        assert!(plan.render(None).contains("disabled"));
    }

    #[test]
    fn install_lock_fixture() {
        let Some(root) = env::var_os("DOTFILES_TEST_TOOLS_LOCK_ROOT").map(PathBuf::from) else {
            return;
        };
        let lock = super::super::acquire_lock(&root.join("apply.lock")).unwrap();
        let mut plan =
            Plan::capture(&root.join("source"), &root.join("home"), &root.join("mise")).unwrap();
        plan.apply(&lock).unwrap();
    }

    #[test]
    fn installer_retains_the_common_lock_after_its_owner_is_killed() {
        use std::io::{BufRead, BufReader, Read};
        use std::process::Stdio;
        use std::time::{Duration, Instant};

        let fixture = Fixture::new(
            "[tools]\nnode = '1.0.0'\n",
            r#"{"node":[{"version":"1.0.0","requested_version":"1.0.0","installed":false,"source":{"path":"CONFIG"}}]}"#,
        );
        let root = fixture._temporary.path();
        fs::write(
            &fixture.mise,
            format!(
                r#"#!/bin/sh
set -eu
case "$*" in
  'ls --json --locked') sed "s|CONFIG|$MISE_GLOBAL_CONFIG_FILE|g" '{}/output.json' ;;
  'install --locked') printf 'installer-ready\n'; while [ ! -e '{}/release' ]; do sleep 0.02; done ;;
  *) exit 77 ;;
esac
"#,
                root.display(), root.display()
            ),
        )
        .unwrap();
        let mut owner = Command::new(env::current_exe().unwrap())
            .args([
                "--exact",
                "system::tools::tests::install_lock_fixture",
                "--nocapture",
            ])
            .env("DOTFILES_TEST_TOOLS_LOCK_ROOT", root)
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut output = BufReader::new(owner.stdout.take().unwrap());
        loop {
            let mut line = String::new();
            assert!(
                output.read_line(&mut line).unwrap() > 0,
                "installer did not start"
            );
            if line.trim() == "installer-ready" {
                break;
            }
        }
        owner.kill().unwrap();
        owner.wait().unwrap();
        let path = root.join("apply.lock");
        let excluded = super::super::acquire_lock(&path).is_err();
        fs::write(root.join("release"), "").unwrap();
        output.read_to_end(&mut Vec::new()).unwrap();
        assert!(excluded);
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if super::super::acquire_lock(&path).is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "finished installer retained the lock"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    #[ignore = "requires the pinned mise executable"]
    fn real_mise_uses_only_frozen_global_inputs_and_rejects_stale_lock() {
        let mise = env::var_os("DOTFILES_TEST_MISE").expect("DOTFILES_TEST_MISE is required");
        let fixture = Fixture::new("[tools]\nnode = '23.0.0'\nripgrep = '14.0.0'\n[env]\nAGY_CLI_DISABLE_AUTO_UPDATE = 'true'\n[settings]\nactivate_aggressive = true\nauto_install = false\ndisable_tools = ['ripgrep']\n[settings.github]\ncredential_command = 'false'\n", "");
        let config = fixture.source.join("home/.config/mise/config.toml");
        let lock = fixture.source.join("home/.config/mise/mise.lock");
        fs::write(
            &lock,
            "[[tools.node]]\nversion = '23.0.0'\nbackend = 'core:node'\n",
        )
        .unwrap();
        fs::create_dir_all(fixture.home.join(".local/share/mise/installs/node/22.0.0")).unwrap();
        fs::create_dir_all(fixture.home.join(".config/mise")).unwrap();
        fs::write(
            fixture.home.join(".config/mise/config.toml"),
            "[tools]\npython = '1.0.0'\n",
        )
        .unwrap();
        let wrapper = fixture._temporary.path().join("real-mise");
        fs::write(&wrapper, format!("#!/bin/sh\nexport MISE_DATA_DIR=\"$HOME/.local/share/mise\"\nexport MISE_CACHE_DIR=\"$HOME/.cache/mise\"\nexport XDG_CONFIG_HOME=\"$HOME/.config\"\nexec '{}' \"$@\"\n", Path::new(&mise).display())).unwrap();
        fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
        let before = fs::read(&lock).unwrap();
        let plan = Plan::capture(&fixture.source, &fixture.home, &wrapper).unwrap();
        assert_eq!(plan.tools.len(), 2);
        assert_eq!(plan.tools[0].status, Status::VersionChange);
        assert_eq!(plan.tools[1].status, Status::Disabled);
        assert!(!fixture.home.join(".local/state/mise").exists());
        assert_eq!(fs::read(&lock).unwrap(), before);
        fs::write(&config, "[tools]\nnode = '24.0.0'\n").unwrap();
        assert!(Plan::capture(&fixture.source, &fixture.home, &wrapper)
            .err()
            .unwrap()
            .to_string()
            .contains("did not resolve"));
    }
}

#[cfg(test)]
mod declaration_tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn parser_inputs_reject_external_symlinks_before_invoking_mise() {
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret");
        fs::write(&secret, "secret-do-not-print").unwrap();
        for name in ["config.toml", "mise.lock"] {
            let source = tempfile::tempdir().unwrap();
            let home = tempfile::tempdir().unwrap();
            let directory = source.path().join("home/.config/mise");
            fs::create_dir_all(&directory).unwrap();
            fs::write(directory.join("config.toml"), "[tools]\n").unwrap();
            fs::write(directory.join("mise.lock"), "").unwrap();
            fs::remove_file(directory.join(name)).unwrap();
            symlink(&secret, directory.join(name)).unwrap();
            let inventory_error = declared_inventory(source.path()).unwrap_err();
            let plan_error =
                Plan::capture(source.path(), home.path(), Path::new("/never-invoke-mise"))
                    .err()
                    .unwrap();
            for error in [inventory_error, plan_error] {
                assert!(!format!("{error:#}").contains("secret-do-not-print"));
                assert!(format!("{error:#}").contains("regular file"));
            }
            assert_eq!(fs::read_to_string(&secret).unwrap(), "secret-do-not-print");
            assert_eq!(fs::read_dir(home.path()).unwrap().count(), 0);
        }
    }
}
