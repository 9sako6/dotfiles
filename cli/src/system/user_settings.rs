//! Public per-user settings. No shell hooks, user-supplied commands, or sudo.
//!
//! The pinned nightlight 1.0.0 CLI has read-only `help`, `schedule`, and `temp`
//! commands. Its schedule setter preserves the current on/off state. NSLocale
//! decides its 12/24-hour output even with LC_ALL=C, so normalize both formats.
//! See https://github.com/smudge/nightlight/tree/v1.0.0/src
use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};

const DECLARATION: &str = "user-settings.toml";
const VERSION: &str = "nightlight v1.0.0";

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Declarations {
    night_shift: Option<NightShift>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NightShift {
    end: String,
    start: String,
    temperature: u8,
}

#[derive(Clone, Debug, PartialEq)]
enum Schedule {
    Off,
    Sunset,
    Custom(u16, u16),
}

impl std::fmt::Display for Schedule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Off => write!(f, "off"),
            Self::Sunset => write!(f, "sunset to sunrise"),
            Self::Custom(start, end) => write!(f, "{} to {}", time_text(*start), time_text(*end)),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Observed {
    schedule: Schedule,
    temperature: u8,
}

#[derive(Debug, PartialEq)]
struct HelperIdentity {
    path: PathBuf,
    hash: Vec<u8>,
    mode: u32,
}

pub(super) struct Plan {
    source: PathBuf,
    stable_source: PathBuf,
    declaration: Option<Vec<u8>>,
    home: PathBuf,
    stable_home: PathBuf,
    helper: PathBuf,
    identity: Option<HelperIdentity>,
    desired: Option<Observed>,
    observed: Option<Observed>,
}

impl Plan {
    /// `helper` is the coordinator's fixed artifact path, never a declaration
    /// field or a PATH lookup. A missing artifact is explicitly pending: plan
    /// does not realize it or pretend that current preferences are known.
    pub(super) fn capture(
        source: &Path,
        home: &Path,
        helper: &Path,
        lock: Option<&File>,
    ) -> Result<Self> {
        if !helper.is_absolute() {
            bail!("Night Shift requires an absolute pinned helper path");
        }
        let declaration = read_declaration(source)?;
        let desired = parse_declarations(declaration.as_deref())?;
        let mut plan = Self {
            source: source.to_owned(),
            stable_source: source.canonicalize()?,
            declaration,
            home: home.to_owned(),
            stable_home: home.canonicalize()?,
            helper: helper.to_owned(),
            identity: None,
            desired,
            observed: None,
        };
        if plan.desired.is_some() {
            plan.identity = helper_identity(helper)?;
            if plan.identity.is_some() {
                plan.check_version(lock)?;
                plan.observed = Some(plan.observe(lock)?);
                plan.verify_inputs()?;
                plan.verify_helper()?;
            }
        }
        Ok(plan)
    }

    pub(super) fn has_changes(&self) -> bool {
        self.desired.is_some() && self.desired != self.observed
    }

    pub(super) fn render(&self) -> String {
        let Some(desired) = &self.desired else {
            return String::new();
        };
        if !self.has_changes() {
            return String::new();
        }
        let mut text = String::from("user settings (Night Shift)");
        if let Some(observed) = &self.observed {
            if observed.schedule != desired.schedule {
                text.push_str(&format!(
                    "\n  ~ schedule: {} -> {}",
                    observed.schedule, desired.schedule
                ));
            }
            if observed.temperature != desired.temperature {
                text.push_str(&format!(
                    "\n  ~ temperature: {} -> {}",
                    observed.temperature, desired.temperature
                ));
            }
        } else {
            text.push_str(&format!(
                "\n  ? schedule: unknown -> {}\n  ? temperature: unknown -> {}\n  helper pending: nightlight 1.0.0 (read after approved artifact installation)",
                desired.schedule, desired.temperature
            ));
        }
        text
    }

    fn command(&self, args: &[&str], lock: Option<&File>) -> Command {
        let mut command = Command::new(&self.helper);
        command
            .args(args)
            .env("HOME", &self.home)
            .env("LC_ALL", "C")
            .env("LANG", "C");
        if let Some(lock) = lock {
            super::retain_apply_lock(&mut command, lock);
        }
        command
    }

    fn read(&self, arg: &str, lock: Option<&File>) -> Result<String> {
        let output = self
            .command(&[arg], lock)
            .output()
            .context("cannot run pinned nightlight helper")?;
        if !output.status.success() || !output.stderr.is_empty() {
            bail!("nightlight {arg} read failed; current Night Shift settings are unknown");
        }
        String::from_utf8(output.stdout).context("unrecognized nightlight output encoding")
    }

    fn check_version(&self, lock: Option<&File>) -> Result<()> {
        if self.read("help", lock)?.lines().next() != Some(VERSION) {
            bail!("unsupported nightlight helper; expected version 1.0.0");
        }
        Ok(())
    }

    fn observe(&self, lock: Option<&File>) -> Result<Observed> {
        let schedule = parse_schedule(single_line(&self.read("schedule", lock)?)?)?;
        let temperature = self.read("temp", lock)?;
        let temperature = single_line(&temperature)?;
        if temperature.is_empty() || !temperature.bytes().all(|byte| byte.is_ascii_digit()) {
            bail!("unrecognized nightlight temperature");
        }
        let temperature = temperature
            .parse::<u8>()
            .ok()
            .filter(|value| *value <= 100)
            .context("unrecognized nightlight temperature")?;
        Ok(Observed {
            schedule,
            temperature,
        })
    }

    fn verify_inputs(&self) -> Result<()> {
        if self.source.canonicalize()? != self.stable_source
            || self.home.canonicalize()? != self.stable_home
            || read_declaration(&self.source)? != self.declaration
        {
            bail!("user settings inputs changed after preview; run plan/apply again");
        }
        Ok(())
    }

    fn verify_helper(&self) -> Result<()> {
        if self.identity.is_some() && helper_identity(&self.helper)? != self.identity {
            bail!("nightlight helper changed after preview; run plan/apply again");
        }
        Ok(())
    }

    pub(super) fn verify(&self, lock: Option<&File>) -> Result<()> {
        self.verify_inputs()?;
        if let Some(observed) = &self.observed {
            self.verify_helper()?;
            if self.observe(lock)? != *observed {
                bail!("Night Shift changed after preview; run plan/apply again");
            }
            self.verify_helper()?;
        }
        Ok(())
    }

    /// Call only after review and artifact realization, with the shared user
    /// apply lock. A pending helper may now exist; read before deciding writes.
    pub(super) fn apply(&mut self, lock: &File) -> Result<()> {
        self.verify(Some(lock))?;
        let Some(desired) = self.desired.clone() else {
            return Ok(());
        };
        if self.observed.is_none() {
            self.identity = Some(helper_identity(&self.helper)?.context(
                "pinned nightlight helper is not installed; realize the approved artifact first",
            )?);
            self.check_version(Some(lock))?;
            self.observed = Some(self.observe(Some(lock))?);
            self.verify(Some(lock))?;
        }
        let mut expected = self
            .observed
            .clone()
            .context("Night Shift readback missing")?;
        if expected.schedule != desired.schedule {
            let Schedule::Custom(start, end) = desired.schedule else {
                bail!("invalid desired Night Shift schedule");
            };
            self.verify(Some(lock))?;
            self.write(&["schedule", &time_text(start), &time_text(end)], lock)?;
            expected.schedule = desired.schedule.clone();
            self.accept_readback(&expected, lock)?;
        }
        if expected.temperature != desired.temperature {
            self.verify(Some(lock))?;
            self.write(&["temp", &desired.temperature.to_string()], lock)?;
            expected.temperature = desired.temperature;
            self.accept_readback(&expected, lock)?;
        }
        self.verify(Some(lock))?;
        if self.observed.as_ref() != Some(&desired) {
            bail!("Night Shift settings did not converge; run plan/apply again");
        }
        Ok(())
    }

    fn write(&self, args: &[&str], lock: &File) -> Result<()> {
        let output = self
            .command(args, Some(lock))
            .output()
            .context("cannot run pinned nightlight helper")?;
        if !output.status.success() || !output.stdout.is_empty() || !output.stderr.is_empty() {
            bail!(
                "nightlight write failed; settings may be partially changed; run plan/apply again"
            );
        }
        Ok(())
    }

    fn accept_readback(&mut self, expected: &Observed, lock: &File) -> Result<()> {
        self.verify_inputs()?;
        self.verify_helper()?;
        if self.observe(Some(lock))? != *expected {
            bail!("Night Shift settings did not converge; settings may be partially changed; run plan/apply again");
        }
        self.observed = Some(expected.clone());
        Ok(())
    }
}

/// Read only declared preferences, using the reconciliation parser. No helper
/// executable is launched and no current preference value is inferred.
pub(super) fn declared_inventory(source: &Path) -> Result<Vec<serde_json::Value>> {
    let Some(desired) = parse_declarations(read_declaration(source)?.as_deref())? else {
        return Ok(Vec::new());
    };
    let Schedule::Custom(start, end) = desired.schedule else {
        bail!("unsupported declared Night Shift schedule");
    };
    Ok(vec![
        serde_json::json!({"name":"Night Shift", "manager":"nightlight",
        "start":time_text(start), "end":time_text(end), "temperature":desired.temperature}),
    ])
}

fn read_declaration(source: &Path) -> Result<Option<Vec<u8>>> {
    let path = source.join(DECLARATION);
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => Ok(Some(fs::read(&path)?)),
        Ok(_) => bail!("user-settings.toml must be a regular file, not a symlink"),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("cannot read user-settings.toml"),
    }
}

