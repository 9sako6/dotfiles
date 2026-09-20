mod render;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Inventory {
    #[serde(default, rename = "schemaVersion")]
    schema_version: u32,
    source: PathBuf,
    packages: Vec<Package>,
    system: Vec<Setting>,
    services: Vec<Service>,
    tools: Vec<Tool>,
    localllm: LocalLlm,
    #[serde(rename = "timeZone")]
    time_zone: String,
    #[serde(skip)]
    skills: Vec<Skill>,
}

#[derive(Deserialize)]
struct Package {
    name: String,
    manager: String,
    declared: String,
}

#[derive(Deserialize)]
struct Setting {
    key: String,
    group: String,
    name: String,
    value: Value,
}

#[derive(Deserialize)]
struct Service {
    name: String,
    scope: String,
    config: Value,
}

#[derive(Deserialize)]
struct Tool {
    path: String,
    deploy: String,
}

#[derive(Deserialize)]
struct LocalLlm {
    enabled: bool,
    default_model: Option<String>,
}

struct Skill {
    name: String,
    origin: String,
    description: String,
}

pub fn report(inputs: crate::system::InventoryInputs, width: Option<usize>) -> Result<String> {
    let (mut inventory, source): (Inventory, _) = inputs.load()?;
    inventory.prepare(&source)?;
    Ok(render::report(&inventory, width))
}

pub struct Preview {
    previous: Option<Inventory>,
    next: Inventory,
    notice: Option<&'static str>,
    generation: Option<(String, String)>,
    pub copy_changes: Vec<crate::home_copy::CopyChange>,
    pub native: String,
}

impl Preview {
    pub fn load(current: Option<&Path>, desired: &Path) -> Result<Self> {
        let next = read_generation(desired)?.context("planned generation has no inventory")?;
        let previous = current.map(read_generation).transpose()?.flatten();
        let notice = if current.is_some() && previous.is_none() {
            Some("resource diff unavailable: active generation has no inventory; using native diff for this transition")
        } else if previous
            .as_ref()
            .is_some_and(|previous| previous.schema_version != next.schema_version)
        {
            Some("resource diff unavailable: generation inventories cover different settings; using native diff for this transition")
        } else {
            None
        };
        let generation =
            if notice.is_none() && render::diff(previous.as_ref(), &next, None).is_empty() {
                current
                    .map(|current| -> Result<_> {
                        let current = current.canonicalize()?;
                        let desired = desired.canonicalize()?;
                        if current == desired {
                            return Ok(None);
                        }
                        let before = generation_revision(&current)?;
                        let after = generation_revision(&desired)?;
                        if before != after {
                            return Ok(Some((before, after)));
                        }
                        Ok(Some((generation_id(&current), generation_id(&desired))))
                    })
                    .transpose()?
                    .flatten()
            } else {
                None
            };
        Ok(Self {
            previous,
            next,
            notice,
            generation,
            copy_changes: Vec::new(),
            native: String::new(),
        })
    }

    pub fn needs_native(&self) -> bool {
        self.notice.is_some()
    }

    pub fn has_changes(&self) -> bool {
        !self.render(None).is_empty()
    }

    pub fn show(&self) -> Result<()> {
        let text = self.render(crate::terminal_width());
        if !text.is_empty() {
            let mut output = io::stdout().lock();
            writeln!(output, "{text}")?;
            output.flush()?;
        }
        Ok(())
    }

    fn render(&self, width: Option<usize>) -> String {
        let resources = if let Some(notice) = self.notice {
            format!("{notice}\n\n{}", self.native).trim_end().into()
        } else {
            render::diff(self.previous.as_ref(), &self.next, width)
        };
        let deployment = render::deployment(self.generation.as_ref(), &self.copy_changes, width);
        [resources, deployment]
            .into_iter()
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}

fn generation_revision(path: &Path) -> Result<String> {
    let bytes = match fs::read(path.join("darwin-version.json")) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(generation_id(path)),
        Err(error) => return Err(error).context("cannot read generation revision"),
    };
    let version: Value = serde_json::from_slice(&bytes).context("invalid generation revision")?;
    Ok(version["configurationRevision"]
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| generation_id(path)))
}

fn generation_id(path: &Path) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "build {:x}",
        Sha256::digest(path.as_os_str().as_encoded_bytes())
    )[..18]
        .into()
}

