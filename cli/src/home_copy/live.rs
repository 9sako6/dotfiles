use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io;
use std::os::unix::fs::symlink;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

use super::{fingerprint, read_entries, state::State, validate_unowned_parents, CopyChange};

const ROOTS: &[(&str, bool)] = &[
    (".config", true),
    (".gitconfig", false),
    (".gitignore_global", false),
    (".zsh.d", true),
    (".zshenv", false),
    (".zshrc", false),
    ("apm.lock.yaml", false),
    ("apm.yml", false),
    ("mybin", true),
];

pub(crate) fn paths(source: &Path, copy: &[String]) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for (root, recursive) in ROOTS {
        let root = Path::new(root);
        if *recursive {
            collect(&source.join("home"), root, copy, &mut paths)?;
        } else if !copy.iter().any(|copy| overlaps(root, Path::new(copy))) {
            paths.push(root.to_owned());
        }
    }
    paths.sort();
    Ok(paths)
}

pub(crate) fn inventory_paths(source: &Path) -> Result<Vec<PathBuf>> {
    let text = match fs::read_to_string(source.join("dotfiles.toml")) {
        Ok(text) => text,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error).context("cannot read live home declarations"),
    };
    let configuration: toml::Value =
        toml::from_str(&text).map_err(|_| anyhow::anyhow!("invalid live home declarations"))?;
    let copy: Vec<String> = configuration
        .get("copy")
        .cloned()
        .map(toml::Value::try_into)
        .transpose()
        .map_err(|_| anyhow::anyhow!("invalid home copy declarations"))?
        .unwrap_or_default();
    paths(source, &copy)
}