fn parse_declarations(bytes: Option<&[u8]>) -> Result<Option<Observed>> {
    let Some(bytes) = bytes else { return Ok(None) };
    let declarations: Declarations =
        toml::from_str(std::str::from_utf8(bytes).context("invalid user-settings.toml encoding")?)
            .map_err(|_| anyhow::anyhow!("invalid user-settings.toml schema"))?;
    declarations
        .night_shift
        .map(|night_shift| {
            if night_shift.temperature > 100 {
                bail!("Night Shift temperature must be between 0 and 100");
            }
            Ok(Observed {
                schedule: Schedule::Custom(
                    declared_time(&night_shift.start)?,
                    declared_time(&night_shift.end)?,
                ),
                temperature: night_shift.temperature,
            })
        })
        .transpose()
}

fn declared_time(text: &str) -> Result<u16> {
    if text.len() != 5 || text.as_bytes()[2] != b':' {
        bail!("Night Shift times must use HH:MM (00:00 to 23:59)");
    }
    parse_time(text).context("Night Shift times must use HH:MM (00:00 to 23:59)")
}

fn helper_identity(helper: &Path) -> Result<Option<HelperIdentity>> {
    let path = match helper.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot resolve pinned nightlight helper"),
    };
    let metadata = fs::metadata(&path)?;
    if !metadata.is_file() || metadata.permissions().mode() & 0o111 == 0 {
        bail!("pinned nightlight helper is not executable");
    }
    Ok(Some(HelperIdentity {
        hash: Sha256::digest(fs::read(&path)?).to_vec(),
        mode: metadata.permissions().mode() & 0o7777,
        path,
    }))
}