fn read_generation(generation: &Path) -> Result<Option<Inventory>> {
    let path = generation.join("dotfiles-inventory.json");
    match fs::symlink_metadata(&path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot read generation inventory"),
        Ok(_) => (),
    }
    let bytes = fs::read(path).context("cannot read generation inventory")?;
    let mut inventory: Inventory =
        serde_json::from_slice(&bytes).context("invalid generation inventory")?;
    inventory.prepare(&inventory.source.clone())?;
    Ok(Some(inventory))
}

impl Inventory {
    fn prepare(&mut self, source: &Path) -> Result<()> {
        self.packages
            .sort_by_key(|p| (p.name.to_lowercase(), p.manager.clone(), p.declared.clone()));
        self.packages.dedup_by(|a, b| {
            a.name == b.name && a.manager == b.manager && a.declared == b.declared
        });
        self.system.sort_by(|a, b| a.key.cmp(&b.key));
        self.services.sort_by(|a, b| a.name.cmp(&b.name));
        self.tools.sort_by(|a, b| a.path.cmp(&b.path));
        self.skills = skills(source)?;
        Ok(())
    }
}

fn skills(source: &Path) -> Result<Vec<Skill>> {
    let manifest: serde_yaml_ng::Value = serde_yaml_ng::from_str(
        &fs::read_to_string(source.join("home/apm.yml"))
            .context("cannot read skill declarations")?,
    )?;
    let dependencies = manifest["dependencies"]["apm"]
        .as_sequence()
        .context("invalid APM skill declarations")?;
    let mut skills = Vec::new();
    for dependency in dependencies {
        let dependency = dependency
            .as_str()
            .context("invalid APM skill dependency")?;
        let name = dependency
            .split('#')
            .next()
            .unwrap_or(dependency)
            .rsplit('/')
            .next()
            .unwrap_or(dependency);
        let path = source
            .join("home/.agents/skills")
            .join(name)
            .join("SKILL.md");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("cannot read compiled skill {name}"))?;
        let metadata = text
            .strip_prefix("---\n")
            .and_then(|s| s.split_once("\n---"))
            .context("invalid skill frontmatter")?
            .0;
        let metadata: BTreeMap<String, serde_yaml_ng::Value> = serde_yaml_ng::from_str(metadata)?;
        let description = metadata
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("—")
            .to_owned();
        skills.push(Skill {
            name: name.into(),
            origin: if dependency.starts_with('.') {
                "local".into()
            } else {
                dependency.split('/').take(2).collect::<Vec<_>>().join("/")
            },
            description,
        });
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(skills)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn preview(current: Option<&Path>, desired: &Path) -> Result<String> {
        Ok(Preview::load(current, desired)?.render(None))
    }

    fn generation(root: &Path, name: &str, version: &str, description: &str) -> PathBuf {
        let generation = root.join(name);
        let source = generation.join("source");
        let skill = source.join("home/.agents/skills/example");
        fs::create_dir_all(&skill).unwrap();
        fs::write(
            source.join("home/apm.yml"),
            "dependencies:\n  apm: [./skills/example]\n",
        )
        .unwrap();
        fs::write(
            skill.join("SKILL.md"),
            format!("---\ndescription: {description}\n---\n"),
        )
        .unwrap();
        fs::write(generation.join("dotfiles-inventory.json"), serde_json::to_vec(&serde_json::json!({
            "source": source,
            "packages": [{"name": "node", "manager": "mise", "declared": version, "lookup": {"kind": "must-not-fetch"}}],
            "system": [], "services": [], "tools": [], "timeZone": "UTC",
            "localllm": {"enabled": false, "default_model": null}
        })).unwrap()).unwrap();
        generation
    }

    #[test]
    fn preview_compares_frozen_generations_without_latest_queries_and_supports_rollback() {
        let root = tempfile::tempdir().unwrap();
        let old = generation(
            root.path(),
            "old",
            "1.0.0",
            "Original description. Use when needed.",
        );
        let new = generation(
            root.path(),
            "new",
            "2.0.0",
            "Updated description. Use when needed.",
        );
        let forward = preview(Some(&old), &new).unwrap();
        let backward = preview(Some(&new), &old).unwrap();
        let forward = console::strip_ansi_codes(&forward);
        let backward = console::strip_ansi_codes(&backward);
        assert!(forward
            .lines()
            .any(|s| s.starts_with('-') && s.contains("1.0.0")));
        assert!(forward
            .lines()
            .any(|s| s.starts_with('+') && s.contains("2.0.0")));
        assert!(backward
            .lines()
            .any(|s| s.starts_with('+') && s.contains("1.0.0")));
        assert!(backward
            .lines()
            .any(|s| s.starts_with('-') && s.contains("2.0.0")));
        assert!(forward.contains("Original description. Use when needed."));
        assert!(forward.contains("Updated description. Use when needed."));
        assert!(!forward.contains("latest"));
        assert!(!forward.contains("localllm"));
        assert_eq!(preview(Some(&new), &new).unwrap(), "");
        let initial = preview(None, &new).unwrap();
        let initial = console::strip_ansi_codes(&initial);
        assert!(initial.lines().any(|s| s.starts_with('+')));
        assert!(!initial.lines().any(|s| s.starts_with('-')));
    }

    #[test]
    fn legacy_generation_is_explicit_but_invalid_or_broken_snapshots_fail() {
        let root = tempfile::tempdir().unwrap();
        let new = generation(root.path(), "new", "2.0.0", "A description.");
        let old = root.path().join("old");
        fs::create_dir(&old).unwrap();
        assert!(preview(Some(&old), &new)
            .unwrap()
            .contains("active generation has no inventory"));
        let inventory = old.join("dotfiles-inventory.json");
        fs::write(&inventory, "invalid json").unwrap();
        assert!(preview(Some(&old), &new).is_err());
        fs::remove_file(&inventory).unwrap();
        symlink(old.join("missing"), &inventory).unwrap();
        assert!(preview(Some(&old), &new).is_err());
        fs::remove_file(new.join("dotfiles-inventory.json")).unwrap();
        assert!(preview(None, &new).is_err());
    }

    #[test]
    fn changed_inventory_coverage_is_not_reported_as_added_or_removed_resources() {
        let root = tempfile::tempdir().unwrap();
        let old = generation(root.path(), "old", "1.0.0", "A description.");
        let new = generation(root.path(), "new", "1.0.0", "A description.");
        for (old_schema, new_schema) in [(0, 2), (2, 3)] {
            for (generation, schema) in [(&old, old_schema), (&new, new_schema)] {
                let path = generation.join("dotfiles-inventory.json");
                let mut inventory: Value =
                    serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
                inventory["schemaVersion"] = schema.into();
                if generation == &new {
                    inventory["system"] = serde_json::json!([
                        {"key": "nix.gc.options", "group": "nix.gc", "name": "options", "value": "--delete-older-than 2d"}
                    ]);
                    inventory["packages"] = serde_json::json!([
                        {"name": "lix", "manager": "Nix", "declared": "2.95.2"}
                    ]);
                }
                fs::write(path, serde_json::to_vec(&inventory).unwrap()).unwrap();
            }
            for (before, after) in [(&old, &new), (&new, &old)] {
                let output = preview(Some(before), after).unwrap();
                assert!(output.contains("inventories cover different settings"));
                assert!(!output.contains("+ "));
                assert!(!output.contains("- "));
                assert!(!output.contains("no resource changes"));
            }
        }
    }

    #[test]
    fn management_policies_and_system_packages_are_reported_and_compared_without_duplicates() {
        let root = tempfile::tempdir().unwrap();
        let old = generation(root.path(), "old", "1.0.0", "A description.");
        let new = generation(root.path(), "new", "1.0.0", "A description.");
        let policies = [
            (
                "homebrew.global.autoUpdate",
                serde_json::json!(false),
                serde_json::json!(true),
            ),
            (
                "homebrew.onActivation.autoUpdate",
                serde_json::json!(false),
                serde_json::json!(true),
            ),
            (
                "homebrew.onActivation.cleanup",
                serde_json::json!("uninstall"),
                serde_json::json!("zap"),
            ),
            (
                "homebrew.onActivation.upgrade",
                serde_json::json!(false),
                serde_json::json!(true),
            ),
            (
                "nix-homebrew.mutableTaps",
                serde_json::json!(false),
                serde_json::json!(true),
            ),
            (
                "nix.gc.automatic",
                serde_json::json!(true),
                serde_json::json!(false),
            ),
            (
                "nix.gc.options",
                serde_json::json!("--delete-older-than 2d"),
                serde_json::json!("--delete-older-than 7d"),
            ),
        ];
        for (generation, version, changed) in [(&old, "1.0.0", false), (&new, "2.0.0", true)] {
            let path = generation.join("dotfiles-inventory.json");
            let mut inventory: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            inventory["schemaVersion"] = 3.into();
            inventory["packages"] = serde_json::json!([
                {"name": "dotfiles", "manager": "Nix", "declared": version},
                {"name": "lix", "manager": "Nix", "declared": version},
                {"name": "lix", "manager": "Nix", "declared": version},
                {"name": "zundamonotify", "manager": "Nix", "declared": version}
            ]);
            inventory["system"] = policies.iter().map(|(key, before, after)| {
                let (group, name) = key.rsplit_once('.').unwrap();
                serde_json::json!({"key": key, "group": group, "name": name, "value": if changed {after} else {before}})
            }).collect();
            fs::write(path, serde_json::to_vec(&inventory).unwrap()).unwrap();
        }
        let inventory = read_generation(&new).unwrap().unwrap();
        let report = render::report(&inventory, None);
        for name in ["dotfiles", "lix", "zundamonotify"] {
            assert_eq!(
                report
                    .lines()
                    .filter(|line| line.split_whitespace().next() == Some(name))
                    .count(),
                1
            );
        }
        let system = report
            .split("\nsystem\n")
            .nth(1)
            .unwrap()
            .split("\nservices\n")
            .next()
            .unwrap();
        assert!(!system.contains('—'));
        for (before, after, removed, added) in [
            (&old, &new, "1.0.0", "2.0.0"),
            (&new, &old, "2.0.0", "1.0.0"),
        ] {
            let output = preview(Some(before), after).unwrap();
            for name in ["dotfiles", "lix", "zundamonotify"] {
                for (sign, version) in [("-", removed), ("+", added)] {
                    assert_eq!(
                        output
                            .lines()
                            .filter(|line| line.split_whitespace().collect::<Vec<_>>()
                                == [sign, name, "Nix", version])
                            .count(),
                        1
                    );
                }
            }
            for (key, _, _) in &policies {
                let (group, name) = key.rsplit_once('.').unwrap();
                let section = output
                    .split(&format!("{group}\n"))
                    .nth(1)
                    .unwrap()
                    .split("\n\n")
                    .next()
                    .unwrap();
                for sign in ["-", "+"] {
                    assert!(section.lines().any(|line| line
                        .split_whitespace()
                        .take(2)
                        .collect::<Vec<_>>()
                        == [sign, name]));
                }
            }
        }
    }

    #[test]
    fn activation_settings_appear_in_reports_and_generation_diffs() {
        let root = tempfile::tempdir().unwrap();
        let old = generation(root.path(), "old", "1.0.0", "A description.");
        let new = generation(root.path(), "new", "1.0.0", "A description.");
        for (generation, start, temperature, shortcut) in
            [(&old, "22:00", 80, true), (&new, "21:15", 65, false)]
        {
            let path = generation.join("dotfiles-inventory.json");
            let mut inventory: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
            inventory["schemaVersion"] = 2.into();
            inventory["system"] = serde_json::json!([
                {"key": "nightShift.schedule.start", "group": "nightShift.schedule", "name": "start", "value": start},
                {"key": "nightShift.schedule.end", "group": "nightShift.schedule", "name": "end", "value": "07:00"},
                {"key": "nightShift.temperature", "group": "nightShift", "name": "temperature", "value": temperature},
                {"key": "dictationShortcut.enabled", "group": "dictationShortcut", "name": "enabled", "value": shortcut}
            ]);
            fs::write(path, serde_json::to_vec(&inventory).unwrap()).unwrap();
        }
        let inventory = read_generation(&new).unwrap().unwrap();
        let report = render::report(&inventory, None);
        assert!(report.contains("nightShift.schedule"));
        assert!(report.contains("21:15"));
        assert!(report.contains("07:00"));
        assert!(report.contains("dictationShortcut"));
        for (before, after, removed, added) in [
            (
                &old,
                &new,
                ["22:00", "80", "true"],
                ["21:15", "65", "false"],
            ),
            (
                &new,
                &old,
                ["21:15", "65", "false"],
                ["22:00", "80", "true"],
            ),
        ] {
            let output = preview(Some(before), after).unwrap();
            let output = console::strip_ansi_codes(&output);
            for (prefix, values) in [('-', removed), ('+', added)] {
                for value in values {
                    assert!(output.lines().any(|line| {
                        line.starts_with(prefix)
                            && line.split_whitespace().any(|word| word == value)
                    }));
                }
            }
            assert!(!output.contains("07:00"));
            assert!(!output.contains("latest"));
            assert!(!output.contains("packages"));
        }
    }
}
