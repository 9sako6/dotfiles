//! Public LaunchAgent declarations and conservative per-user reconciliation.
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, Output};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
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
#[derive(Serialize, Deserialize)]
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
    desired: BTreeMap<String, String>,
    previous: BTreeMap<String, String>,
    files: BTreeMap<String, Option<Vec<u8>>>,
    loaded: BTreeMap<String, bool>,
    declaration: Option<Vec<u8>>,
    source: PathBuf,
    home: PathBuf,
    state: PathBuf,
    state_anchor: PathBuf,
    stable_home: PathBuf,
    stable_source: PathBuf,
    stable_state_anchor: PathBuf,
    state_bytes: Option<Vec<u8>>,
    launchctl: Launchctl,
}

#[derive(Default)]
struct Launchctl {
    executable: PathBuf,
    uid: u32,
}

impl Launchctl {
    fn domain(&self) -> String {
        format!("gui/{}", self.uid)
    }

    fn run(&self, args: &[&str], lock: Option<&File>) -> Result<Output> {
        let mut command = Command::new(&self.executable);
        command.args(args).env("LC_ALL", "C").env("LANG", "C");
        if let Some(lock) = lock {
            super::retain_apply_lock(&mut command, lock);
        }
        command.output().context("cannot run launchctl")
    }

    fn observe(&self, label: &str, path: &Path, lock: Option<&File>) -> Result<bool> {
        let domain = self.domain();
        let target = format!("{domain}/{label}");
        let output = self.run(&["print", &target], lock)?;
        if !output.status.success() {
            // Status 113 alone is insufficient: a missing GUI domain, access errors,
            // unsupported commands, or unknown diagnostics must never mean absent.
            let missing = format!(
                "Could not find service \"{label}\" in domain for user gui: {}",
                self.uid
            );
            let error = std::str::from_utf8(&output.stderr)
                .unwrap_or_default()
                .trim();
            if output.status.code() == Some(113)
                && output.stdout.is_empty()
                && (error == missing || error == format!("Bad request.\n{missing}"))
                && self.run(&["print", &domain], lock)?.status.success()
            {
                return Ok(false);
            }
            bail!("cannot establish launchd service presence: {label}");
        }
        // Apple's print output is explicitly not an API. Recognize only this
        // conservative shape; a format change requires review, never bootout.
        let text =
            std::str::from_utf8(&output.stdout).context("unrecognized launchctl print encoding")?;
        let mut lines = text.lines();
        if lines.next() != Some(format!("{target} = {{").as_str()) || !output.stderr.is_empty() {
            bail!("unrecognized launchctl service identity: {label}");
        }
        let mut source = None;
        let mut kind = None;
        let mut closed = false;
        for line in lines {
            if line == "}" {
                if closed {
                    bail!("unrecognized launchctl service structure");
                }
                closed = true;
            } else if closed && !line.is_empty() {
                bail!("unrecognized launchctl trailing output");
            }
            // Exact one-tab top-level fields cannot be supplied by nested
            // environment/argument values. Never trim arbitrary indentation.
            if let Some(value) = line.strip_prefix("\tpath = ") {
                if source.replace(value).is_some() {
                    bail!("duplicate launchctl path");
                }
            }
            if let Some(value) = line.strip_prefix("\ttype = ") {
                if kind.replace(value).is_some() {
                    bail!("duplicate launchctl type");
                }
            }
        }
        if !closed || source != path.to_str() || kind != Some("LaunchAgent") {
            bail!("loaded LaunchAgent has unverified source: {label}");
        }
        Ok(true)
    }

    fn mutate(&self, args: &[&str], lock: &File) -> Result<()> {
        if !self.run(args, Some(lock))?.status.success() {
            bail!(
                "launchctl {} failed; service state may be partially changed; run plan/apply again",
                args[0]
            );
        }
        Ok(())
    }
}