fn single_line(text: &str) -> Result<&str> {
    let text = text.strip_suffix('\n').unwrap_or(text);
    if text.is_empty() || text.chars().any(char::is_control) {
        bail!("unrecognized nightlight output");
    }
    Ok(text)
}

fn parse_schedule(text: &str) -> Result<Schedule> {
    match text {
        "off" => Ok(Schedule::Off),
        "sunset to sunrise" => Ok(Schedule::Sunset),
        text => {
            let (start, end) = text
                .split_once(" to ")
                .context("unrecognized nightlight schedule")?;
            Ok(Schedule::Custom(parse_time(start)?, parse_time(end)?))
        }
    }
}

fn parse_time(text: &str) -> Result<u16> {
    let (text, afternoon) = if let Some(text) = text.strip_suffix("AM") {
        (text, Some(false))
    } else if let Some(text) = text.strip_suffix("PM") {
        (text, Some(true))
    } else {
        (text, None)
    };
    let (hour, minute) = text
        .split_once(':')
        .context("unrecognized nightlight time")?;
    if !(1..=2).contains(&hour.len())
        || minute.len() != 2
        || !hour
            .bytes()
            .chain(minute.bytes())
            .all(|byte| byte.is_ascii_digit())
    {
        bail!("unrecognized nightlight time");
    }
    let hour: u16 = hour.parse()?;
    let minute: u16 = minute.parse()?;
    if minute > 59 || afternoon.is_none() && hour > 23 {
        bail!("unrecognized nightlight time");
    }
    let hour = if let Some(afternoon) = afternoon {
        if !(1..=12).contains(&hour) {
            bail!("unrecognized nightlight time");
        }
        hour % 12 + if afternoon { 12 } else { 0 }
    } else {
        hour
    };
    Ok(hour * 60 + minute)
}

