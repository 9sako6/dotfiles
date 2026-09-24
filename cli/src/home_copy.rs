use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

pub struct CopyChange {
    pub path: String,
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone)]
pub struct CopyPlan {
    home: PathBuf,
    entries: Vec<CopyEntry>,
}

#[derive(Debug, Clone)]
struct CopyEntry {
    relative: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}

impl CopyPlan {
    pub fn changes(&self) -> Result<Vec<CopyChange>> {
        let mut changes = Vec::new();
        for entry in &self.entries {
            validate_unowned_parents(&self.home, &entry.relative)?;
            let after = fingerprint(&entry.source, true)?.context("copy source disappeared")?;
            let before = fingerprint(&entry.destination, false)?;
            if before.as_ref() != Some(&after) {
                changes.push(CopyChange {
                    path: entry.relative.display().to_string(),
                    before,
                    after,
                });
            }
        }
        Ok(changes)
    }

    pub fn apply(&self) -> Result<()> {
        // Validate every destination and source before changing the first entry.
        // Only declared entries are owned; their ancestors must never be replaced.
        self.changes()?;
        for entry in &self.entries {
            validate_unowned_parents(&self.home, &entry.relative)?;
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

fn validate_unowned_parents(home: &Path, relative: &Path) -> Result<()> {
    // HOME is the caller's anchor (and may itself resolve through a system link).
    match fs::metadata(home) {
        Ok(metadata) if metadata.is_dir() => (),
        Ok(_) => bail!("copy root is not a directory: {}", home.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error).context("cannot inspect copy root"),
    }
    let mut parent = home.to_path_buf();
    for component in relative
        .parent()
        .context("copy entry has no parent")?
        .components()
    {
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => (),
            Ok(_) => bail!(
                "copy parent must be a real directory; refusing to replace {}",
                parent.display()
            ),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => {
                return Err(error).with_context(|| format!("cannot inspect {}", parent.display()))
            }
        }
    }
    Ok(())
}

fn fingerprint(path: &Path, source: bool) -> Result<Option<String>> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(error).with_context(|| format!("cannot inspect {}", path.display()))
        }
    };
    let mut digest = Sha256::new();
    if metadata.file_type().is_symlink() {
        if source {
            bail!("copy source contains a symlink: {}", path.display());
        }
        digest.update(b"symlink");
        digest.update(fs::read_link(path)?.as_os_str().as_encoded_bytes());
    } else if metadata.is_file() {
        digest.update(b"file");
        let mode = (metadata.permissions().mode() | if source { 0o200 } else { 0 }) & 0o777;
        digest.update(mode.to_be_bytes());
        digest.update(fs::read(path)?);
    } else if metadata.is_dir() {
        digest.update(b"directory");
        let mut entries = read_entries(path)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let name = entry.file_name();
            let name = name.as_encoded_bytes();
            digest.update(name.len().to_be_bytes());
            digest.update(name);
            digest.update(fingerprint(&entry.path(), source)?.context("copy entry disappeared")?);
        }
    } else {
        bail!("unsupported copy entry: {}", path.display());
    }
    Ok(Some(format!("{:x}", digest.finalize())))
}

