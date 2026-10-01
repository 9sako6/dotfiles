//! Public LaunchAgent declarations and read-only planning. Reconciliation is separate.
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declarations {
    #[serde(default)]
    agents: Vec<Agent>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Agent {
    label: String,
    argv: Vec<String>,
    #[serde(default)]
    run_at_load: bool,
    #[serde(default)]
    keep_alive: bool,
    start_interval: Option<u32>,
    start_calendar_interval: Option<Calendar>,
    working_directory: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Calendar {
    minute: Option<u8>,
    hour: Option<u8>,
    day: Option<u8>,
    weekday: Option<u8>,
    month: Option<u8>,
}

// Results only: no desired declarations or execution instructions live here.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Applied {
    version: u32,
    home: PathBuf,
    #[serde(deserialize_with = "unique_results")]
    agents: BTreeMap<String, String>,
}

fn unique_results<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<BTreeMap<String, String>, D::Error> {
    struct Results;
    impl<'de> serde::de::Visitor<'de> for Results {
        type Value = BTreeMap<String, String>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("unique LaunchAgent result labels")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            mut map: M,
        ) -> std::result::Result<Self::Value, M::Error> {
            let mut values = BTreeMap::new();
            while let Some((label, fingerprint)) = map.next_entry::<String, String>()? {
                if values.insert(label, fingerprint).is_some() {
                    return Err(serde::de::Error::custom(
                        "duplicate LaunchAgent result label",
                    ));
                }
            }
            Ok(values)
        }
    }
    deserializer.deserialize_map(Results)
}

#[derive(Default)]
pub(super) struct Plan {
    changes: BTreeMap<String, char>,
}

impl Plan {
    pub(super) fn capture(source: &Path, home: &Path) -> Result<Self> {
        let configured = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        let directory = configured
            .clone()
            .unwrap_or_else(|| home.join(".local/state"));
        let state = directory.join("dotfiles/user-services.json");
        Self::capture_with_anchor(source, home, &state, configured.as_deref().unwrap_or(home))
    }

    #[cfg(test)]
    fn capture_at(source: &Path, home: &Path, state: &Path) -> Result<Self> {
        Self::capture_with_anchor(source, home, state, home)
    }

    fn capture_with_anchor(
        source: &Path,
        home: &Path,
        state: &Path,
        state_anchor: &Path,
    ) -> Result<Self> {
        validate_path(home)?;
        let declarations = read_regular(&source.join("user-services.toml"), source)?
            .map(|bytes| -> Result<Declarations> {
                let text =
                    std::str::from_utf8(&bytes).context("invalid user-services.toml encoding")?;
                // Do not echo declaration contents in parser diagnostics.
                toml::from_str(text)
                    .map_err(|_| anyhow::anyhow!("invalid user-services.toml schema"))
            })
            .transpose()?
            .unwrap_or_default();
        let mut desired = BTreeMap::new();
        for agent in declarations.agents {
            let plist = agent.plist(home)?;
            if desired.insert(agent.label, plist).is_some() {
                bail!("duplicate public LaunchAgent label");
            }
        }
        let previous = read_regular(state, state_anchor)?
            .map(|bytes| -> Result<Applied> {
                let applied: Applied = serde_json::from_slice(&bytes)
                    .map_err(|_| anyhow::anyhow!("invalid user service result record"))?;
                if applied.version != 1 || applied.home != home {
                    bail!("user service result record has unsupported version or another home");
                }
                for (label, hash) in &applied.agents {
                    validate_label(label)?;
                    if hash.len() != 64
                        || !hash
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    {
                        bail!("invalid user service result fingerprint");
                    }
                }
                Ok(applied)
            })
            .transpose()?
            .map(|applied| applied.agents)
            .unwrap_or_default();
        let mut changes = BTreeMap::new();
        // Never enumerate LaunchAgents: unrelated/private files cannot become owned.
        let labels: BTreeSet<_> = desired.keys().chain(previous.keys()).collect();
        let mut casefolded = BTreeSet::new();
        for label in &labels {
            if !casefolded.insert(label.to_ascii_lowercase()) {
                bail!("case-insensitive public LaunchAgent label collision");
            }
        }
        for label in labels {
            let path = home
                .join("Library/LaunchAgents")
                .join(format!("{label}.plist"));
            let current = read_regular(&path, home)?;
            match (previous.get(label), &current) {
                (None, Some(_)) => bail!("unmanaged LaunchAgent conflict: {label}"),
                (Some(hash), Some(bytes)) if digest(bytes) != *hash => {
                    bail!("managed LaunchAgent changed outside dotfiles: {label}")
                }
                _ => {}
            }
            let operation = match (desired.get(label), previous.get(label), current) {
                (Some(_), None, _) => Some('+'),
                (Some(plist), Some(_), Some(bytes)) if plist.as_bytes() == bytes => None,
                (Some(_), Some(_), _) => Some('~'),
                (None, Some(_), _) => Some('-'),
                _ => None,
            };
            if let Some(operation) = operation {
                changes.insert(label.clone(), operation);
            }
        }
        Ok(Self { changes })
    }