fn overlaps(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

fn collect(
    source: &Path,
    relative: &Path,
    copy: &[String],
    paths: &mut Vec<PathBuf>,
) -> Result<()> {
    if copy.iter().any(|copy| relative.starts_with(copy)) {
        return Ok(());
    }
    let path = source.join(relative);
    let metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("cannot inspect live home source"),
    };
    if metadata.is_dir() {
        let mut entries = read_entries(&path)?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            collect(source, &relative.join(entry.file_name()), copy, paths)?;
        }
    } else if !copy.iter().any(|copy| overlaps(relative, Path::new(copy))) {
        paths.push(relative.to_owned());
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub(super) struct Plan {
    entries: BTreeMap<PathBuf, PathBuf>,
}

impl Plan {
    pub(super) fn new(source: &Path, directory: &Path, copy: &[String]) -> Result<Self> {
        if !directory.is_absolute() {
            bail!("live home checkout must be absolute");
        }
        Ok(Self {
            entries: paths(source, copy)?
                .into_iter()
                .map(|relative| {
                    let target = directory.join("home").join(&relative);
                    (relative, target)
                })
                .collect(),
        })
    }

    fn retired<'a>(&self, state: &'a State) -> Vec<(&'a String, &'a PathBuf)> {
        state
            .links()
            .iter()
            .filter(|(relative, _)| !self.entries.contains_key(Path::new(relative)))
            .collect()
    }

    fn removable(
        &self,
        home: &Path,
        state: &State,
        copies: &super::CopyPlan,
    ) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for (relative, previous) in copies.retired(state) {
            // A prior attempt may have removed this owned entry before pruning
            // its empty parents. Keep that retirement eligible for retry, but
            // never authorize a replacement that differs from the recorded copy.
            let current = copies.owned_fingerprint(relative)?;
            if current.is_none() || current.as_ref() == Some(previous) {
                paths.push(PathBuf::from(relative));
            }
        }
        paths.extend(self.removable_links(home, state)?);
        Ok(paths)
    }

    pub(super) fn removable_links(&self, home: &Path, state: &State) -> Result<Vec<PathBuf>> {
        let mut paths = Vec::new();
        for (relative, previous) in self.retired(state) {
            if validate_unowned_parents(home, Path::new(relative)).is_ok()
                && (fs::read_link(home.join(relative)).ok().as_ref() == Some(previous)
                    || fs::symlink_metadata(home.join(relative))
                        .is_err_and(|error| error.kind() == io::ErrorKind::NotFound))
                && active_declaration(home, Path::new(relative))?.is_none()
            {
                paths.push(PathBuf::from(relative));
            }
        }
        Ok(paths)
    }

    pub(super) fn prunable_directories(
        &self,
        home: &Path,
        state: &State,
        copies: &super::CopyPlan,
    ) -> Result<Vec<PathBuf>> {
        let removable = self.removable(home, state, copies)?;
        let mut directories = Vec::new();
        for relative in self.entries.keys() {
            if removable
                .iter()
                .any(|path| path != relative && path.starts_with(relative))
                && fs::symlink_metadata(home.join(relative)).is_ok_and(|metadata| metadata.is_dir())
                && removable_tree(home, relative, &removable)?
            {
                collect_prunable_directories(home, relative, &removable, &mut directories)?;
            }
        }
        Ok(directories)
    }

    pub(super) fn inspect(
        &self,
        home: &Path,
        state: &State,
        copies: &super::CopyPlan,
    ) -> Result<Vec<CopyChange>> {
        let removable = self.removable(home, state, copies)?;
        let mut changes = Vec::new();
        for (relative, previous) in self.retired(state) {
            if removable.contains(&PathBuf::from(relative)) {
                changes.push(CopyChange {
                    path: relative.clone(),
                    before: Some(format!("link -> {}", previous.display())),
                    after: None,
                });
            }
        }
        for (relative, target) in &self.entries {
            state.validate_destination(&home.join(relative))?;
            validate_parents(home, relative, &removable)?;
            self.validate_entry(home, state, relative, target, &removable)?;
            if fs::read_link(home.join(relative)).ok().as_deref() != Some(target) {
                changes.push(CopyChange {
                    path: relative.display().to_string(),
                    before: if removable
                        .iter()
                        .any(|path| relative != path && relative.starts_with(path))
                    {
                        None
                    } else {
                        fingerprint(&home.join(relative), false)?
                    },
                    after: Some(format!("link -> {}", target.display())),
                });
            }
        }
        Ok(changes)
    }

    fn validate_entry(
        &self,
        home: &Path,
        state: &State,
        relative: &Path,
        target: &Path,
        removable: &[PathBuf],
    ) -> Result<()> {
        let declaration = active_declaration(home, relative)?;
        if let Some(declaration) = &declaration {
            if declaration.relative != relative || !follows_to(&declaration.path, target)? {
                bail!(
                    "live home target is owned by Home Manager: {}",
                    relative.display()
                );
            }
        }
        let destination = home.join(relative);
        let metadata = match fs::symlink_metadata(&destination) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error)
                if error.kind() == io::ErrorKind::NotADirectory
                    && removable.iter().any(|path| relative.starts_with(path)) =>
            {
                return Ok(())
            }
            Err(error) => return Err(error).context("cannot inspect live home target"),
        };
        if removable.iter().any(|path| relative.starts_with(path)) {
            return Ok(());
        }
        if metadata.file_type().is_symlink() {
            let actual = fs::read_link(&destination)?;
            if actual == target
                || state.links().get(&relative.display().to_string()) == Some(&actual)
                || declaration.as_ref().is_some_and(|declaration| {
                    follows_to(&destination, &declaration.path).unwrap_or(false)
                })
            {
                return Ok(());
            }
        } else if metadata.is_dir()
            && removable.iter().any(|path| path.starts_with(relative))
            && removable_tree(home, relative, removable)?
        {
            return Ok(());
        }
        bail!(
            "live home target conflicts with an unmanaged or changed entry: {}",
            relative.display()
        )
    }

    pub(super) fn retire(&self, home: &Path, state: &State) -> Result<()> {
        for (relative, target) in self.retired(state) {
            let relative_path = Path::new(relative);
            if validate_unowned_parents(home, relative_path).is_ok()
                && fs::read_link(home.join(relative)).ok().as_ref() == Some(target)
                && active_declaration(home, relative_path)?.is_none()
            {
                fs::remove_file(home.join(relative))?;
            }
        }
        Ok(())
    }

    pub(super) fn apply(&self, home: &Path, state: &mut State) -> Result<()> {
        for (relative, target) in &self.entries {
            validate_unowned_parents(home, relative)?;
            self.validate_entry(home, state, relative, target, &[])?;
            let destination = home.join(relative);
            if fs::read_link(&destination).ok().as_ref() != Some(target) {
                let parent = destination
                    .parent()
                    .context("live home target has no parent")?;
                fs::create_dir_all(parent)?;
                let temporary = tempfile::Builder::new()
                    .prefix(".dotfiles-link-")
                    .tempdir_in(parent)?;
                let link = temporary.path().join("link");
                symlink(target, &link)?;
                self.validate_entry(home, state, relative, target, &[])?;
                fs::rename(link, &destination).context("cannot install live home link")?;
            }
            self.record_entry(home, state, relative, target)?;
        }
        Ok(())
    }

    fn record_entry(
        &self,
        home: &Path,
        state: &mut State,
        relative: &Path,
        target: &Path,
    ) -> Result<()> {
        validate_unowned_parents(home, relative)?;
        self.validate_entry(home, state, relative, target, &[])?;
        if fs::read_link(home.join(relative)).ok().as_deref() != Some(target) {
            bail!(
                "live home link changed before recording: {}",
                relative.display()
            );
        }
        state.record_link(relative, target)
    }

    pub(super) fn forget_retired(&self, state: &mut State) -> Result<()> {
        let retired: Vec<_> = self
            .retired(state)
            .into_iter()
            .map(|(relative, _)| relative.clone())
            .collect();
        for relative in retired {
            state.forget_link(&relative)?;
        }
        Ok(())
    }

    pub(super) fn record_current(&self, home: &Path, state: &mut State) -> Result<()> {
        self.forget_retired(state)?;
        for (relative, target) in &self.entries {
            self.record_entry(home, state, relative, target)?;
        }
        Ok(())
    }
}

