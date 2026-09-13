use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct CopyPlan {
    entries: Vec<CopyEntry>,
}

#[derive(Debug, Clone)]
struct CopyEntry {
    relative: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}

impl CopyPlan {
    pub fn preview(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut lines = vec!["home copy plan:".to_owned()];
        lines.extend(self.entries.iter().map(|entry| {
            format!(
                "  {} -> {}",
                entry.relative.display(),
                entry.destination.display()
            )
        }));
        lines.join("\n")
    }

    pub fn apply(&self) -> Result<()> {
        for entry in &self.entries {
            sync_entry(&entry.source, &entry.destination).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    entry.relative.display(),
                    entry.destination.display()
                )
            })?;
        }
        Ok(())
    }
}

pub fn plan(repo_root: &Path, home: &Path) -> Result<CopyPlan> {
    let config_path = repo_root.join(".dotfiles.toml");
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let paths = parse_config(&raw)?;
    let source_root = repo_root.join("home");

    let mut entries = Vec::with_capacity(paths.len());
    for relative in paths {
        let relative_path = PathBuf::from(&relative);
        let source = source_root.join(&relative_path);
        let metadata = fs::symlink_metadata(&source).with_context(|| {
            format!(
                ".dotfiles.toml: copy source does not exist: {}",
                relative_path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                ".dotfiles.toml: copy source must not be a symlink: {}",
                relative_path.display()
            );
        }
        if !metadata.is_file() && !metadata.is_dir() {
            bail!(
                ".dotfiles.toml: copy source must be a file or directory: {}",
                relative_path.display()
            );
        }
        entries.push(CopyEntry {
            source,
            destination: home.join(&relative_path),
            relative: relative_path,
        });
    }

    Ok(CopyPlan { entries })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CopyConfig {
    #[serde(default)]
    copy: Vec<String>,
}

fn parse_config(raw: &str) -> Result<Vec<String>> {
    let config: CopyConfig = toml::from_str(raw).context(".dotfiles.toml: invalid TOML config")?;
    validate_paths(&config.copy)?;
    Ok(config.copy)
}

fn validate_paths(paths: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;

    for value in paths {
        validate_relative_path(value)?;
        if !seen.insert(value.as_str()) {
            bail!(".dotfiles.toml: copy contains duplicate entry: {value}");
        }
        if let Some(previous) = previous {
            if value.as_str() < previous {
                bail!(
                    ".dotfiles.toml: copy entries must be alphabetical; {value} should come before {previous}"
                );
            }
        }
        previous = Some(value);
    }

    for (index, path) in paths.iter().enumerate() {
        let path = Path::new(path);
        for other in &paths[index + 1..] {
            let other = Path::new(other);
            if other.starts_with(path) || path.starts_with(other) {
                bail!(
                    ".dotfiles.toml: copy entries must not overlap: {} and {}",
                    path.display(),
                    other.display()
                );
            }
        }
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<()> {
    if value.is_empty() {
        bail!(".dotfiles.toml: copy entries must not be empty");
    }
    let path = Path::new(value);
    if path.is_absolute() {
        bail!(".dotfiles.toml: copy entry must be relative: {value}");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!(".dotfiles.toml: invalid copy entry: {value}");
        }
    }
    Ok(())
}

fn sync_entry(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("copy source contains a symlink: {}", source.display());
    }
    if metadata.is_file() {
        sync_file(source, destination)
    } else if metadata.is_dir() {
        sync_directory(source, destination)
    } else {
        bail!("unsupported copy source: {}", source.display())
    }
}

fn sync_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        ensure_directory(parent)?;
    }
    if fs::symlink_metadata(destination).is_ok() {
        remove_entry(destination)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn sync_directory(source: &Path, destination: &Path) -> Result<()> {
    ensure_directory(destination)?;

    let mut source_names = BTreeSet::new();
    let mut source_entries = read_entries(source)?;
    source_entries.sort_by_key(|entry| entry.file_name());
    for entry in source_entries {
        let name = entry.file_name();
        source_names.insert(name.clone());
        sync_entry(&entry.path(), &destination.join(&name))?;
    }

    for entry in read_entries(destination)? {
        if !source_names.contains(&entry.file_name()) {
            remove_entry(&entry.path())?;
        }
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => {
            remove_entry(path)?;
            fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).with_context(|| format!("failed to create {}", path.display()))
        }
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn read_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read {}", path.display()))
}

fn remove_entry(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).with_context(|| format!("failed to remove {}", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::{parse_config, plan};

    #[test]
    fn parses_copy_paths() {
        let raw = r#"
          copy = [
            ".agents/skills",
            '.claude/settings.json', # literal paths are accepted
            "\u8a2d\u5b9a",
          ]
        "#;
        assert_eq!(
            parse_config(raw).unwrap(),
            vec![".agents/skills", ".claude/settings.json", "設定"]
        );
    }

    #[test]
    fn accepts_empty_copy_config() {
        for raw in ["", "# no home copies\n", "copy = []"] {
            assert!(parse_config(raw).unwrap().is_empty());
        }
    }

    #[test]
    fn rejects_invalid_config() {
        for raw in [
            "copi = []",
            "[extra]",
            "copy = []\ncopy = []",
            "copy = 1",
            "copy = 'a'",
            "copy = [1]",
            "copy = ['a', false]",
            "copy = {}",
            "copy = ['a'",
            "copy = [] trailing",
            r#"{"copy": []}"#,
            r#"copy = ["b", "a"]"#,
            r#"copy = ["a", "a"]"#,
            r#"copy = [""]"#,
            r#"copy = ["/secret"]"#,
            r#"copy = ["."]"#,
            r#"copy = ["./secret"]"#,
            r#"copy = ["../secret"]"#,
            r#"copy = ["a/../secret"]"#,
            r#"copy = [".claude", ".claude/settings.json"]"#,
        ] {
            assert!(parse_config(raw).is_err(), "should reject {raw}");
        }
    }

    #[test]
    fn apply_replaces_store_links_and_prunes_owned_directory_only() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home-target");
        fs::create_dir_all(repo.join("home/.claude/skills/design-it")).unwrap();
        fs::write(repo.join("home/.claude/skills/design-it/SKILL.md"), "new\n").unwrap();
        fs::write(
            repo.join(".dotfiles.toml"),
            "copy = [\n  \".claude/skills\",\n]\n",
        )
        .unwrap();

        fs::create_dir_all(home.join(".claude/skills/old")).unwrap();
        fs::write(home.join(".claude/skills/old/SKILL.md"), "old\n").unwrap();
        fs::write(home.join(".claude/runtime.json"), "keep\n").unwrap();
        let store_file = temp.path().join("store-skill");
        fs::write(&store_file, "store\n").unwrap();
        fs::create_dir_all(home.join(".claude/skills/design-it")).unwrap();
        symlink(&store_file, home.join(".claude/skills/design-it/SKILL.md")).unwrap();

        let plan = plan(&repo, &home).unwrap();
        assert_eq!(
            plan.preview(),
            format!(
                "home copy plan:\n  .claude/skills -> {}",
                home.join(".claude/skills").display()
            )
        );
        assert_eq!(
            fs::read_to_string(home.join(".claude/skills/design-it/SKILL.md")).unwrap(),
            "store\n"
        );
        assert!(home.join(".claude/skills/old").exists());
        plan.apply().unwrap();

        let deployed = home.join(".claude/skills/design-it/SKILL.md");
        assert_eq!(fs::read_to_string(&deployed).unwrap(), "new\n");
        assert!(!fs::symlink_metadata(&deployed)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(!home.join(".claude/skills/old").exists());
        assert_eq!(
            fs::read_to_string(home.join(".claude/runtime.json")).unwrap(),
            "keep\n"
        );
    }
}
