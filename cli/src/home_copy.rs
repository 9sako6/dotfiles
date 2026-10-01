use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

mod live;
mod state;

pub(crate) use live::{inventory_paths as live_paths, roots as live_roots};

pub struct CopyChange {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CopyPlan {
    home: PathBuf,
    entries: Vec<CopyEntry>,
    live: Option<live::Plan>,
    artifact_handoffs: BTreeMap<PathBuf, PathBuf>,
    artifact_retirements: BTreeMap<PathBuf, String>,
}

#[derive(Debug, Clone)]
struct CopyEntry {
    relative: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}

impl CopyPlan {
    pub(crate) fn with_artifact_retirements(
        mut self,
        targets: impl Iterator<Item = PathBuf>,
    ) -> Result<Self> {
        let copy: Vec<_> = self
            .entries
            .iter()
            .map(|e| e.relative.to_string_lossy().into_owned())
            .collect();
        for target in targets {
            if let Some(proof) = retiring_copy_fingerprint(&self.home, &target, &copy)? {
                self.artifact_retirements.insert(target, proof);
            } else {
                bail!("copy-to-artifact retirement ownership is unverified");
            }
        }
        Ok(self)
    }
    pub fn changes(&self) -> Result<Vec<CopyChange>> {
        self.inspect(&state::State::load(&self.home)?)
    }

    fn inspect(&self, state: &state::State) -> Result<Vec<CopyChange>> {
        for (target, expected) in &self.artifact_retirements {
            if fingerprint(&self.home.join(target), false)?.as_ref() != Some(expected) {
                bail!("copy-to-artifact handoff changed after preview");
            }
        }
        let mut changes = Vec::new();
        for (relative, previous) in self.retired(state) {
            if self.owned_fingerprint(relative)?.as_ref() == Some(previous) {
                changes.push(CopyChange {
                    path: relative.clone(),
                    before: Some(previous.clone()),
                    after: None,
                });
            }
        }
        let mut retiring_links = self
            .live
            .as_ref()
            .map(|live| live.removable_links(&self.home, state))
            .transpose()?
            .unwrap_or_default();
        retiring_links.extend(verify_artifact_handoffs(
            &self.home,
            &self.artifact_handoffs,
        )?);
        for entry in &self.entries {
            state.validate_destination(&entry.destination)?;
            live::validate_parents(&self.home, &entry.relative, &retiring_links)?;
            let after = fingerprint(&entry.source, true)?.context("copy source disappeared")?;
            let before = if retiring_links
                .iter()
                .any(|path| entry.relative != *path && entry.relative.starts_with(path))
            {
                None
            } else {
                fingerprint(&entry.destination, false)?
            };
            if before.as_ref() != Some(&after) {
                changes.push(CopyChange {
                    path: entry.relative.display().to_string(),
                    before,
                    after: Some(after),
                });
            }
        }
        if let Some(live) = &self.live {
            changes.extend(live.inspect(&self.home, state, self)?);
        }
        Ok(changes)
    }

    fn retired<'a>(&self, state: &'a state::State) -> Vec<(&'a String, &'a String)> {
        state
            .copies()
            .iter()
            .filter(|(relative, _)| {
                !self
                    .entries
                    .iter()
                    .any(|entry| entry.relative == Path::new(relative))
            })
            .collect()
    }

    fn owned_fingerprint(&self, relative: &str) -> Result<Option<String>> {
        validate_unowned_parents(&self.home, Path::new(relative))?;
        fingerprint(&self.home.join(relative), false)
    }

    pub fn record_current(&self) -> Result<()> {
        let mut state = state::State::load(&self.home)?;
        if !self.inspect(&state)?.is_empty() {
            bail!("home copy changed after preview; run plan/apply again");
        }
        let retired: Vec<_> = self
            .retired(&state)
            .into_iter()
            .map(|(relative, _)| relative.clone())
            .collect();
        for relative in retired {
            state.forget(&relative)?;
        }
        for entry in &self.entries {
            self.record_result(&mut state, entry)?;
        }
        if let Some(live) = &self.live {
            live.record_current(&self.home, &mut state)?;
        }
        Ok(())
    }

    fn record_result(&self, state: &mut state::State, entry: &CopyEntry) -> Result<()> {
        validate_unowned_parents(&self.home, &entry.relative)?;
        let after = fingerprint(&entry.source, true)?.context("copy source disappeared")?;
        if fingerprint(&entry.destination, false)?.as_ref() != Some(&after) {
            bail!(
                "copy result changed before recording: {}",
                entry.relative.display()
            );
        }
        state.record(&entry.relative, after)
    }