pub(super) fn validate_parents(home: &Path, relative: &Path, removable: &[PathBuf]) -> Result<()> {
    if let Some(parent) = relative.parent() {
        for ancestor in parent
            .ancestors()
            .filter(|path| !path.as_os_str().is_empty())
        {
            if removable.iter().any(|path| ancestor.starts_with(path)) {
                return Ok(());
            }
        }
    }
    validate_unowned_parents(home, relative)
}

// Preview verified this tree contains only retiring entries and directories.
// Keep the exact postorder list so apply only removes directories that are empty.
fn collect_prunable_directories(
    home: &Path,
    relative: &Path,
    removable: &[PathBuf],
    directories: &mut Vec<PathBuf>,
) -> Result<()> {
    if removable.iter().any(|path| relative.starts_with(path)) {
        return Ok(());
    }
    for entry in read_entries(&home.join(relative))? {
        collect_prunable_directories(
            home,
            &relative.join(entry.file_name()),
            removable,
            directories,
        )?;
    }
    directories.push(relative.to_owned());
    Ok(())
}

fn removable_tree(home: &Path, relative: &Path, removable: &[PathBuf]) -> Result<bool> {
    if removable.iter().any(|path| relative.starts_with(path)) {
        return Ok(true);
    }
    if !fs::symlink_metadata(home.join(relative))?.is_dir() {
        return Ok(false);
    }
    for entry in read_entries(&home.join(relative))? {
        if !removable_tree(home, &relative.join(entry.file_name()), removable)? {
            return Ok(false);
        }
    }
    Ok(true)
}

struct Declaration {
    path: PathBuf,
    relative: PathBuf,
}

fn active_declaration(home: &Path, relative: &Path) -> Result<Option<Declaration>> {
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    let current = state.join("home-manager/gcroots/current-home");
    match fs::symlink_metadata(&current) {
        Ok(_) => (),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("cannot inspect active Home Manager generation"),
    }
    let root = current
        .join("home-files")
        .canonicalize()
        .context("cannot resolve active Home Manager declarations")?;
    let mut path = root;
    let mut owned = PathBuf::new();
    for component in relative.components() {
        path.push(component);
        owned.push(component);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() && owned != relative => (),
            Ok(_) => {
                return Ok(Some(Declaration {
                    path,
                    relative: owned,
                }))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => {
                return Err(error).context("cannot inspect active Home Manager declaration")
            }
        }
    }
    Ok(None)
}

