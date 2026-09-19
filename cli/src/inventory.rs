mod latest;
mod render;

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
pub struct Inventory {
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

pub fn run(inputs: crate::system::InventoryInputs) -> Result<ExitCode> {
    let (mut inventory, source): (Inventory, _) = inputs.load()?;
    inventory.prepare(&source)?;
    let interactive = io::stdout().is_terminal()
        && io::stdin().is_terminal()
        && std::env::var("TERM").is_ok_and(|term| term != "dumb");
    let mut checks = latest::Checks::start(&inventory.packages);
    let cancelled = if interactive {
        render::live(&mut inventory, &mut checks)?
    } else {
        while let Some((indices, result)) = checks.recv() {
            for index in indices {
                inventory.packages[index].latest = result.clone();
            }
        }
        false
    };
    let width = io::stdout()
        .is_terminal()
        .then(|| usize::from(console::Term::stdout().size().1));
    let mut output = io::stdout().lock();
    writeln!(output, "\n{}", render::report(&inventory, width))?;
    output.flush()?;
    Ok(if cancelled {
        ExitCode::from(130)
    } else {
        ExitCode::SUCCESS
    })
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
