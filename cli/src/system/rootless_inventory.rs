//! Read-only current declarations, separate from the last active system state.
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use tabled::builder::Builder;
use tabled::settings::peaker::PriorityMax;
use tabled::settings::{Style, Width};

use super::configuration::Configuration;

/// Retain the existing settings table shape, including per-leaf provenance.
pub(super) fn settings_json(
    public: &[u8],
    local: Option<&[u8]>,
    configuration: &Configuration,
) -> Result<Value> {
    let public = parse_toml(public, "dotfiles.toml")?;
    let local = local
        .map(|bytes| parse_toml(bytes, "dotfiles.local.toml"))
        .transpose()?
        .unwrap_or_else(|| toml::Value::Table(Default::default()));
    let values = configuration.system_value();
    let mut rows = Vec::new();
    fn flatten(
        prefix: &str,
        value: &Value,
        public: &toml::Value,
        local: &toml::Value,
        rows: &mut Vec<Value>,
    ) {
        if let Some(table) = value.as_object() {
            for (key, value) in table {
                let key = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                flatten(&key, value, public, local, rows);
            }
        } else {
            let present = |root: &toml::Value| {
                prefix
                    .split('.')
                    .try_fold(root, |value, key| value.get(key))
                    .is_some()
            };
            let source = if present(local) {
                Some("dotfiles.local.toml")
            } else if present(public) {
                Some("dotfiles.toml")
            } else {
                None
            };
            rows.push(json!({ "key": prefix, "value": value, "source": source }));
        }
    }
    flatten("", &values, &public, &local, &mut rows);
    rows.sort_by(|a, b| a["key"].as_str().cmp(&b["key"].as_str()));
    Ok(Value::Array(rows))
}

fn parse_toml(bytes: &[u8], file: &str) -> Result<toml::Value> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| anyhow::anyhow!("{file}: invalid encoding"))?;
    toml::from_str(text).map_err(|_| anyhow::anyhow!("{file}: invalid TOML or schema"))
}

/// Current public declarations only. Never merges active system records here.
pub(super) fn inventory_json(source: &Path, configuration: &Configuration) -> Result<Value> {
    let packages = super::tools::declared_inventory(source)?;
    let mut home: BTreeMap<String, &str> = configuration
        .copy
        .iter()
        .map(|path| (path.clone(), "copy"))
        .collect();
    for path in crate::home_copy::live_paths(source)? {
        home.entry(path.to_string_lossy().into_owned())
            .or_insert("symlink");
    }
    let home: Vec<_> = home
        .into_iter()
        .map(|(path, manager)| json!({"path": path, "manager": manager}))
        .collect();
    Ok(
        json!({"packages": packages, "home": home, "services": super::user_services::declared_inventory(&configuration.services)?,
        "userSettings": super::user_settings::declared_inventory(&configuration.settings)?, "localllm": configuration.localllm }),
    )
}