fn follows_to(path: &Path, expected: &Path) -> Result<bool> {
    let mut current = normalize(path);
    for _ in 0..64 {
        if current == expected {
            return Ok(true);
        }
        if let Some(parent) = current.parent() {
            if let Ok(parent) = parent.canonicalize() {
                current = parent.join(current.file_name().context("invalid home link")?);
            }
        }
        if current == expected {
            return Ok(true);
        }
        match fs::read_link(&current) {
            Ok(target) => {
                current = normalize(&if target.is_absolute() {
                    target
                } else {
                    current.parent().context("invalid home link")?.join(target)
                })
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::NotFound
                        | io::ErrorKind::InvalidInput
                        | io::ErrorKind::NotADirectory
                ) =>
            {
                return Ok(false)
            }
            Err(error) => return Err(error).context("cannot inspect Home Manager link target"),
        }
    }
    Ok(false)
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => (),
            _ => result.push(component),
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::plan_live;
    use super::*;

    struct Fixture {
        root: tempfile::TempDir,
        source: PathBuf,
        directory: PathBuf,
        home: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let source = root.path().join("snapshot");
            let directory = root.path().join("checkout");
            let home = root.path().join("home");
            for path in [&source, &directory, &home] {
                fs::create_dir_all(path.join("home")).unwrap();
            }
            Self {
                root,
                source,
                directory,
                home,
            }
        }

        fn source(&self, relative: &str, contents: &str) {
            for root in [&self.source, &self.directory] {
                let path = root.join("home").join(relative);
                fs::create_dir_all(path.parent().unwrap()).unwrap();
                fs::write(path, contents).unwrap();
            }
        }

        fn plan(&self, copy: &[&str]) -> super::super::CopyPlan {
            plan_live(
                &self.source,
                &self.directory,
                &self.home,
                &copy.iter().map(|path| path.to_string()).collect::<Vec<_>>(),
            )
            .unwrap()
        }

        fn declaration(&self, relative: &str, target: &Path) -> PathBuf {
            let generation = self.root.path().join("generation");
            let files = generation.join("home-files");
            let path = files.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            symlink(target, &path).unwrap();
            let current = self
                .home
                .join(".local/state/home-manager/gcroots/current-home");
            fs::create_dir_all(current.parent().unwrap()).unwrap();
            if !current.is_symlink() {
                symlink(generation, current).unwrap();
            }
            path
        }
    }

    #[test]
    fn live_leaves_use_frozen_topology_live_contents_and_preserve_runtime_siblings() {
        let fixture = Fixture::new();
        fixture.source(".config/tool/config", "original");
        fixture.source(".zsh.d/init", "original");
        fixture.source("mybin/tool", "original");
        fs::create_dir_all(fixture.home.join(".zsh.d")).unwrap();
        fs::write(fixture.home.join(".zsh.d/runtime"), "keep").unwrap();
        fs::write(
            fixture.directory.join("home/.config/tool/late"),
            "not frozen",
        )
        .unwrap();
        let plan = fixture.plan(&[]);
        assert!(!plan.changes().unwrap().is_empty());
        plan.apply().unwrap();
        assert!(plan.changes().unwrap().is_empty());
        for path in [".config", ".zsh.d", "mybin"] {
            assert!(!fixture.home.join(path).is_symlink());
        }
        assert_eq!(
            fs::read_link(fixture.home.join(".config/tool/config")).unwrap(),
            fixture.directory.join("home/.config/tool/config")
        );
        assert!(!fixture.home.join(".config/tool/late").exists());
        fs::write(
            fixture.directory.join("home/.config/tool/config"),
            "live edit",
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/tool/config")).unwrap(),
            "live edit"
        );
        assert!(plan.changes().unwrap().is_empty());
        assert_eq!(
            fs::read_to_string(fixture.home.join(".zsh.d/runtime")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn copy_owns_overlapping_live_files_and_directories() {
        let fixture = Fixture::new();
        fixture.source(".config/owned/a", "snapshot");
        fixture.source(".config/live", "live");
        fixture.source(".zshrc", "snapshot");
        let plan = fixture.plan(&[".config/owned", ".zshrc"]);
        plan.apply().unwrap();
        assert!(!fixture.home.join(".config/owned/a").is_symlink());
        assert!(!fixture.home.join(".zshrc").is_symlink());
        assert!(fixture.home.join(".config/live").is_symlink());
        fs::write(fixture.directory.join("home/.config/owned/a"), "not copied").unwrap();
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/owned/a")).unwrap(),
            "snapshot"
        );
        assert!(plan.changes().unwrap().is_empty());
    }

    #[test]
    fn active_home_manager_adoption_requires_its_declaration_and_exact_live_path() {
        let fixture = Fixture::new();
        fixture.source(".zshrc", "live");
        let wrapper = fixture.root.path().join("store-wrapper");
        symlink(fixture.directory.join("home/.zshrc"), &wrapper).unwrap();
        let declaration = fixture.declaration(".zshrc", &wrapper);
        symlink(declaration, fixture.home.join(".zshrc")).unwrap();
        let plan = fixture.plan(&[]);
        plan.apply().unwrap();
        assert_eq!(
            fs::read_link(fixture.home.join(".zshrc")).unwrap(),
            fixture.directory.join("home/.zshrc")
        );
        assert!(plan.changes().unwrap().is_empty());
    }

    #[test]
    fn active_declarations_block_absent_targets_and_unverified_links() {
        for present in [false, true] {
            let fixture = Fixture::new();
            fixture.source(".zshrc", "same contents");
            let foreign = fixture.root.path().join("other-source");
            fs::write(&foreign, "same contents").unwrap();
            let declaration = fixture.declaration(".zshrc", &foreign);
            if present {
                symlink(declaration, fixture.home.join(".zshrc")).unwrap();
            }
            let plan = fixture.plan(&[]);
            assert!(plan
                .changes()
                .err()
                .unwrap()
                .to_string()
                .contains("owned by Home Manager"));
            assert!(plan.apply().is_err());
            assert_eq!(fixture.home.join(".zshrc").is_symlink(), present);
        }
        let fixture = Fixture::new();
        fixture.source(".zshrc", "live");
        let wrapper = fixture.root.path().join("unregistered-wrapper");
        symlink(fixture.directory.join("home/.zshrc"), &wrapper).unwrap();
        symlink(wrapper, fixture.home.join(".zshrc")).unwrap();
        assert!(fixture.plan(&[]).apply().is_err());
    }

    #[test]
    fn active_directory_owner_blocks_a_public_descendant() {
        let fixture = Fixture::new();
        fixture.source(".config/tool/config", "live");
        let foreign = fixture.root.path().join("private-config");
        fs::create_dir_all(&foreign).unwrap();
        fixture.declaration(".config/tool", &foreign);
        assert!(fixture
            .plan(&[])
            .changes()
            .err()
            .unwrap()
            .to_string()
            .contains("owned by Home Manager"));
        assert!(!fixture.home.join(".config/tool/config").exists());
    }

    #[test]
    fn unmanaged_regular_files_and_changed_recorded_links_are_preserved() {
        let fixture = Fixture::new();
        fixture.source(".zshrc", "live");
        fs::write(fixture.home.join(".zshrc"), "unmanaged").unwrap();
        assert!(fixture.plan(&[]).apply().is_err());
        assert_eq!(
            fs::read_to_string(fixture.home.join(".zshrc")).unwrap(),
            "unmanaged"
        );
        fs::remove_file(fixture.home.join(".zshrc")).unwrap();
        fixture.plan(&[]).apply().unwrap();
        fs::remove_file(fixture.home.join(".zshrc")).unwrap();
        symlink("/foreign", fixture.home.join(".zshrc")).unwrap();
        assert!(fixture.plan(&[]).apply().is_err());
        assert_eq!(
            fs::read_link(fixture.home.join(".zshrc")).unwrap(),
            Path::new("/foreign")
        );
    }

    #[test]
    fn retirement_removes_only_recorded_unchanged_links() {
        let fixture = Fixture::new();
        fixture.source(".config/a", "a");
        fixture.source(".config/b", "b");
        fixture.plan(&[]).apply().unwrap();
        fs::remove_file(fixture.home.join(".config/b")).unwrap();
        symlink("/foreign", fixture.home.join(".config/b")).unwrap();
        fs::remove_dir_all(fixture.source.join("home/.config")).unwrap();
        let plan = fixture.plan(&[]);
        let changes = plan.changes().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].path, ".config/a");
        plan.apply().unwrap();
        assert!(!fixture.home.join(".config/a").is_symlink());
        assert!(fixture.home.join(".config/b").is_symlink());
        assert!(plan.changes().unwrap().is_empty());
    }

    #[test]
    fn missing_state_recovers_matching_links_without_claiming_old_paths() {
        let fixture = Fixture::new();
        fixture.source(".config/old", "old");
        fixture.plan(&[]).apply().unwrap();
        fs::remove_file(fixture.home.join(".local/state/dotfiles/home.json")).unwrap();
        fs::remove_file(fixture.source.join("home/.config/old")).unwrap();
        let plan = fixture.plan(&[]);
        assert!(plan.changes().unwrap().is_empty());
        plan.record_current().unwrap();
        assert!(fixture.home.join(".config/old").is_symlink());
    }

    #[test]
    fn recorded_live_leaf_can_become_a_directory_and_back_without_touching_siblings() {
        let fixture = Fixture::new();
        fixture.source(".config/tool", "file");
        fixture.plan(&[]).apply().unwrap();
        fs::remove_file(fixture.source.join("home/.config/tool")).unwrap();
        fs::remove_file(fixture.directory.join("home/.config/tool")).unwrap();
        fixture.source(".config/tool/config", "nested");
        let plan = fixture.plan(&[]);
        assert!(!plan.changes().unwrap().is_empty());
        plan.apply().unwrap();
        assert!(!fixture.home.join(".config/tool").is_symlink());
        fs::remove_dir_all(fixture.source.join("home/.config/tool")).unwrap();
        fs::remove_dir_all(fixture.directory.join("home/.config/tool")).unwrap();
        fixture.source(".config/tool", "file again");
        fixture.plan(&[]).apply().unwrap();
        assert!(fixture.home.join(".config/tool").is_symlink());
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/tool")).unwrap(),
            "file again"
        );
    }

    #[test]
    fn directory_to_leaf_conflict_preserves_unmanaged_descendants() {
        let fixture = Fixture::new();
        fixture.source(".config/tool/config", "nested");
        fixture.plan(&[]).apply().unwrap();
        fs::write(fixture.home.join(".config/tool/runtime"), "keep").unwrap();
        fs::remove_dir_all(fixture.source.join("home/.config/tool")).unwrap();
        fs::remove_dir_all(fixture.directory.join("home/.config/tool")).unwrap();
        fixture.source(".config/tool", "file");
        assert!(fixture.plan(&[]).apply().is_err());
        assert!(fixture.home.join(".config/tool/config").is_symlink());
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/tool/runtime")).unwrap(),
            "keep"
        );
    }

    #[test]
    fn recorded_live_leaf_can_become_a_copy_descendant() {
        let fixture = Fixture::new();
        fixture.source(".config/tool", "old leaf");
        fixture.plan(&[]).apply().unwrap();
        for root in [&fixture.source, &fixture.directory] {
            fs::remove_file(root.join("home/.config/tool")).unwrap();
        }
        fixture.source(".config/tool/config", "copied");
        fs::write(fixture.home.join(".config/runtime"), "keep").unwrap();
        // Copy-only plans cannot retire links and must retain their strict boundary.
        assert!(super::super::plan(
            &fixture.source,
            &fixture.home,
            &[".config/tool/config".into()],
        )
        .is_err());
        let plan = fixture.plan(&[".config/tool/config"]);
        let changes = plan.changes().unwrap();
        let addition = changes
            .iter()
            .find(|change| change.path == ".config/tool/config")
            .unwrap();
        assert!(addition.before.is_none());
        plan.apply().unwrap();
        assert!(!fixture.home.join(".config/tool").is_symlink());
        assert!(!fixture.home.join(".config/tool/config").is_symlink());
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/tool/config")).unwrap(),
            "copied"
        );
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/runtime")).unwrap(),
            "keep"
        );
        assert!(plan.changes().unwrap().is_empty());
        plan.apply().unwrap();
    }

    #[test]
    fn copy_descendant_rejects_unverified_live_parents_and_rechecks_preview() {
        for obstruction in ["changed", "unrecorded", "home-manager"] {
            let fixture = Fixture::new();
            fixture.source(".config/tool", "old leaf");
            fixture.plan(&[]).apply().unwrap();
            for root in [&fixture.source, &fixture.directory] {
                fs::remove_file(root.join("home/.config/tool")).unwrap();
            }
            fixture.source(".config/tool/config", "copied");
            let plan = fixture.plan(&[".config/tool/config"]);
            plan.changes().unwrap();
            if obstruction == "unrecorded" {
                fs::remove_file(fixture.home.join(".local/state/dotfiles/home.json")).unwrap();
            } else if obstruction == "home-manager" {
                fixture.declaration(".config/tool", &fixture.directory.join("home/.config/tool"));
            } else {
                fs::remove_file(fixture.home.join(".config/tool")).unwrap();
                symlink(
                    fixture.directory.join("home/.config"),
                    fixture.home.join(".config/tool"),
                )
                .unwrap();
            }
            assert!(plan.apply().is_err(), "{obstruction}");
            assert!(fixture.home.join(".config/tool").is_symlink());
            assert_eq!(
                fs::read_to_string(fixture.directory.join("home/.config/tool/config")).unwrap(),
                "copied"
            );
            assert!(plan_live(
                &fixture.source,
                &fixture.directory,
                &fixture.home,
                &[".config/tool/config".into()]
            )
            .is_err());
        }
    }

    #[test]
    fn retired_copy_descendants_can_become_a_live_ancestor() {
        let fixture = Fixture::new();
        fixture.source(".config/tool/deep/config", "copied");
        fixture.source(".config/tool/other", "copied");
        fixture
            .plan(&[".config/tool/deep/config", ".config/tool/other"])
            .apply()
            .unwrap();
        fs::create_dir_all(fixture.home.join(".config/tool/empty/nested")).unwrap();
        fs::write(fixture.home.join(".config/runtime"), "keep").unwrap();
        for root in [&fixture.source, &fixture.directory] {
            fs::remove_dir_all(root.join("home/.config/tool")).unwrap();
        }
        fixture.source(".config/tool", "live leaf");
        let plan = fixture.plan(&[]);
        assert!(!plan.changes().unwrap().is_empty());
        plan.apply().unwrap();
        assert_eq!(
            fs::read_link(fixture.home.join(".config/tool")).unwrap(),
            fixture.directory.join("home/.config/tool")
        );
        assert_eq!(
            fs::read_to_string(fixture.home.join(".config/runtime")).unwrap(),
            "keep"
        );
        assert!(State::load(&fixture.home).unwrap().copies().is_empty());
        assert!(plan.changes().unwrap().is_empty());
        plan.apply().unwrap();
    }

    #[test]
    fn interrupted_live_retirement_retries_only_with_verified_empty_parents() {
        for obstruction in ["none", "changed", "home-manager", "unmanaged"] {
            let fixture = Fixture::new();
            fixture.source(".config/tool/deep/config", "live child");
            fixture.plan(&[]).apply().unwrap();
            for root in [&fixture.source, &fixture.directory] {
                fs::remove_dir_all(root.join("home/.config/tool")).unwrap();
            }
            fixture.source(".config/tool", "live leaf");
            // The old link was removed, but its record and parents remain.
            fs::remove_file(fixture.home.join(".config/tool/deep/config")).unwrap();
            match obstruction {
                "changed" => {
                    fs::write(fixture.home.join(".config/tool/deep/config"), "keep").unwrap()
                }
                "home-manager" => {
                    fixture.declaration(".config/tool/deep/config", Path::new("/foreign"));
                }
                "unmanaged" => {
                    fs::write(fixture.home.join(".config/tool/deep/runtime"), "keep").unwrap()
                }
                _ => (),
            }
            let plan = fixture.plan(&[]);
            if obstruction == "none" {
                assert!(!plan.changes().unwrap().is_empty());
                plan.apply().unwrap();
                assert_eq!(
                    fs::read_link(fixture.home.join(".config/tool")).unwrap(),
                    fixture.directory.join("home/.config/tool")
                );
                assert!(plan.changes().unwrap().is_empty());
                plan.apply().unwrap();
            } else {
                assert!(plan.changes().is_err(), "{obstruction}");
                assert!(plan.apply().is_err(), "{obstruction}");
                assert!(fixture.home.join(".config/tool/deep").is_dir());
                assert!(State::load(&fixture.home)
                    .unwrap()
                    .links()
                    .contains_key(".config/tool/deep/config"));
                if obstruction != "home-manager" {
                    let name = if obstruction == "changed" {
                        "config"
                    } else {
                        "runtime"
                    };
                    assert_eq!(
                        fs::read_to_string(fixture.home.join(".config/tool/deep").join(name))
                            .unwrap(),
                        "keep"
                    );
                }
            }
        }
    }

    #[test]
    fn interrupted_copy_retirement_retries_only_with_empty_parents() {
        for unmanaged in [false, true] {
            let fixture = Fixture::new();
            fixture.source(".config/tool/deep/config", "copied");
            fixture.plan(&[".config/tool/deep/config"]).apply().unwrap();
            for root in [&fixture.source, &fixture.directory] {
                fs::remove_dir_all(root.join("home/.config/tool")).unwrap();
            }
            fixture.source(".config/tool", "live leaf");
            // Simulate interruption after entry removal, before parent pruning
            // and forgetting the recorded successful copy.
            fs::remove_file(fixture.home.join(".config/tool/deep/config")).unwrap();
            if unmanaged {
                fs::write(fixture.home.join(".config/tool/deep/runtime"), "keep").unwrap();
            }
            let plan = fixture.plan(&[]);
            if unmanaged {
                assert!(plan.changes().is_err());
                assert!(plan.apply().is_err());
                assert_eq!(
                    fs::read_to_string(fixture.home.join(".config/tool/deep/runtime")).unwrap(),
                    "keep"
                );
                assert!(State::load(&fixture.home)
                    .unwrap()
                    .copies()
                    .contains_key(".config/tool/deep/config"));
            } else {
                assert!(!plan.changes().unwrap().is_empty());
                plan.apply().unwrap();
                assert_eq!(
                    fs::read_link(fixture.home.join(".config/tool")).unwrap(),
                    fixture.directory.join("home/.config/tool")
                );
                assert!(State::load(&fixture.home).unwrap().copies().is_empty());
                assert!(plan.changes().unwrap().is_empty());
                plan.apply().unwrap();
            }
        }
    }

    #[test]
    fn copy_to_live_ancestor_preserves_changed_copies_and_unmanaged_siblings() {
        for changed_copy in [false, true] {
            let fixture = Fixture::new();
            fixture.source(".config/tool/config", "copied");
            fixture.plan(&[".config/tool/config"]).apply().unwrap();
            for root in [&fixture.source, &fixture.directory] {
                fs::remove_dir_all(root.join("home/.config/tool")).unwrap();
            }
            fixture.source(".config/tool", "live leaf");
            let plan = fixture.plan(&[]);
            plan.changes().unwrap();
            let keep = fixture.home.join(if changed_copy {
                ".config/tool/config"
            } else {
                ".config/tool/runtime"
            });
            fs::write(&keep, "keep").unwrap();
            assert!(plan.changes().is_err());
            assert!(plan.apply().is_err());
            assert_eq!(fs::read_to_string(keep).unwrap(), "keep");
            assert!(fixture.home.join(".config/tool/config").is_file());
            assert!(State::load(&fixture.home)
                .unwrap()
                .copies()
                .contains_key(".config/tool/config"));
        }
    }

    #[test]
    fn declaration_changes_after_preview_are_rechecked_before_apply() {
        let fixture = Fixture::new();
        fixture.source(".zshrc", "live");
        let plan = fixture.plan(&[]);
        plan.changes().unwrap();
        fixture.declaration(".zshrc", Path::new("/foreign"));
        assert!(plan.apply().is_err());
        assert!(!fixture.home.join(".zshrc").is_symlink());
    }
}