fn time_text(minutes: u16) -> String {
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    const CONFIG: &str = "[night_shift]\nend = '07:00'\nstart = '22:00'\ntemperature = 80\n";

    struct Fixture {
        root: tempfile::TempDir,
        source: PathBuf,
        home: PathBuf,
        helper: PathBuf,
        lock: File,
    }

    impl Fixture {
        fn new(schedule: &str, temperature: &str) -> Self {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("source");
            let home = root.path().join("home");
            let helper = root.path().join("nightlight");
            fs::create_dir(&source).unwrap();
            fs::create_dir(&home).unwrap();
            fs::write(source.join(DECLARATION), CONFIG).unwrap();
            let lock = File::create(root.path().join("apply.lock")).unwrap();
            let fixture = Self {
                root,
                source,
                home,
                helper,
                lock,
            };
            fixture.put("schedule", schedule);
            fixture.put("temp", temperature);
            fixture.put("version", "nightlight v1.0.0\n  Usage fixture\n");
            fixture.put("on-off", "on\n");
            fixture.install_helper();
            fixture
        }

        fn put(&self, name: &str, value: &str) {
            fs::write(self.home.join(name), value).unwrap();
        }

        fn install_helper(&self) {
            // A command fixture implementing the documented v1.0.0 contract.
            // It fails closed on unexpected calls (including no args/toggle).
            fs::write(
                &self.helper,
                r#"#!/bin/sh
set -eu
[ "$LC_ALL" = C ]
[ "$LANG" = C ]
[ -d "$HOME" ]
if [ -f "$HOME/read-error" ] && [ "$#" = 1 ]; then exit 9; fi
if [ -f "$HOME/read-warning" ] && [ "$#" = 1 ]; then echo warning >&2; fi
case "$#:$1" in
  1:help) cat "$HOME/version" ;;
  1:schedule) cat "$HOME/schedule" ;;
  1:temp) cat "$HOME/temp" ;;
  3:schedule)
    printf 'schedule %s %s\n' "$2" "$3" >> "$HOME/writes"
    printf '%s to %s\n' "$2" "$3" > "$HOME/schedule"
    ;;
  2:temp)
    printf 'temp %s\n' "$2" >> "$HOME/writes"
    [ ! -f "$HOME/fail-temp" ] || exit 8
    [ ! -f "$HOME/ignore-temp" ] || exit 0
    printf '%s\n' "$2" > "$HOME/temp"
    ;;
  *) exit 77 ;;