    pub fn apply(&self) -> Result<()> {
        let mut state = state::State::load(&self.home)?;
        self.inspect(&state)?;
        let retired: Vec<_> = self
            .retired(&state)
            .into_iter()
            .map(|(relative, previous)| (relative.clone(), previous.clone()))
            .collect();
        let mut pruning = self
            .live
            .as_ref()
            .map(|live| live.prunable_directories(&self.home, &state, self))
            .transpose()?
            .unwrap_or_default();
        for target in self.artifact_retirements.keys() {
            collect_artifact_pruning(&self.home.join(target), &mut pruning, &self.home)?;
        }
        for (relative, previous) in &retired {
            if self.owned_fingerprint(relative)?.as_ref() == Some(previous) {
                remove_entry(&self.home.join(relative))?;
            }
        }
        if let Some(live) = &self.live {
            live.retire(&self.home, &state)?;
        }
        for relative in pruning {
            validate_unowned_parents(&self.home, &relative)?;
            match fs::remove_dir(self.home.join(relative)) {
                Ok(()) => (),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error).context("cannot retire home directory"),
            }
        }
        for (relative, _) in retired {
            state.forget(&relative)?;
        }
        if let Some(live) = &self.live {
            live.forget_retired(&mut state)?;
        }
        for entry in &self.entries {
            validate_unowned_parents(&self.home, &entry.relative)?;
            sync_entry(&entry.source, &entry.destination).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    entry.relative.display(),
                    entry.destination.display()
                )
            })?;
            self.record_result(&mut state, entry)?;
        }
        if let Some(live) = &self.live {
            live.apply(&self.home, &mut state)?;
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

pub(crate) fn fingerprint(path: &Path, source: bool) -> Result<Option<String>> {
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
    plan_with_live(repo_root, home, paths, None, BTreeMap::new())
}

fn plan_with_live(
    repo_root: &Path,
    home: &Path,
    paths: &[String],
    live: Option<live::Plan>,
    artifact_handoffs: BTreeMap<PathBuf, PathBuf>,
) -> Result<CopyPlan> {
    validate_paths(paths)?;
    let mut retiring_links = live
        .as_ref()
        .map(|live| live.removable_links(home, &state::State::load(home)?))
        .transpose()?
        .unwrap_or_default();
    retiring_links.extend(verify_artifact_handoffs(home, &artifact_handoffs)?);
    let source_root = repo_root.join("home");

    let mut entries = Vec::with_capacity(paths.len());
    for relative in paths {
        let relative_path = PathBuf::from(&relative);
        live::validate_parents(home, &relative_path, &retiring_links)?;
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
        live,
        artifact_handoffs,
        artifact_retirements: BTreeMap::new(),
    })
}

pub fn plan_live(
    repo_root: &Path,
    directory: &Path,
    home: &Path,
    paths: &[String],
) -> Result<CopyPlan> {
    plan_with_live(
        repo_root,
        home,
        paths,
        Some(live::Plan::new(repo_root, directory, paths)?),
        BTreeMap::new(),
    )
}