    pub(super) fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    pub(super) fn render(&self) -> String {
        if !self.has_changes() {
            return String::new();
        }
        let mut text = String::from("user services (launchd; preview only)");
        for (label, operation) in &self.changes {
            text.push_str(&format!("\n  {operation} {label}"));
        }
        text
    }

    pub(super) fn ensure_applicable(&self) -> Result<()> {
        if self.has_changes() {
            bail!("user service changes are preview-only until the reconciliation backend is available; nothing was applied");
        }
        Ok(())
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_label(label: &str) -> Result<()> {
    if label.is_empty()
        || label.len() > 200
        || label.starts_with('.')
        || label.ends_with('.')
        || label.contains("..")
        || !label
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        bail!("invalid public LaunchAgent label");
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<()> {
    if value
        .chars()
        .any(|c| c.is_control() || c == '\u{fffe}' || c == '\u{ffff}')
    {
        bail!("LaunchAgent strings must not contain control characters");
    }
    Ok(())
}

fn validate_path(path: &Path) -> Result<()> {
    if path
        .to_str()
        .is_some_and(|text| text.split('/').any(|part| part == "." || part == ".."))
        || !path.is_absolute()
        || path
            .components()
            .any(|c| !matches!(c, Component::RootDir | Component::Normal(_)))
    {
        bail!("LaunchAgent path must be absolute without traversal");
    }
    validate_text(path.to_str().context("LaunchAgent path must be UTF-8")?)
}

fn expand_path(value: &str, home: &Path) -> Result<String> {
    let path = value
        .strip_prefix("~/")
        .map(|tail| home.join(tail))
        .unwrap_or_else(|| PathBuf::from(value));
    validate_path(&path)?;
    Ok(path
        .to_str()
        .context("LaunchAgent path must be UTF-8")?
        .to_owned())
}

fn read_regular(path: &Path, anchor: &Path) -> Result<Option<Vec<u8>>> {
    // The caller-selected HOME/source/XDG root may resolve through an OS alias.
    // Reject symlinks below that trusted anchor, including dangling links.
    for ancestor in path.ancestors().skip(1).take_while(|path| *path != anchor) {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) if !metadata.is_dir() => bail!(
                "user service parent must be a directory: {}",
                ancestor.display()
            ),
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(fs::read(path)?)),
        Ok(_) => bail!(
            "user service input must be a regular file: {}",
            path.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn xml(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

impl Agent {
    fn plist(&self, home: &Path) -> Result<String> {
        validate_label(&self.label)?;
        let executable = self
            .argv
            .first()
            .context("LaunchAgent argv must not be empty")?;
        for argument in &self.argv {
            validate_text(argument)?;
        }
        let executable = expand_path(executable, home)?;
        let name = Path::new(&executable)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if [
            "bash", "csh", "dash", "env", "fish", "ksh", "sh", "tcsh", "zsh",
        ]
        .contains(&name)
            || executable.starts_with("/nix/store/")
            || executable.contains("/installs/")
        {
            bail!("LaunchAgent executable must be a stable direct entrypoint, not a shell or pinned tool path");
        }
        if self.start_interval == Some(0)
            || (self.start_interval.is_some() && self.start_calendar_interval.is_some())
        {
            bail!("LaunchAgent requires a positive interval or a calendar schedule, not both");
        }
        let mut plist = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE plist PUBLIC \"-//Apple//DTD PLIST 1.0//EN\" \"http://www.apple.com/DTDs/PropertyList-1.0.dtd\">\n<plist version=\"1.0\">\n<dict>\n");
        plist.push_str(&format!(
            "  <key>Label</key><string>{}</string>\n  <key>ProgramArguments</key>\n  <array>\n",
            xml(&self.label)
        ));
        for argument in std::iter::once(&executable).chain(self.argv.iter().skip(1)) {
            plist.push_str(&format!("    <string>{}</string>\n", xml(argument)));
        }
        plist.push_str("  </array>\n");
        for (key, value) in [
            ("RunAtLoad", self.run_at_load),
            ("KeepAlive", self.keep_alive),
        ] {
            plist.push_str(&format!("  <key>{key}</key><{value}/>\n"));
        }
        if let Some(interval) = self.start_interval {
            plist.push_str(&format!(
                "  <key>StartInterval</key><integer>{interval}</integer>\n"
            ));
        }
        if let Some(calendar) = &self.start_calendar_interval {
            let values = [
                ("Minute", calendar.minute, 0, 59),
                ("Hour", calendar.hour, 0, 23),
                ("Day", calendar.day, 1, 31),
                ("Weekday", calendar.weekday, 0, 7),
                ("Month", calendar.month, 1, 12),
            ];
            if values.iter().all(|(_, value, _, _)| value.is_none()) {
                bail!("empty LaunchAgent calendar schedule");
            }
            plist.push_str("  <key>StartCalendarInterval</key>\n  <dict>\n");
            for (key, value, min, max) in values {
                if let Some(value) = value {
                    if !(min..=max).contains(&value) {
                        bail!("invalid LaunchAgent calendar value: {key}");
                    }
                    plist.push_str(&format!("    <key>{key}</key><integer>{value}</integer>\n"));
                }
            }
            plist.push_str("  </dict>\n");
        }
        if let Some(directory) = &self.working_directory {
            plist.push_str(&format!(
                "  <key>WorkingDirectory</key><string>{}</string>\n",
                xml(&expand_path(directory, home)?)
            ));
        }
        plist.push_str("</dict>\n</plist>\n");
        Ok(plist)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const DECLARATION: &str = "[[agents]]\nlabel = 'com.example.check'\nargv = ['~/.local/share/mise/shims/node', 'hello<&\".js']\nrun_at_load = true\nstart_interval = 60\n";

    struct Fixture {
        _root: tempfile::TempDir,
        source: PathBuf,
        home: PathBuf,
        state: PathBuf,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let root_path = root.path().canonicalize().unwrap();
            let source = root_path.join("source");
            let home = root_path.join("home");
            let state = home.join(".local/state/dotfiles/user-services.json");
            fs::create_dir(&source).unwrap();
            fs::create_dir_all(home.join("Library/LaunchAgents")).unwrap();
            Self {
                _root: root,
                source,
                home,
                state,
            }
        }
        fn declare(&self, text: &str) {
            fs::write(self.source.join("user-services.toml"), text).unwrap();
        }
        fn plan(&self) -> Result<Plan> {
            Plan::capture_at(&self.source, &self.home, &self.state)
        }
        fn plist(&self, label: &str) -> PathBuf {
            self.home
                .join("Library/LaunchAgents")
                .join(format!("{label}.plist"))
        }
        fn applied(&self) {
            let declarations: Declarations = toml::from_str(DECLARATION).unwrap();
            let plist = declarations.agents[0].plist(&self.home).unwrap();
            fs::write(self.plist("com.example.check"), &plist).unwrap();
            fs::create_dir_all(self.state.parent().unwrap()).unwrap();
            fs::write(&self.state, serde_json::to_vec(&serde_json::json!({
                "version": 1, "home": self.home, "agents": {"com.example.check": digest(plist.as_bytes())}
            })).unwrap()).unwrap();
        }
    }

    #[test]
    fn deterministic_plist_escapes_arguments_and_expands_only_paths() {
        let declarations: Declarations = toml::from_str(DECLARATION).unwrap();
        let plist = declarations.agents[0]
            .plist(Path::new("/Users/test"))
            .unwrap();
        assert_eq!(
            plist,
            declarations.agents[0]
                .plist(Path::new("/Users/test"))
                .unwrap()
        );
        assert!(plist.contains("<string>/Users/test/.local/share/mise/shims/node</string>"));
        assert!(plist.contains("<string>hello&lt;&amp;&quot;.js</string>"));
        assert!(plist.contains("<key>RunAtLoad</key><true/>"));
        assert!(plist.contains("<key>StartInterval</key><integer>60</integer>"));
        let calendar: Declarations = toml::from_str("[[agents]]\nlabel='com.example.calendar'\nargv=['/usr/bin/true']\nworking_directory='~/jobs'\n[agents.start_calendar_interval]\nhour=9\nminute=30\nweekday=1").unwrap();
        let plist = calendar.agents[0].plist(Path::new("/Users/test")).unwrap();
        assert!(plist.contains("<key>Hour</key><integer>9</integer>"));
        assert!(plist.contains("<string>/Users/test/jobs</string>"));
    }

    #[test]
    fn plans_add_change_delete_missing_and_noop_without_writes() {
        let f = Fixture::new();
        assert!(!f.plan().unwrap().has_changes());
        f.declare(DECLARATION);
        assert_eq!(f.plan().unwrap().changes["com.example.check"], '+');
        assert!(!f.state.exists());
        assert!(!f.plist("com.example.check").exists());
        f.applied();
        let before = fs::read(&f.state).unwrap();
        assert!(!f.plan().unwrap().has_changes());
        f.declare(&DECLARATION.replace("60", "120"));
        assert_eq!(f.plan().unwrap().changes["com.example.check"], '~');
        f.declare("");
        assert_eq!(f.plan().unwrap().changes["com.example.check"], '-');
        fs::remove_file(f.plist("com.example.check")).unwrap();
        assert_eq!(f.plan().unwrap().changes["com.example.check"], '-');
        f.declare(DECLARATION);
        assert_eq!(f.plan().unwrap().changes["com.example.check"], '~');
        assert_eq!(before, fs::read(&f.state).unwrap());
    }

    #[test]
    fn unmanaged_private_and_system_files_are_never_adopted() {
        let f = Fixture::new();
        fs::write(f.plist("private.agent"), b"private").unwrap();
        fs::create_dir_all(f.home.join("Library/LaunchDaemons")).unwrap();
        fs::write(f.home.join("Library/LaunchDaemons/system.plist"), b"system").unwrap();
        assert!(!f.plan().unwrap().has_changes());
        f.declare(DECLARATION);
        fs::write(f.plist("com.example.check"), b"existing unmanaged").unwrap();
        assert!(f.plan().err().unwrap().to_string().contains("unmanaged"));
        assert_eq!(fs::read(f.plist("private.agent")).unwrap(), b"private");
    }

    #[test]
    fn malformed_inputs_and_modified_owned_files_fail_closed() {
        let f = Fixture::new();
        for declaration in [
            "pre_apply = 'sudo true'".to_owned(),
            DECLARATION.replace("com.example.check", "../bad"),
            DECLARATION.repeat(2),
            DECLARATION.replace("60", "0"),
            DECLARATION.replace("~/.local/share/mise/shims/node", "/bin/sh"),
            DECLARATION.replace("~/.local/share/mise/shims/node", "/nix/store/hash/bin/node"),
            DECLARATION.replace("shims/node", "installs/node/1/bin/node"),
            DECLARATION.replace("~/.local/share/mise/shims/node", "node"),
            DECLARATION.replace("hello", "\u{1b}hello"),
            format!("{DECLARATION}\n[agents.start_calendar_interval]\nhour=24"),
            DECLARATION.replace("start_interval = 60", "[agents.start_calendar_interval]"),
        ] {
            f.declare(&declaration);
            assert!(f.plan().is_err(), "accepted {declaration:?}");
        }
        f.declare(DECLARATION);
        f.applied();
        fs::write(f.plist("com.example.check"), b"changed").unwrap();
        assert!(f.plan().is_err());
        f.applied();
        fs::write(&f.state, b"broken").unwrap();
        assert!(f.plan().is_err());
        f.applied();
        let record = fs::read_to_string(&f.state)
            .unwrap()
            .replace("com.example.check", "../outside");
        fs::write(&f.state, record).unwrap();
        assert!(f.plan().is_err());
    }

    #[test]
    fn symlinks_and_dangling_links_are_conflicts() {
        let f = Fixture::new();
        f.declare(DECLARATION);
        symlink("/missing", f.plist("com.example.check")).unwrap();
        assert!(f.plan().is_err());
        fs::remove_file(f.plist("com.example.check")).unwrap();
        fs::remove_dir(f.home.join("Library/LaunchAgents")).unwrap();
        symlink("/missing", f.home.join("Library/LaunchAgents")).unwrap();
        assert!(f.plan().is_err());
    }

    #[test]
    fn result_record_rejects_duplicate_labels_and_wrong_home() {
        let f = Fixture::new();
        f.declare(DECLARATION);
        f.applied();
        let text = fs::read_to_string(&f.state).unwrap();
        let duplicated = text.replace(
            "\"agents\":{",
            &format!("\"agents\":{{\"com.example.check\":\"{}\",", "0".repeat(64)),
        );
        fs::write(&f.state, duplicated).unwrap();
        assert!(f.plan().is_err());
        let mut value: serde_json::Value = serde_json::from_str(&text).unwrap();
        value["home"] = serde_json::json!("/another-home");
        fs::write(&f.state, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(f.plan().is_err());
        assert!(expand_path("/tmp/./file", &f.home).is_err());
        assert!(expand_path("/tmp/../file", &f.home).is_err());
    }

    #[test]
    fn trusted_home_and_source_aliases_work_but_children_do_not() {
        let f = Fixture::new();
        let home_alias = f._root.path().join("home-alias");
        let source_alias = f._root.path().join("source-alias");
        symlink(&f.home, &home_alias).unwrap();
        symlink(&f.source, &source_alias).unwrap();
        let state = home_alias.join(".local/state/dotfiles/user-services.json");
        assert!(!Plan::capture_at(&source_alias, &home_alias, &state)
            .unwrap()
            .has_changes());
        f.declare(DECLARATION);
        assert!(Plan::capture_at(&source_alias, &home_alias, &state)
            .unwrap()
            .has_changes());
        fs::remove_dir(f.home.join("Library/LaunchAgents")).unwrap();
        symlink("/missing", f.home.join("Library/LaunchAgents")).unwrap();
        assert!(Plan::capture_at(&source_alias, &home_alias, &state).is_err());
    }

    #[test]
    fn explicit_state_root_alias_and_case_collisions() {
        let f = Fixture::new();
        let actual = f._root.path().join("state-actual");
        fs::create_dir(&actual).unwrap();
        let alias = f.home.join("state-alias");
        symlink(&actual, &alias).unwrap();
        let state = alias.join("dotfiles/user-services.json");
        assert!(
            !Plan::capture_with_anchor(&f.source, &f.home, &state, &alias)
                .unwrap()
                .has_changes()
        );
        f.declare(&format!(
            "{DECLARATION}{}",
            DECLARATION.replace("com.example.check", "com.example.Check")
        ));
        assert!(f.plan().is_err());
        f.applied();
        f.declare(&DECLARATION.replace("com.example.check", "com.example.Check"));
        assert!(f.plan().is_err());
    }

    #[test]
    fn pending_services_block_unified_apply_before_confirmation() {
        let f = Fixture::new();
        f.declare(DECLARATION);
        let plan = f.plan().unwrap();
        let preview = crate::inventory::Preview::copy_only();
        assert!(matches!(
            super::super::review_plan_with(
                super::super::Mode::Plan,
                &preview,
                None,
                Some(&plan),
                || panic!("plan confirmation")
            ),
            Ok(super::super::Review::Finished)
        ));
        let result = super::super::review_plan_with(
            super::super::Mode::Apply,
            &preview,
            None,
            Some(&plan),
            || panic!("must fail before confirmation"),
        );
        assert!(result.err().unwrap().to_string().contains("preview-only"));
        assert!(!f.state.exists());
    }
}