impl Plan {
    pub(super) fn capture(
        source: &Path,
        home: &Path,
        executable: &Path,
        lock: Option<&File>,
    ) -> Result<Self> {
        let configured = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute());
        let directory = configured
            .clone()
            .unwrap_or_else(|| home.join(".local/state"));
        let state = directory.join("dotfiles/user-services.json");
        Self::capture_with_anchor(
            source,
            home,
            &state,
            configured.as_deref().unwrap_or(home),
            Launchctl {
                executable: executable.to_owned(),
                uid: unsafe { libc::geteuid() },
            },
            lock,
        )
    }

    #[cfg(test)]
    fn capture_at(source: &Path, home: &Path, state: &Path) -> Result<Self> {
        Self::capture_with_anchor(
            source,
            home,
            state,
            home,
            Launchctl {
                executable: source.join("launchctl-fixture"),
                uid: 501,
            },
            None,
        )
    }

    fn capture_with_anchor(
        source: &Path,
        home: &Path,
        state: &Path,
        state_anchor: &Path,
        launchctl: Launchctl,
        lock: Option<&File>,
    ) -> Result<Self> {
        validate_path(home)?;
        validate_path(state)?;
        validate_path(state_anchor)?;
        let stable_source = source.canonicalize()?;
        let stable_state_anchor = resolve_anchor(state_anchor)?;
        let declaration = read_regular(&source.join("user-services.toml"), source)?;
        let declarations = declaration
            .as_deref()
            .map(|bytes| -> Result<Declarations> {
                let text =
                    std::str::from_utf8(bytes).context("invalid user-services.toml encoding")?;
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
        let state_bytes = read_regular(state, state_anchor)?;
        let previous = state_bytes
            .as_deref()
            .map(|bytes| -> Result<Applied> {
                let applied: Applied = serde_json::from_slice(bytes)
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
        let mut files = BTreeMap::new();
        let mut loaded = BTreeMap::new();
        // Resolve the trusted HOME alias once, so launchd records a stable path.
        let stable_home = home.canonicalize()?;
        for label in labels {
            let path = stable_home
                .join("Library/LaunchAgents")
                .join(format!("{label}.plist"));
            let current = read_regular(&path, &stable_home)?;
            match (previous.get(label), &current) {
                (None, Some(_)) => bail!("unmanaged LaunchAgent conflict: {label}"),
                (Some(hash), Some(bytes)) if digest(bytes) != *hash => {
                    bail!("managed LaunchAgent changed outside dotfiles: {label}")
                }
                _ => {}
            }
            let is_loaded = launchctl.observe(label, &path, lock)?;
            if is_loaded && !previous.contains_key(label) {
                bail!("unmanaged loaded LaunchAgent conflict: {label}");
            }
            loaded.insert(label.clone(), is_loaded);
            files.insert(label.clone(), current.clone());
            let operation = match (desired.get(label), previous.get(label), current) {
                (Some(_), None, _) => Some('+'),
                (Some(plist), Some(_), Some(bytes)) if plist.as_bytes() == bytes && is_loaded => {
                    None
                }
                (Some(_), Some(_), _) => Some('~'),
                (None, Some(_), _) => Some('-'),
                _ => None,
            };
            if let Some(operation) = operation {
                changes.insert(label.clone(), operation);
            }
        }
        Ok(Self {
            changes,
            desired,
            previous,
            files,
            loaded,
            declaration,
            source: source.to_owned(),
            home: home.to_owned(),
            state: state.to_owned(),
            state_anchor: state_anchor.to_owned(),
            stable_home,
            stable_source,
            stable_state_anchor,
            state_bytes,
            launchctl,
        })
    }

    pub(super) fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    pub(super) fn render(&self) -> String {
        if !self.has_changes() {
            return String::new();
        }
        let mut text = String::from("user services (launchd)");
        for (label, operation) in &self.changes {
            text.push_str(&format!("\n  {operation} {label}"));
        }
        text
    }

    fn path(&self, label: &str) -> Result<PathBuf> {
        Ok(self
            .stable_home
            .join("Library/LaunchAgents")
            .join(format!("{label}.plist")))
    }

    pub(super) fn verify(&self, lock: Option<&File>) -> Result<()> {
        if self.files.is_empty() {
            return Ok(());
        }
        if self.home.canonicalize()? != self.stable_home
            || self.source.canonicalize()? != self.stable_source
            || resolve_anchor(&self.state_anchor)? != self.stable_state_anchor
            || read_regular(&self.source.join("user-services.toml"), &self.source)?
                != self.declaration
            || read_regular(&self.state, &self.state_anchor)? != self.state_bytes
        {
            bail!(
                "user service inputs or result record changed after preview; run plan/apply again"
            );
        }
        for label in self.files.keys() {
            self.verify_resource(label, lock)?;
        }
        Ok(())
    }

    fn verify_resource(&self, label: &str, lock: Option<&File>) -> Result<()> {
        let path = self.path(label)?;
        if read_regular(&path, &self.stable_home)? != self.files[label]
            || self.launchctl.observe(label, &path, lock)? != self.loaded[label]
        {
            bail!("user service changed after preview: {label}; run plan/apply again");
        }
        Ok(())
    }

    pub(super) fn apply(&mut self, lock: &File) -> Result<()> {
        self.verify(Some(lock))?;
        for label in self.changes.clone().keys() {
            self.verify(Some(lock))?;
            let path = self.path(label)?;
            let anchor = self.stable_home.clone();
            self.verify_resource(label, Some(lock))?;
            if self.loaded[label] {
                // Capture already proved both historical file ownership and the
                // loaded job's source. Never boot out a merely matching label.
                if !self.previous.contains_key(label) {
                    bail!("cannot bootout unowned agent");
                }
                self.launchctl.mutate(
                    &["bootout", &format!("{}/{}", self.launchctl.domain(), label)],
                    lock,
                )?;
                if self.launchctl.observe(label, &path, Some(lock))? {
                    bail!("LaunchAgent remains loaded after bootout: {label}");
                }
                self.loaded.insert(label.clone(), false);
            }
            self.verify(Some(lock))?;
            if let Some(plist) = self.desired.get(label).cloned() {
                let old = self.files[label].clone();
                let old_mode = if old.is_some() {
                    fs::symlink_metadata(&path)?.permissions().mode() & 0o777
                } else {
                    0o644
                };
                atomic_write(&path, &anchor, old.as_deref(), plist.as_bytes(), 0o644)?;
                self.files
                    .insert(label.clone(), Some(plist.as_bytes().to_vec()));
                let mut next = self.previous.clone();
                next.insert(label.clone(), digest(plist.as_bytes()));
                if let Err(error) = self.checkpoint(next) {
                    // No bootstrap before the file result is recorded. A
                    // failed checkpoint may leave disk changed; restore only
                    // exact bytes written by us, never somebody else's edit.
                    if read_regular(&path, &anchor)?.as_deref() == Some(plist.as_bytes()) {
                        if let Some(old) = old {
                            atomic_write(&path, &anchor, Some(plist.as_bytes()), &old, old_mode)?;
                        } else {
                            fs::remove_file(&path)?;
                        }
                    }
                    return Err(error).context(
                        "service result checkpoint failed before bootstrap; run plan/apply again",
                    );
                }
                self.verify(Some(lock))?;
                self.verify_resource(label, Some(lock))?;
                self.launchctl.mutate(
                    &[
                        "bootstrap",
                        &self.launchctl.domain(),
                        path.to_str().context("invalid service path")?,
                    ],
                    lock,
                )?;
                if !self.launchctl.observe(label, &path, Some(lock))? {
                    bail!("LaunchAgent absent after bootstrap: {label}");
                }
                self.loaded.insert(label.clone(), true);
            } else {
                if self.files[label].is_some() {
                    fs::remove_file(&path)?;
                }
                self.files.insert(label.clone(), None);
                let mut next = self.previous.clone();
                next.remove(label);
                self.checkpoint(next).context(
                    "service removed but result checkpoint failed; run plan/apply again",
                )?;
            }
            self.verify(Some(lock))?;
        }
        self.changes.clear();
        Ok(())
    }

    fn checkpoint(&mut self, agents: BTreeMap<String, String>) -> Result<()> {
        let bytes = serde_json::to_vec(&Applied {
            version: 1,
            home: self.home.clone(),
            agents: agents.clone(),
        })?;
        atomic_write(
            &self.state,
            &self.state_anchor,
            self.state_bytes.as_deref(),
            &bytes,
            0o600,
        )?;
        self.state_bytes = Some(bytes);
        self.previous = agents;
        Ok(())
    }
}

// Resolve the trusted root's existing ancestors without requiring a fresh
// XDG_STATE_HOME to exist yet. Detect later alias retargeting before writes.
fn resolve_anchor(path: &Path) -> Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                bail!("dangling user service anchor");
            }
            Ok(
                resolve_anchor(path.parent().context("unresolvable state anchor")?)?
                    .join(path.file_name().context("invalid state anchor")?),
            )
        }
        Err(error) => Err(error.into()),
    }
}