// A captured artifact Plan supplies exact retiring links, never arbitrary parents.
// Every preview verifies their current source; apply uses strict parents after
// the artifact Plan has retired them under the same common lock.
pub(crate) fn plan_live_with_artifact_handoffs(
    repo_root: &Path,
    directory: &Path,
    home: &Path,
    paths: &[String],
    handoffs: BTreeMap<PathBuf, PathBuf>,
) -> Result<CopyPlan> {
    plan_with_live(
        repo_root,
        home,
        paths,
        Some(live::Plan::new(repo_root, directory, paths)?),
        handoffs,
    )
}
fn verify_artifact_handoffs(
    home: &Path,
    handoffs: &BTreeMap<PathBuf, PathBuf>,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (path, source) in handoffs {
        validate_relative_path(path.to_str().context("invalid artifact handoff")?)?;
        validate_unowned_parents(home, path)?;
        match fs::symlink_metadata(home.join(path)) {
            Ok(m) if m.file_type().is_symlink() && fs::read_link(home.join(path))? == *source => {
                paths.push(path.clone())
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            _ => bail!("artifact-to-copy handoff changed after preview"),
        }
    }
    Ok(paths)
}
pub(crate) fn retiring_copy_fingerprint(
    home: &Path,
    target: &Path,
    copy: &[String],
) -> Result<Option<String>> {
    if copy
        .iter()
        .any(|p| target.starts_with(p) || Path::new(p).starts_with(target))
    {
        return Ok(None);
    }
    if fingerprint(&home.join(target), false)?.is_none() {
        return Ok(None);
    }
    let state = state::State::load(home)?;
    let retired: Vec<_> = state
        .copies()
        .iter()
        .filter(|(p, _)| !copy.iter().any(|c| c == *p))
        .collect();
    if !retired
        .iter()
        .any(|(p, _)| target.starts_with(p) || Path::new(p).starts_with(target))
    {
        return Ok(None);
    }
    for (path, recorded) in &retired {
        if target.starts_with(path) || Path::new(path).starts_with(target) {
            validate_unowned_parents(home, Path::new(path))?;
            if fingerprint(&home.join(path), false)?.is_some_and(|current| &current != *recorded) {
                bail!("retiring copy was modified; artifact deployment refused");
            }
        }
    }
    fn covered(home: &Path, path: &Path, retired: &[(&String, &String)]) -> Result<bool> {
        if retired.iter().any(|(p, _)| path.starts_with(p)) {
            return Ok(true);
        }
        if !retired.iter().any(|(p, _)| Path::new(p).starts_with(path)) {
            return Ok(false);
        }
        let metadata = fs::symlink_metadata(home.join(path))?;
        if !metadata.is_dir() {
            return Ok(false);
        }
        for entry in read_entries(&home.join(path))? {
            if !covered(home, &path.join(entry.file_name()), retired)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
    validate_unowned_parents(home, target)?;
    if !covered(home, target, &retired)? {
        bail!("unmanaged content blocks copy-to-artifact handoff");
    }
    fingerprint(&home.join(target), false)
}
fn collect_artifact_pruning(path: &Path, paths: &mut Vec<PathBuf>, home: &Path) -> Result<()> {
    if fs::symlink_metadata(path).is_ok_and(|m| m.is_dir()) {
        for entry in read_entries(path)? {
            collect_artifact_pruning(&entry.path(), paths, home)?;
        }
        let relative = path.strip_prefix(home)?.to_owned();
        if !paths.contains(&relative) {
            paths.push(relative);
        }
    }
    Ok(())
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
    if value
        .split('/')
        .any(|part| part.is_empty() || part == "." || part == "..")
    {
        bail!("dotfiles.toml: invalid copy entry: {value}");
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
            assert_ne!(changes[0].before, changes[0].after);
            plan.apply().unwrap();
            assert!(plan.changes().unwrap().is_empty(), "{change}");
        }
    }

    #[test]
    fn removal_preview_does_not_change_files_or_recorded_results() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        fs::create_dir_all(repo.join("home")).unwrap();
        fs::write(repo.join("home/managed"), "owned").unwrap();
        plan(&repo, &home, &["managed".into()])
            .unwrap()
            .apply()
            .unwrap();
        let state = home.join(".local/state/dotfiles/home.json");
        let recorded = fs::read(&state).unwrap();
        let plan = plan(&repo, &home, &[]).unwrap();
        let changes = plan.changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, "managed");
        assert!(changes[0].before.is_some());
        assert!(changes[0].after.is_none());
        assert_eq!(fs::read_to_string(home.join("managed")).unwrap(), "owned");
        assert_eq!(fs::read(&state).unwrap(), recorded);
        plan.apply().unwrap();
        assert!(!home.join("managed").exists());
        assert!(plan.changes().unwrap().is_empty());
    }

    #[test]
    fn metadata_recovery_rejects_home_changes_after_an_unchanged_preview() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home");
        fs::create_dir_all(repo.join("home")).unwrap();
        fs::create_dir_all(&home).unwrap();
        fs::write(repo.join("home/managed"), "owned").unwrap();
        fs::copy(repo.join("home/managed"), home.join("managed")).unwrap();
        let plan = plan(&repo, &home, &["managed".into()]).unwrap();
        assert!(plan.changes().unwrap().is_empty());
        fs::write(home.join("managed"), "changed after preview").unwrap();
        assert!(plan.record_current().is_err());
        assert_eq!(
            fs::read_to_string(home.join("managed")).unwrap(),
            "changed after preview"
        );
        assert!(!home.join(".local/state/dotfiles/home.json").exists());
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