fn table(headers: &[&str], rows: Vec<Vec<String>>, width: Option<usize>) -> String {
    let mut builder = Builder::default();
    builder.push_record(headers.iter().copied());
    for row in rows {
        builder.push_record(row.into_iter().map(|value| clean(&value)));
    }
    let mut table = builder.build();
    table.with(Style::empty());
    if let Some(width) = width {
        table.with(Width::wrap(width).priority(PriorityMax::default()));
    }
    table
        .to_string()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn clean(text: &str) -> String {
    text.chars()
        .flat_map(|character| {
            if character.is_control() {
                character.escape_default().collect::<Vec<_>>()
            } else {
                vec![character]
            }
        })
        .collect()
}

/// Rootless desired declarations followed by a separately labelled active-system
/// summary. Reading a generation never evaluates its Nix configuration.
pub(super) fn report(
    source: &Path,
    configuration: &Configuration,
    active_generation: Option<&Path>,
    width: Option<usize>,
) -> Result<String> {
    let inventory = inventory_json(source, configuration)?;
    let mut sections = vec![format!(
        "packages\n{}",
        table(
            &["name", "manager", "declared", "status"],
            inventory["packages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| vec![
                    p["name"].as_str().unwrap().into(),
                    "mise".into(),
                    p["declared"].as_str().unwrap().into(),
                    if p["disabled"] == true {
                        "disabled"
                    } else {
                        "declared"
                    }
                    .into(),
                ])
                .collect(),
            width
        )
    )];
    sections.push(format!(
        "home (current public declarations)\n{}",
        table(
            &["path", "manager"],
            inventory["home"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| vec![
                    format!("~/{}", p["path"].as_str().unwrap()),
                    p["manager"].as_str().unwrap().into(),
                ])
                .collect(),
            width
        )
    ));
    sections.push(format!(
        "user services (launchd; current declarations)\n{}",
        table(
            &["name", "manager", "schedule"],
            inventory["services"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| vec![
                    p["name"].as_str().unwrap().into(),
                    "launchd".into(),
                    p["config"].to_string(),
                ])
                .collect(),
            width
        )
    ));
    sections.push(format!(
        "user settings (current declarations)\n{}",
        table(
            &["name", "manager", "declared"],
            inventory["userSettings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|p| vec![
                    p["name"].as_str().unwrap().into(),
                    p["manager"].as_str().unwrap().into(),
                    format!(
                        "{} to {}; temperature {}",
                        p["start"].as_str().unwrap(),
                        p["end"].as_str().unwrap(),
                        p["temperature"]
                    ),
                ])
                .collect(),
            width
        )
    ));
    sections.push(format!("artifacts (Nix; current configuration)\nlocalllm enabled: {}; default model: {}\nArtifact outputs are inspected by plan/apply; no artifact evaluation was performed", configuration.localllm.enabled,
        clean(configuration.localllm.default_model.as_deref().unwrap_or("—"))));
    let system = match active_generation {
        None => "No active generation; system/private details unavailable".into(),
        Some(generation) => {
            let path = generation.join("dotfiles-inventory.json");
            match fs::read(path) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    "Active generation has no inventory; system/private details unavailable".into()
                }
                Err(error) => return Err(error).context("cannot read active system inventory"),
                Ok(bytes) => {
                    let inventory: Value = serde_json::from_slice(&bytes)
                        .map_err(|_| anyhow::anyhow!("invalid active system inventory"))?;
                    if !inventory.is_object() {
                        bail!("invalid active system inventory");
                    }
                    let entries = |key: &str| -> Result<Vec<&Value>> {
                        match inventory.get(key) {
                            None => Ok(Vec::new()),
                            Some(Value::Array(values)) => Ok(values.iter().collect()),
                            Some(_) => bail!("invalid active system inventory"),
                        }
                    };
                    let mut packages = Vec::new();
                    for item in entries("packages")? {
                        let name = item["name"]
                            .as_str()
                            .context("invalid active package record")?;
                        let manager = item["manager"]
                            .as_str()
                            .context("invalid active package record")?;
                        let declared = item["declared"]
                            .as_str()
                            .context("invalid active package record")?;
                        if manager != "mise" {
                            packages.push(vec![name.into(), manager.into(), declared.into()]);
                        }
                    }
                    let mut settings = Vec::new();
                    for item in entries("system")? {
                        let key = item["key"]
                            .as_str()
                            .context("invalid active system setting")?;
                        let value = item.get("value").context("invalid active system setting")?;
                        settings.push(vec![key.into(), value.to_string()]);
                    }
                    let mut services = Vec::new();
                    for item in entries("services")? {
                        let name = item["name"]
                            .as_str()
                            .context("invalid active service record")?;
                        let scope = item["scope"]
                            .as_str()
                            .context("invalid active service record")?;
                        let config = item["config"]
                            .as_object()
                            .context("invalid active service record")?;
                        let schedule: serde_json::Map<String, Value> = [
                            "RunAtLoad",
                            "KeepAlive",
                            "StartCalendarInterval",
                            "StartInterval",
                        ]
                        .into_iter()
                        .filter_map(|key| config.get(key).map(|value| (key.into(), value.clone())))
                        .collect();
                        services.push(vec![
                            name.into(),
                            scope.into(),
                            Value::Object(schedule).to_string(),
                        ]);
                    }
                    format!("Active generation: {}\nThese are last-applied records, not current desired configuration\n{}\n\nrecorded system settings\n{}\n\nrecorded system/private services\n{}",
                        clean(&generation.display().to_string()),
                        table(&["package", "manager", "recorded"], packages, width),
                        table(&["setting", "recorded"], settings, width),
                        table(&["service", "scope", "schedule"], services, width))
                }
            }
        }
    };
    sections.push(format!(
        "system/private (nix-darwin; active generation only)\n{system}"
    ));
    Ok(sections.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir_all(root.path().join("home/.config/mise")).unwrap();
        fs::write(
            root.path().join("dotfiles.toml"),
            "copy = ['.claude/settings.json']",
        )
        .unwrap();
        fs::write(root.path().join("home/.config/mise/mise.lock"), "").unwrap();
        fs::write(root.path().join("home/.config/mise/config.toml"), "[tools]\nalpha = '1.2.3'\nbeta = [{ version = '2.0.0', options = true }, '3.0.0']\nawscli = '4.0.0'\n[env]\nTOKEN = 'secret-do-not-print'\n[settings]\ndisable_tools = ['awscli']\n").unwrap();
        root
    }

    #[test]
    fn provenance_preserves_defaults_false_and_replaced_arrays() {
        let public = b"copy = ['a']\n[localllm]\nenabled = true\nmodels = ['model']\ndefault_model = 'model'\n";
        let local = b"[localllm]\nenabled = false\nmodels = []\n[private]\npath = '../private'\n";
        let config = Configuration::parse(public, Some(local), &["model".into()]).unwrap();
        let settings = settings_json(public, Some(local), &config).unwrap();
        let rows: BTreeMap<_, _> = settings
            .as_array()
            .unwrap()
            .iter()
            .map(|row| (row["key"].as_str().unwrap(), row))
            .collect();
        assert_eq!(rows["copy"]["source"], "dotfiles.toml");
        assert_eq!(rows["localllm.enabled"]["value"], false);
        assert_eq!(rows["localllm.models"]["value"], json!([]));
        assert_eq!(rows["localllm.models"]["source"], "dotfiles.local.toml");
        assert_eq!(rows["localllm.default_model"]["source"], "dotfiles.toml");
        let defaults = settings_json(b"", None, &Configuration::default()).unwrap();
        assert!(defaults
            .as_array()
            .unwrap()
            .iter()
            .all(|row| row["source"].is_null()));
    }

    #[test]
    fn report_labels_ownership_without_launching_backends_or_printing_environment() {
        let root = fixture();
        let config = Configuration::parse(b"copy = ['.claude/settings.json']\n[[services.agents]]\nlabel = 'com.example.check'\nargv = ['~/bin/check', 'secret-do-not-print']\nstart_interval = 60\n[settings.night_shift]\nstart = '22:00'\nend = '07:00'\ntemperature = 80\n", None, &[]).unwrap();
        let text = report(root.path(), &config, None, None).unwrap();
        for expected in [
            "alpha",
            "1.2.3",
            "2.0.0, 3.0.0",
            "disabled",
            "symlink",
            "copy",
            "launchd",
            "Night Shift",
            "nix-darwin",
            "No active generation",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        assert!(!text.contains("secret-do-not-print"));
    }

    #[test]
    fn stale_system_is_explicit_and_never_merged_into_desired_tools() {
        let root = fixture();
        let generation = root.path().join("generation");
        fs::create_dir(&generation).unwrap();
        fs::write(
            generation.join("dotfiles-inventory.json"),
            json!({"packages": [
                {"name":"obsolete-tool", "manager":"mise", "declared":"old"},
                {"name":"private-package", "manager":"Nix", "declared":"1.0"}
            ], "system":[{"key":"time.timeZone", "value":"Asia/Tokyo"}],
            "services":[{"name":"private-service", "scope":"user", "config":{"RunAtLoad":true, "ProgramArguments":["secret-do-not-print"]}}]
            })
            .to_string(),
        )
        .unwrap();
        let text = report(
            root.path(),
            &Configuration::default(),
            Some(&generation),
            None,
        )
        .unwrap();
        assert!(!text.contains("obsolete-tool"));
        assert!(text.contains("private-package"));
        assert!(text.contains("Asia/Tokyo"));
        assert!(text.contains("private-service"));
        assert!(!text.contains("secret-do-not-print"));
        assert!(text.contains("last-applied records, not current desired"));
    }
}