esac
"#,
            )
            .unwrap();
            fs::set_permissions(&self.helper, fs::Permissions::from_mode(0o755)).unwrap();
        }

        fn capture(&self) -> Result<Plan> {
            Plan::capture(&self.source, &self.home, &self.helper, Some(&self.lock))
        }

        fn writes(&self) -> String {
            fs::read_to_string(self.home.join("writes")).unwrap_or_default()
        }
    }

    #[test]
    fn preserves_existing_values_and_normalizes_both_nslocale_formats_without_writes() {
        for schedule in ["22:00 to 7:00\n", "10:00PM to 7:00AM\n"] {
            let fixture = Fixture::new(schedule, "80\n");
            let mut plan = fixture.capture().unwrap();
            assert!(!plan.has_changes());
            assert!(plan.render().is_empty());
            plan.verify(Some(&fixture.lock)).unwrap();
            plan.apply(&fixture.lock).unwrap();
            assert!(fixture.writes().is_empty());
            assert_eq!(
                fs::read_to_string(fixture.home.join("on-off")).unwrap(),
                "on\n"
            );
        }
        assert_eq!(parse_time("12:00AM").unwrap(), 0);
        assert_eq!(parse_time("12:00PM").unwrap(), 720);
    }

    #[test]
    fn plan_is_read_only_then_apply_changes_only_differences_and_converges() {
        let fixture = Fixture::new("sunset to sunrise\n", "40\n");
        let mut plan = fixture.capture().unwrap();
        assert!(plan.has_changes());
        assert!(plan
            .render()
            .contains("sunset to sunrise -> 22:00 to 07:00"));
        assert!(fixture.writes().is_empty());
        plan.apply(&fixture.lock).unwrap();
        assert_eq!(fixture.writes(), "schedule 22:00 07:00\ntemp 80\n");
        assert!(!plan.has_changes());
        assert!(!fixture.capture().unwrap().has_changes());
        plan.apply(&fixture.lock).unwrap();
        assert_eq!(fixture.writes(), "schedule 22:00 07:00\ntemp 80\n");
        assert_eq!(
            fs::read_to_string(fixture.home.join("on-off")).unwrap(),
            "on\n"
        );
    }

    #[test]
    fn missing_artifact_is_explicitly_pending_and_read_only_until_apply() {
        let fixture = Fixture::new("22:00 to 07:00\n", "80\n");
        fs::remove_file(&fixture.helper).unwrap();
        let mut plan = fixture.capture().unwrap();
        assert!(plan.has_changes());
        assert!(plan.render().contains("unknown -> 80"));
        assert!(plan.render().contains("helper pending"));
        assert!(plan
            .apply(&fixture.lock)
            .unwrap_err()
            .to_string()
            .contains("not installed"));
        assert!(fixture.writes().is_empty());
        fixture.install_helper();
        plan.apply(&fixture.lock).unwrap();
        assert!(!plan.has_changes());
        assert!(fixture.writes().is_empty());
    }

    #[test]
    fn pending_helper_reads_actual_settings_after_installation_before_writing() {
        let fixture = Fixture::new("off\n", "20\n");
        fs::remove_file(&fixture.helper).unwrap();
        let mut plan = fixture.capture().unwrap();
        fixture.install_helper();
        // A user or the previous installation has already set temperature.
        fixture.put("temp", "80\n");
        plan.apply(&fixture.lock).unwrap();
        assert_eq!(fixture.writes(), "schedule 22:00 07:00\n");
    }

    #[test]
    fn stale_preferences_declarations_or_helpers_are_not_overwritten() {
        for kind in ["settings", "declaration", "helper", "home"] {
            let fixture = Fixture::new("off\n", "80\n");
            let mut plan = fixture.capture().unwrap();
            match kind {
                "settings" => fixture.put("schedule", "sunset to sunrise\n"),
                "declaration" => fs::write(fixture.source.join(DECLARATION), "").unwrap(),
                "helper" => {
                    let mut bytes = fs::read(&fixture.helper).unwrap();
                    bytes.extend_from_slice(b"\n# changed\n");
                    fs::write(&fixture.helper, bytes).unwrap();
                }
                "home" => {
                    fs::rename(&fixture.home, fixture.root.path().join("old-home")).unwrap();
                    fs::create_dir(&fixture.home).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(plan.apply(&fixture.lock).is_err(), "{kind}");
            assert!(fixture.writes().is_empty());
        }
    }

    #[test]
    fn failed_write_is_reported_and_a_fresh_plan_recovers_only_unfinished_changes() {
        let fixture = Fixture::new("off\n", "40\n");
        fixture.put("fail-temp", "");
        let mut plan = fixture.capture().unwrap();
        assert!(plan
            .apply(&fixture.lock)
            .unwrap_err()
            .to_string()
            .contains("partially changed"));
        assert_eq!(fixture.writes(), "schedule 22:00 07:00\ntemp 80\n");
        fs::remove_file(fixture.home.join("fail-temp")).unwrap();
        let mut recovery = fixture.capture().unwrap();
        assert!(!recovery.render().contains("~ schedule"));
        recovery.apply(&fixture.lock).unwrap();
        assert_eq!(fixture.writes(), "schedule 22:00 07:00\ntemp 80\ntemp 80\n");
        assert!(!fixture.capture().unwrap().has_changes());
    }

    #[test]
    fn successful_exit_without_readback_convergence_is_not_success() {
        let fixture = Fixture::new("22:00 to 7:00\n", "40\n");
        fixture.put("ignore-temp", "");
        let mut plan = fixture.capture().unwrap();
        assert!(plan
            .apply(&fixture.lock)
            .unwrap_err()
            .to_string()
            .contains("did not converge"));
        assert!(fixture.capture().unwrap().has_changes());
    }

    #[test]
    fn absent_or_empty_declaration_does_not_read_or_reset_preferences() {
        for remove in [false, true] {
            let fixture = Fixture::new("not supported\n", "bad\n");
            fs::remove_file(&fixture.helper).unwrap();
            if remove {
                fs::remove_file(fixture.source.join(DECLARATION)).unwrap();
            } else {
                fs::write(fixture.source.join(DECLARATION), "").unwrap();
            }
            let mut plan = fixture.capture().unwrap();
            assert!(!plan.has_changes());
            plan.apply(&fixture.lock).unwrap();
            assert!(fixture.writes().is_empty());
        }
    }

    #[test]
    fn invalid_schema_time_temperature_and_symlink_declarations_fail_closed() {
        for config in [
            "hook = 'echo unsafe'",
            "[night_shift]\nstart = '22:00'\nend = '07:00'\ntemperature = 101",
            "[night_shift]\nstart = '24:00'\nend = '07:00'\ntemperature = 80",
            "[night_shift]\nstart = '22:00'\nend = '7:00'\ntemperature = 80",
            "[night_shift]\nstart = '22:00'\nend = '07:00'\ntemperature = -1",
            "[night_shift]\nstart = '22:00'\nend = '07:00'\ntemperature = 80\nhelper = '/bin/sh'",
        ] {
            assert!(
                parse_declarations(Some(config.as_bytes())).is_err(),
                "{config}"
            );
        }
        let fixture = Fixture::new("off\n", "20\n");
        fs::remove_file(fixture.source.join(DECLARATION)).unwrap();
        fixture.put("config", CONFIG);
        symlink(
            fixture.home.join("config"),
            fixture.source.join(DECLARATION),
        )
        .unwrap();
        assert!(fixture.capture().is_err());
        assert!(fixture.writes().is_empty());
    }

    #[test]
    fn unsupported_version_malformed_output_and_read_failures_never_mean_drift() {
        for (name, content) in [
            ("version", "nightlight v2.0.0\n"),
            ("schedule", "10:00pm to 7:00am\n"),
            ("schedule", "22:00 to 7:00\nextra\n"),
            ("schedule", "13:00PM to 7:00AM\n"),
            ("temp", "101\n"),
            ("temp", "80 degrees\n"),
            ("read-error", ""),
            ("read-warning", ""),
        ] {
            let fixture = Fixture::new("off\n", "20\n");
            fixture.put(name, content);
            assert!(fixture.capture().is_err(), "{name}: {content}");
            assert!(fixture.writes().is_empty());
        }
    }
}