// Atomic visibility, not a transaction across launchd and the two files. A
// process crash before checkpoint can leave an unowned file: fail closed then.
fn atomic_write(
    path: &Path,
    anchor: &Path,
    expected: Option<&[u8]>,
    bytes: &[u8],
    mode: u32,
) -> Result<()> {
    if read_regular(path, anchor)?.as_deref() != expected {
        bail!("user service file changed before write");
    }
    let parent = path.parent().context("service file has no parent")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(mode))?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if read_regular(path, anchor)?.as_deref() != expected {
        bail!("user service file changed during write");
    }
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
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

    fn fixture_launchctl(source: &Path, _home: &Path) -> Launchctl {
        let executable = source.join("launchctl-fixture");
        fs::create_dir_all(source.join("loaded")).unwrap();
        fs::write(
            &executable,
            include_str!("../../tests/fixtures/launchctl.sh"),
        )
        .unwrap();
        fs::write(source.join("uid"), "501").unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        Launchctl {
            executable,
            uid: 501,
        }
    }

    const DECLARATION: &str = "[[agents]]\nlabel = 'com.example.check'\nargv = ['~/.local/share/mise/shims/node', 'hello<&\".js']\nrun_at_load = true\nstart_interval = 60\n";

    struct Fixture {
        apply_lock: std::cell::OnceCell<File>,
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
            fixture_launchctl(&source, &home);
            Self {
                apply_lock: std::cell::OnceCell::new(),
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
            fs::write(
                self.source.join("loaded/com.example.check"),
                self.plist("com.example.check").to_str().unwrap(),
            )
            .unwrap();
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
        assert!(!Plan::capture_with_anchor(
            &f.source,
            &f.home,
            &state,
            &alias,
            fixture_launchctl(&f.source, &f.home),
            None
        )
        .unwrap()
        .has_changes());
        f.declare(&format!(
            "{DECLARATION}{}",
            DECLARATION.replace("com.example.check", "com.example.Check")
        ));
        assert!(f.plan().is_err());
        f.applied();
        f.declare(&DECLARATION.replace("com.example.check", "com.example.Check"));
        assert!(f.plan().is_err());
    }

    fn apply(f: &Fixture) -> Result<()> {
        // Lifecycle tests exercise one backend under one shared lock. Keeping
        // its guard avoids racing unrelated test forks which can briefly hold
        // a copied descriptor before exec closes it. Dedicated tests below
        // still prove lock inheritance and release across process boundaries.
        let lock = f.apply_lock.get_or_init(|| {
            super::super::acquire_lock(&f._root.path().join("apply.lock")).unwrap()
        });
        f.plan()?.apply(lock)
    }

    fn mutations(f: &Fixture) -> Vec<String> {
        fs::read_to_string(f.source.join("calls"))
            .unwrap_or_default()
            .lines()
            .filter(|line| line.starts_with("bootstrap ") || line.starts_with("bootout "))
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn executable_boundary_add_reload_delete_noop_and_missing_job() {
        let f = Fixture::new();
        f.declare(DECLARATION);
        apply(&f).unwrap();
        assert_eq!(
            mutations(&f),
            [format!(
                "bootstrap gui/501 {}",
                f.plist("com.example.check").display()
            )]
        );
        let before = fs::read(&f.state).unwrap();
        apply(&f).unwrap();
        assert_eq!(mutations(&f).len(), 1);
        assert_eq!(fs::read(&f.state).unwrap(), before);
        fs::remove_file(f.source.join("loaded/com.example.check")).unwrap();
        assert!(f.plan().unwrap().has_changes());
        apply(&f).unwrap();
        assert_eq!(mutations(&f).len(), 2); // Missing job is bootstrapped, never booted out.
        f.declare(&DECLARATION.replace("60", "120"));
        apply(&f).unwrap();
        assert_eq!(mutations(&f)[2], "bootout gui/501/com.example.check");
        assert_eq!(mutations(&f).len(), 4);
        assert!(fs::read_to_string(f.plist("com.example.check"))
            .unwrap()
            .contains("120"));
        f.declare("");
        apply(&f).unwrap();
        assert_eq!(mutations(&f)[4], "bootout gui/501/com.example.check");
        assert!(!f.plist("com.example.check").exists());
        assert!(!f.plan().unwrap().has_changes());
        apply(&f).unwrap();
        assert_eq!(mutations(&f).len(), 5);
    }

    #[test]
    fn failed_and_partial_bootstrap_leave_recorded_file_and_retry_safely() {
        for failure in ["fail-bootstrap", "partial-bootstrap", "vanish-bootstrap"] {
            let f = Fixture::new();
            f.declare(DECLARATION);
            fs::write(f.source.join(failure), "").unwrap();
            assert!(apply(&f).is_err());
            let record: Applied = serde_json::from_slice(&fs::read(&f.state).unwrap()).unwrap();
            assert_eq!(
                record.agents["com.example.check"],
                digest(&fs::read(f.plist("com.example.check")).unwrap())
            );
            fs::remove_file(f.source.join(failure)).unwrap();
            apply(&f).unwrap();
            assert!(!f.plan().unwrap().has_changes());
            assert!(mutations(&f)
                .iter()
                .all(|line| !line.starts_with("bootout")));
        }
    }

    #[test]
    fn failed_and_partial_bootout_preserve_previous_file_and_retry_safely() {
        for failure in ["fail-bootout", "partial-bootout"] {
            let f = Fixture::new();
            f.declare(DECLARATION);
            f.applied();
            let bytes = fs::read(f.plist("com.example.check")).unwrap();
            let state = fs::read(&f.state).unwrap();
            f.declare(&DECLARATION.replace("60", "120"));
            fs::write(f.source.join(failure), "").unwrap();
            assert!(apply(&f).is_err());
            assert_eq!(fs::read(f.plist("com.example.check")).unwrap(), bytes);
            assert_eq!(fs::read(&f.state).unwrap(), state);
            assert_eq!(mutations(&f), ["bootout gui/501/com.example.check"]);
            fs::remove_file(f.source.join(failure)).unwrap();
            apply(&f).unwrap();
            assert!(!f.plan().unwrap().has_changes());
        }
    }

    #[test]
    fn partial_batch_keeps_successes_and_retry_does_not_reload_them() {
        let f = Fixture::new();
        f.declare(&format!(
            "{DECLARATION}{}",
            DECLARATION.replace("com.example.check", "com.example.z")
        ));
        fs::write(f.source.join("fail-bootstrap-com.example.z"), "").unwrap();
        assert!(apply(&f).is_err());
        assert!(f.source.join("loaded/com.example.check").exists());
        assert!(!f.source.join("loaded/com.example.z").exists());
        fs::remove_file(f.source.join("fail-bootstrap-com.example.z")).unwrap();
        apply(&f).unwrap();
        assert_eq!(mutations(&f).len(), 3);
        assert!(mutations(&f)[2].ends_with("com.example.z.plist"));
        assert!(!f.plan().unwrap().has_changes());
    }

    #[test]
    fn ownership_and_observation_errors_never_authorize_mutation() {
        for failure in ["print-error", "domain-error", "unknown-print"] {
            let f = Fixture::new();
            f.declare(DECLARATION);
            fs::write(f.source.join(failure), "").unwrap();
            assert!(f.plan().is_err(), "accepted {failure}");
            assert!(mutations(&f).is_empty());
            assert!(!f.state.exists());
        }
        let f = Fixture::new();
        f.declare(DECLARATION);
        fs::write(
            f.source.join("loaded/com.example.check"),
            f.plist("com.example.check").to_str().unwrap(),
        )
        .unwrap();
        assert!(f.plan().is_err()); // Same path without ledger never grants ownership.
        f.applied();
        fs::write(
            f.source.join("loaded/com.example.check"),
            "/Library/LaunchAgents/private.plist",
        )
        .unwrap();
        assert!(f.plan().is_err()); // Same label, foreign source.
        assert!(mutations(&f).is_empty());
    }

    #[test]
    fn apply_revalidates_declaration_ledger_file_loaded_state_and_home_alias() {
        for changed in ["declaration", "ledger", "file", "loaded", "source"] {
            let f = Fixture::new();
            f.declare(DECLARATION);
            f.applied();
            f.declare(&DECLARATION.replace("60", "120"));
            let mut plan = f.plan().unwrap();
            match changed {
                "declaration" => f.declare(""),
                "ledger" => fs::write(&f.state, b"{}").unwrap(),
                "file" => fs::write(f.plist("com.example.check"), b"external").unwrap(),
                "loaded" => fs::remove_file(f.source.join("loaded/com.example.check")).unwrap(),
                "source" => fs::write(f.source.join("loaded/com.example.check"), "/other").unwrap(),
                _ => unreachable!(),
            }
            let lock = super::super::acquire_lock(&f._root.path().join("apply.lock")).unwrap();
            assert!(plan.apply(&lock).is_err());
            assert!(mutations(&f).is_empty());
        }
        let f = Fixture::new();
        f.declare(DECLARATION);
        let alias = f._root.path().join("alias");
        symlink(&f.home, &alias).unwrap();
        let mut plan = Plan::capture_at(&f.source, &alias, &f.state).unwrap();
        fs::remove_file(&alias).unwrap();
        symlink(&f.source, &alias).unwrap();
        let lock = super::super::acquire_lock(&f._root.path().join("apply.lock")).unwrap();
        assert!(plan.apply(&lock).is_err());
        assert!(mutations(&f).is_empty());
    }

    #[test]
    fn state_write_failure_rolls_back_only_file_and_does_not_bootstrap() {
        // This fixture exercises a real OS write failure, not a backend mock.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        for existing in [false, true] {
            let f = Fixture::new();
            f.declare(DECLARATION);
            if existing {
                f.applied();
                fs::set_permissions(
                    f.plist("com.example.check"),
                    fs::Permissions::from_mode(0o600),
                )
                .unwrap();
            }
            let old = fs::read(f.plist("com.example.check")).ok();
            let state = fs::read(&f.state).ok();
            f.declare(&DECLARATION.replace("60", "120"));
            fs::create_dir_all(f.state.parent().unwrap()).unwrap();
            fs::set_permissions(f.state.parent().unwrap(), fs::Permissions::from_mode(0o500))
                .unwrap();
            let result = apply(&f);
            fs::set_permissions(f.state.parent().unwrap(), fs::Permissions::from_mode(0o700))
                .unwrap();
            assert!(result.is_err());
            assert_eq!(fs::read(f.plist("com.example.check")).ok(), old);
            assert_eq!(fs::read(&f.state).ok(), state);
            if existing {
                assert_eq!(
                    fs::metadata(f.plist("com.example.check"))
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o600
                );
            }
            assert!(mutations(&f)
                .iter()
                .all(|line| !line.starts_with("bootstrap")));
            apply(&f).unwrap();
            assert!(!f.plan().unwrap().has_changes());
        }
    }

    #[test]
    fn delete_checkpoint_failure_retains_safe_retry_ownership() {
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let f = Fixture::new();
        f.applied();
        let state = fs::read(&f.state).unwrap();
        fs::set_permissions(f.state.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
        let result = apply(&f);
        fs::set_permissions(f.state.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
        assert!(result.is_err());
        assert!(!f.plist("com.example.check").exists());
        assert_eq!(fs::read(&f.state).unwrap(), state);
        apply(&f).unwrap();
        assert!(!f.plan().unwrap().has_changes());
        assert_eq!(mutations(&f), ["bootout gui/501/com.example.check"]);
    }

    #[test]
    fn successful_apply_preserves_unmanaged_private_system_resources() {
        let f = Fixture::new();
        f.declare(DECLARATION);
        fs::write(f.plist("private.agent"), b"private").unwrap();
        fs::write(
            f.source.join("loaded/private.agent"),
            b"/private/agent.plist",
        )
        .unwrap();
        fs::create_dir_all(f.home.join("Library/LaunchDaemons")).unwrap();
        let daemon = f.home.join("Library/LaunchDaemons/system.plist");
        fs::write(&daemon, b"system").unwrap();
        apply(&f).unwrap();
        f.declare("");
        apply(&f).unwrap();
        assert_eq!(fs::read(f.plist("private.agent")).unwrap(), b"private");
        assert_eq!(fs::read(&daemon).unwrap(), b"system");
        assert_eq!(
            fs::read(f.source.join("loaded/private.agent")).unwrap(),
            b"/private/agent.plist"
        );
        assert!(!fs::read_to_string(f.source.join("calls"))
            .unwrap()
            .contains("private.agent"));
    }

    #[test]
    fn diagnostic_parser_rejects_ambiguous_status_output_and_identity() {
        let f = Fixture::new();
        let runner = fixture_launchctl(&f.source, &f.home);
        let path = f.plist("com.example.check");
        let target = "gui/501/com.example.check";
        let valid = format!(
            "{target} = {{\n\tpath = {}\n\ttype = LaunchAgent\n}}\n",
            path.display()
        );
        let missing = "Bad request.\nCould not find service \"com.example.check\" in domain for user gui: 501\n";
        let responses = [
            (5, "".to_owned(), missing.to_owned()),
            (113, "".to_owned(), "permission denied".to_owned()),
            (113, "unexpected".to_owned(), missing.to_owned()),
            (
                113,
                "".to_owned(),
                missing.replace("com.example.check", "other"),
            ),
            (0, valid.replace("gui/501", "system"), "".to_owned()),
            (
                0,
                valid.replace("LaunchAgent", "LaunchDaemon"),
                "".to_owned(),
            ),
            (0, valid.replace("\tpath", "\t\tpath"), "".to_owned()),
            (
                0,
                valid.replace("\ttype", &format!("\tpath = {}\n\ttype", path.display())),
                "".to_owned(),
            ),
            (0, valid.trim_end_matches("}\n").to_owned(), "".to_owned()),
            (0, format!("{valid}trailing"), "".to_owned()),
            (0, valid.clone(), "warning".to_owned()),
        ];
        for (code, stdout, stderr) in responses {
            fs::write(f.source.join("print-code"), code.to_string()).unwrap();
            fs::write(f.source.join("print-stdout"), stdout).unwrap();
            fs::write(f.source.join("print-stderr"), stderr).unwrap();
            assert!(runner.observe("com.example.check", &path, None).is_err());
        }
        fs::write(f.source.join("print-code"), "0").unwrap();
        fs::write(
            f.source.join("print-stdout"),
            valid.replace("\ttype", "\tpid = 42\n\truns = 999\n\ttype"),
        )
        .unwrap();
        fs::write(f.source.join("print-stderr"), "").unwrap();
        assert!(runner.observe("com.example.check", &path, None).unwrap());
        assert!(mutations(&f).is_empty());
    }

    #[test]
    fn explicit_state_anchor_retargeting_is_detected_even_with_identical_bytes() {
        let f = Fixture::new();
        f.declare(DECLARATION);
        let first = f._root.path().join("first");
        let second = f._root.path().join("second");
        fs::create_dir(&first).unwrap();
        fs::create_dir(&second).unwrap();
        let alias = f._root.path().join("state-alias");
        symlink(&first, &alias).unwrap();
        let mut plan = Plan::capture_with_anchor(
            &f.source,
            &f.home,
            &alias.join("dotfiles/user-services.json"),
            &alias,
            fixture_launchctl(&f.source, &f.home),
            None,
        )
        .unwrap();
        fs::remove_file(&alias).unwrap();
        symlink(&second, &alias).unwrap();
        let lock = super::super::acquire_lock(&f._root.path().join("apply.lock")).unwrap();
        assert!(plan.apply(&lock).is_err());
        assert!(mutations(&f).is_empty());
        assert!(!f.plist("com.example.check").exists());
    }

    #[test]
    fn inherited_apply_lock_survives_parent_handle() {
        use fs2::FileExt;
        let f = Fixture::new();
        let path = f._root.path().join("apply.lock");
        let lock = super::super::acquire_lock(&path).unwrap();
        let runner = fixture_launchctl(&f.source, &f.home);
        assert!(runner
            .run(&["lock-probe"], Some(&lock))
            .unwrap()
            .status
            .success());
        drop(lock);
        let contender = File::open(&path).unwrap();
        assert!(contender.try_lock_exclusive().is_err());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while !f.source.join("child-finished").exists() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        // The child may still be closing its descriptors after touch.
        while contender.try_lock_exclusive().is_err() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn pending_services_share_unified_confirmation() {
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
            || Ok(()),
        );
        assert!(matches!(result, Ok(super::super::Review::Apply)));
        assert!(!f.state.exists());
    }
}