pub fn plan(repo_root: &Path, home: &Path, paths: &[String]) -> Result<CopyPlan> {
    validate_paths(paths)?;
    let source_root = repo_root.join("home");

    let mut entries = Vec::with_capacity(paths.len());
    for relative in paths {
        let relative_path = PathBuf::from(&relative);
        validate_unowned_parents(home, &relative_path)?;
        validate_unowned_parents(&source_root, &relative_path)?;
        let source = source_root.join(&relative_path);
        let metadata = fs::symlink_metadata(&source).with_context(|| {
            format!(
                "dotfiles.toml: copy source does not exist: {}",
                relative_path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                "dotfiles.toml: copy source must not be a symlink: {}",
                relative_path.display()
            );
        }
        if !metadata.is_file() && !metadata.is_dir() {
            bail!(
                "dotfiles.toml: copy source must be a file or directory: {}",
                relative_path.display()
            );
        }
        entries.push(CopyEntry {
            source,
            destination: home.join(&relative_path),
            relative: relative_path,
        });
    }

    Ok(CopyPlan {
        home: home.to_path_buf(),
        entries,
    })
}

fn validate_paths(paths: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;

    for value in paths {
        validate_relative_path(value)?;
        if !seen.insert(value.as_str()) {
            bail!("dotfiles.toml: copy contains duplicate entry: {value}");
        }
        if let Some(previous) = previous {
            if value.as_str() < previous {
                bail!(
                    "dotfiles.toml: copy entries must be alphabetical; {value} should come before {previous}"
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
                    "dotfiles.toml: copy entries must not overlap: {} and {}",
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
        bail!("dotfiles.toml: copy entries must not be empty");
    }
    let path = Path::new(value);
    if path.is_absolute() {
        bail!("dotfiles.toml: copy entry must be relative: {value}");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!("dotfiles.toml: invalid copy entry: {value}");
        }
    }
    Ok(())
}

fn sync_entry(source: &Path, destination: &Path) -> Result<()> {
    let after = fingerprint(source, true)?.context("copy source disappeared")?;
    if fingerprint(destination, false)?.as_ref() == Some(&after) {
        return Ok(());
    }
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
        // Parents are not owned by this file entry. Never repair them by removal.
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create parent {}", parent.display()))?;
    }
    let parent = destination
        .parent()
        .context("copy destination has no parent")?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?;
    fs::copy(source, temporary.path()).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    let mode = fs::metadata(source)?.permissions().mode() | 0o200;
    fs::set_permissions(temporary.path(), fs::Permissions::from_mode(mode))
        .with_context(|| format!("failed to set copy permissions: {}", destination.display()))?;
    match fs::symlink_metadata(destination) {
        Ok(metadata) if metadata.is_dir() => fs::remove_dir_all(destination)?,
        Ok(_) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error).context("cannot inspect copy destination"),
    }
    temporary
        .persist(destination)
        .with_context(|| format!("failed to replace {}", destination.display()))?;
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

    use super::plan;

    #[test]
    fn apply_replaces_store_links_and_prunes_owned_directory_only() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home-target");
        fs::create_dir_all(repo.join("home/.claude/skills/design-it")).unwrap();
        fs::write(repo.join("home/.claude/skills/design-it/SKILL.md"), "new\n").unwrap();
        fs::write(
            repo.join("dotfiles.toml"),
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

        let plan = plan(&repo, &home, &[".claude/skills".into()]).unwrap();
        assert_eq!(plan.changes().unwrap().len(), 1);
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
        assert!(plan.changes().unwrap().is_empty());
    }

    #[test]
    fn copy_changes_detect_content_modes_missing_files_and_extras() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source");
        let home = temp.path().join("home");
        fs::create_dir_all(source.join("home/managed")).unwrap();
        let original = source.join("home/managed/tool");
        fs::write(&original, "original").unwrap();
        fs::set_permissions(&original, fs::Permissions::from_mode(0o555)).unwrap();
        let plan = plan(&source, &home, &["managed".into()]).unwrap();
        let changes = plan.changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert!(changes[0].before.is_none());
        plan.apply().unwrap();
        assert!(plan.changes().unwrap().is_empty());
        let target = home.join("managed/tool");
        for change in ["content", "mode", "missing", "extra"] {
            match change {
                "content" => fs::write(&target, "changed").unwrap(),
                "mode" => fs::set_permissions(&target, fs::Permissions::from_mode(0o644)).unwrap(),
                "missing" => fs::remove_file(&target).unwrap(),
                "extra" => fs::write(home.join("managed/extra"), "obsolete").unwrap(),
                _ => unreachable!(),
            }
            let changes = plan.changes().unwrap();
            assert_eq!(changes.len(), 1, "{change}");
            assert_ne!(changes[0].before.as_ref(), Some(&changes[0].after));
            plan.apply().unwrap();
            assert!(plan.changes().unwrap().is_empty(), "{change}");
        }
    }

    #[test]
    fn unsafe_parent_is_rejected_before_preview_or_any_copy() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        let outside = temp.path().join("outside");
        fs::create_dir_all(repo.join("home/.claude")).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::create_dir_all(&outside).unwrap();
        fs::write(repo.join("home/.claude/settings.json"), "new").unwrap();
        fs::write(outside.join("history"), "keep").unwrap();
        symlink(&outside, home.join(".claude")).unwrap();
        assert!(plan(&repo, &home, &[".claude/settings.json".into()]).is_err());
        assert!(home.join(".claude").is_symlink());
        assert_eq!(fs::read_to_string(outside.join("history")).unwrap(), "keep");
        assert!(!outside.join("settings.json").exists());
    }
}
