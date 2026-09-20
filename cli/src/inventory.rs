mod latest;
mod render;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Inventory {
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

#[derive(Clone, Deserialize)]
struct Package {
    name: String,
    manager: String,
    declared: String,
    lookup: Value,
    #[serde(skip)]
    latest: String,
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

pub fn run(
    inputs: crate::system::InventoryInputs,
    settings: &[crate::settings::Setting],
    interactive: bool,
) -> Result<ExitCode> {
    let (mut inventory, source): (Inventory, _) = inputs.load()?;
    inventory.prepare(&source)?;
    if interactive {
        let mut checks = latest::Checks::start(&inventory.packages);
        return render::live(settings, &mut inventory, &mut checks);
    }
    let mut output = io::stdout().lock();
    writeln!(output, "\n{}", render::report(&inventory, None, false))?;
    output.flush()?;
    Ok(ExitCode::SUCCESS)
}

pub fn preview(current: Option<&Path>, desired: &Path) -> Result<String> {
    let width = io::stdout()
        .is_terminal()
        .then(|| usize::from(console::Term::stdout().size().1));
    let next = read_generation(desired)?.context("planned generation has no inventory")?;
    let previous = current.map(read_generation).transpose()?.flatten();
    if current.is_some() && previous.is_none() {
        return Ok("resource diff unavailable: active generation has no inventory; using native diff for this transition".into());
    }
    Ok(render::diff(previous.as_ref(), &next, width))
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
        for package in &mut self.packages {
            package.latest = "checking".into();
        }
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
        assert_eq!(preview(Some(&new), &new).unwrap(), "no resource changes");
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
}
