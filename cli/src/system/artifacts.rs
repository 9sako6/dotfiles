//! Frozen Nix artifacts, separately rooted and journaled from ordinary home files.
use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Artifact {
    id: String,
    kind: String,
    store_path: PathBuf,
    relative_path: PathBuf,
    home_target: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    schema_version: u32,
    artifacts: Vec<Artifact>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Evaluation {
    manifest_data: Manifest,
    manifest: PathBuf,
    output: PathBuf,
    root: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Applied {
    version: u32,
    home: PathBuf,
    links: BTreeMap<PathBuf, PathBuf>,
    #[serde(default)]
    retiring_directories: BTreeMap<PathBuf, DirectoryReceipt>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct DirectoryReceipt {
    device: u64,
    inode: u64,
}

pub(super) struct Plan {
    evaluation: Evaluation,
    nix: PathBuf,
    home: PathBuf,
    state: PathBuf,
    state_anchor: PathBuf,
    stable_state_anchor: PathBuf,
    stable_home: PathBuf,
    state_bytes: Option<Vec<u8>>,
    applied: Applied,
    desired: BTreeMap<PathBuf, PathBuf>,
    observed: BTreeMap<PathBuf, Option<String>>,
    copy_handoffs: BTreeMap<PathBuf, PathBuf>,
    retiring_copies: BTreeMap<PathBuf, String>,
    copy_pruning: BTreeMap<PathBuf, DirectoryReceipt>,
    recovered_retirements: std::collections::BTreeSet<PathBuf>,
    adopted: std::collections::BTreeSet<PathBuf>,
    changes: BTreeMap<PathBuf, char>,
    roots: PathBuf,
    rooted: bool,
    home_manager_generation: Option<PathBuf>,
}

impl Plan {
    #[cfg(test)]
    pub(super) fn empty_for_test(home: &Path) -> Self {
        Self {
            evaluation: Evaluation {
                manifest_data: Manifest {
                    schema_version: 1,
                    artifacts: vec![],
                },
                manifest: PathBuf::new(),
                output: PathBuf::new(),
                root: PathBuf::new(),
            },
            nix: PathBuf::new(),
            home: home.to_owned(),
            state: home.join(".local/state/dotfiles/artifacts.json"),
            state_anchor: home.to_owned(),
            stable_state_anchor: resolve_anchor(home).unwrap(),
            stable_home: resolve_anchor(home).unwrap(),
            state_bytes: None,
            applied: Applied {
                version: 1,
                home: home.to_owned(),
                links: BTreeMap::new(),
                retiring_directories: BTreeMap::new(),
            },
            desired: BTreeMap::new(),
            observed: BTreeMap::new(),
            copy_handoffs: BTreeMap::new(),
            retiring_copies: BTreeMap::new(),
            copy_pruning: BTreeMap::new(),
            recovered_retirements: Default::default(),
            adopted: Default::default(),
            changes: BTreeMap::new(),
            roots: home.join(".local/state/dotfiles/artifact-roots"),
            rooted: false,
            home_manager_generation: home_manager_generation(home).unwrap(),
        }
    }
    pub(super) fn capture(
        nix: &Path,
        source: &Path,
        input: &Path,
        home: &Path,
        copy: &[String],
        show_trace: bool,
    ) -> Result<Self> {
        let evaluation = super::evaluate(nix, source, input, "artifacts", show_trace)?;
        Self::from_evaluation(nix, source, home, copy, evaluation)
    }

    fn from_evaluation(
        nix: &Path,
        source: &Path,
        home: &Path,
        copy: &[String],
        evaluation: Evaluation,
    ) -> Result<Self> {
        if evaluation.manifest_data.schema_version != 1 {
            bail!("unsupported artifact manifest schema");
        }
        store_path(&evaluation.output, false)?;
        store_path(&evaluation.root, true)?;
        store_path(&evaluation.manifest, true)?;
        let configured_state = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute());
        let state_home = configured_state
            .clone()
            .unwrap_or_else(|| home.join(".local/state"));
        let state_anchor = configured_state.unwrap_or_else(|| home.to_owned());
        let stable_state_anchor = resolve_anchor(&state_anchor)?;
        let stable_home = resolve_anchor(home)?;
        let state = state_home.join("dotfiles/artifacts.json");
        safe_parents(&state_anchor, &state)?;
        let state_bytes = read_state(&state)?;
        let applied: Applied = match &state_bytes {
            Some(bytes) => {
                serde_json::from_slice(bytes).context("invalid artifact result ledger")?
            }
            None => Applied {
                version: 1,
                home: home.to_owned(),
                links: BTreeMap::new(),
                retiring_directories: BTreeMap::new(),
            },
        };
        if applied.version != 1 || applied.home != home {
            bail!("artifact ledger belongs to another home or version");
        }
        for (target, source) in &applied.links {
            relative(target)?;
            if ![
                Path::new(".local/bin/localllm"),
                Path::new("Library/Application Support/Anki2/addons21/anki-connect"),
            ]
            .contains(&target.as_path())
            {
                bail!("unknown artifact ledger target");
            }
            if !source.is_absolute() {
                bail!("invalid artifact ledger source");
            }
        }
        for path in applied.retiring_directories.keys() {
            relative(path)?;
            if ![
                Path::new(".local/bin/localllm"),
                Path::new("Library/Application Support/Anki2/addons21/anki-connect"),
            ]
            .iter()
            .any(|target| path.starts_with(target))
            {
                bail!("invalid artifact directory receipt");
            }
        }
        let mut desired = BTreeMap::new();
        let mut ids = std::collections::BTreeSet::new();
        let live = crate::home_copy::live_paths(source)?;
        for artifact in &evaluation.manifest_data.artifacts {
            let contract = match artifact.id.as_str() {
                "anki-connect" => (
                    "anki-addon",
                    "share/anki/addons/anki-connect",
                    "Library/Application Support/Anki2/addons21/anki-connect",
                ),
                "localllm" => ("executable", "bin/localllm", ".local/bin/localllm"),
                _ => bail!("unknown artifact ID"),
            };
            if !ids.insert(&artifact.id)
                || artifact.kind != contract.0
                || artifact.relative_path != Path::new(contract.1)
                || artifact.home_target != Path::new(contract.2)
            {
                bail!("invalid or duplicate artifact deployment contract");
            }
            store_path(&artifact.store_path, false)?;
            if copy
                .iter()
                .any(|p| overlaps(&artifact.home_target, Path::new(p)))
            {
                continue;
            }
            if live.iter().any(|p| overlaps(&artifact.home_target, p)) {
                bail!("artifact target conflicts with public live ownership");
            }
            desired.insert(
                artifact.home_target.clone(),
                artifact.store_path.join(&artifact.relative_path),
            );
        }
        let roots = state_home.join("dotfiles/artifact-roots");
        safe_parents(&state_anchor, &roots.join("entry"))?;
        let mut plan = Self {
            evaluation,
            nix: nix.to_owned(),
            home: home.to_owned(),
            state,
            state_anchor,
            stable_state_anchor,
            stable_home,
            state_bytes,
            applied,
            desired,
            observed: BTreeMap::new(),
            copy_handoffs: BTreeMap::new(),
            retiring_copies: BTreeMap::new(),
            copy_pruning: BTreeMap::new(),
            recovered_retirements: Default::default(),
            adopted: std::collections::BTreeSet::new(),
            changes: BTreeMap::new(),
            roots,
            rooted: false,
            home_manager_generation: home_manager_generation(home)?,
        };
        let paths: std::collections::BTreeSet<_> = plan
            .desired
            .keys()
            .chain(plan.applied.links.keys())
            .cloned()
            .collect();
        for path in paths {
            safe_parents(home, &home.join(&path))?;
            let current = crate::home_copy::fingerprint(&home.join(&path), false)?;
            let current_link = fs::read_link(home.join(&path)).ok();
            if let Some(wanted) = plan.desired.get(&path) {
                let owned = plan
                    .applied
                    .links
                    .get(&path)
                    .is_some_and(|old| current_link.as_ref() == Some(old));
                let adoption = exact_home_manager(home, &path, wanted)?;
                let recovered = current_link.as_ref() == Some(wanted)
                    && fs::read_link(plan.output_root()).ok().as_ref()
                        == Some(&plan.evaluation.output);
                let retiring_copy = if recovered {
                    None
                } else {
                    crate::home_copy::retiring_copy_fingerprint(home, &path, copy)?
                };
                if let Some(proof) = retiring_copy {
                    plan.retiring_copies.insert(path.clone(), proof);
                    capture_directories(home, &path, &mut plan.copy_pruning)?;
                } else if !recovered
                    && current.is_some()
                    && receipt_tree(home, &path, &plan.applied.retiring_directories)?
                {
                    plan.retiring_copies
                        .insert(path.clone(), current.clone().unwrap());
                    plan.recovered_retirements.insert(path.clone());
                    for (directory, receipt) in &plan.applied.retiring_directories {
                        if directory.starts_with(&path) {
                            plan.copy_pruning.insert(directory.clone(), receipt.clone());
                        }
                    }
                }
                if current.is_some()
                    && !owned
                    && !adoption
                    && !recovered
                    && !plan.retiring_copies.contains_key(&path)
                {
                    bail!(
                        "artifact target is unmanaged or modified: {}",
                        path.display()
                    );
                }
                if adoption {
                    plan.adopted.insert(path.clone());
                }
                if current_link.as_ref() != Some(wanted)
                    || plan.applied.links.get(&path) != Some(wanted)
                    || fs::read_link(plan.output_root()).ok().as_ref()
                        != Some(&plan.evaluation.output)
                {
                    plan.changes.insert(path.clone(), '+');
                }
            } else if current_link.as_ref() == plan.applied.links.get(&path) || current.is_none() {
                plan.changes.insert(path.clone(), '-');
                if copy.iter().any(|p| overlaps(&path, Path::new(p))) {
                    if let Some(old) = plan.applied.links.get(&path) {
                        plan.copy_handoffs.insert(path.clone(), old.clone());
                    }
                }
            }
            plan.observed.insert(path, current);
        }
        Ok(plan)
    }

    fn output_root(&self) -> PathBuf {
        self.roots.join(format!(
            "{:x}",
            Sha256::digest(self.evaluation.output.as_os_str().as_encoded_bytes())
        ))
    }
    pub(super) fn has_changes(&self) -> bool {
        !self.changes.is_empty()
    }
    pub(super) fn render(&self) -> String {
        self.changes
            .iter()
            .map(|(p, c)| format!("{c} artifact {}", p.display()))
            .collect::<Vec<_>>()
            .join("\n")
    }
    pub(super) fn targets(&self) -> impl Iterator<Item = &PathBuf> {
        self.desired.keys()
    }
    pub(super) fn verify(&self) -> Result<()> {
        if resolve_anchor(&self.home)? != self.stable_home
            || resolve_anchor(&self.state_anchor)? != self.stable_state_anchor
        {
            bail!("artifact home or state anchor changed after preview");
        }
        if home_manager_generation(&self.home)? != self.home_manager_generation {
            bail!("active Home Manager generation changed after artifact preview");
        }
        if self.rooted
            && !self.desired.is_empty()
            && fs::read_link(self.output_root())? != self.evaluation.output
        {
            bail!("artifact GC root changed before deployment");
        }
        safe_parents(&self.state_anchor, &self.state)?;
        if read_state(&self.state)? != self.state_bytes {
            bail!("artifact ledger changed after preview");
        }
        for (target, source) in &self.desired {
            if active_home_manager_declaration(&self.home, target)?.is_some()
                && (!self.adopted.contains(target)
                    || !exact_home_manager(&self.home, target, source)?)
            {
                bail!("artifact target has an unreviewed active Home Manager owner");
            }
        }
        for (path, before) in &self.observed {
            safe_parents(&self.home, &self.home.join(path))?;
            if crate::home_copy::fingerprint(&self.home.join(path), false)? != *before {
                bail!("artifact target changed after preview: {}", path.display());
            }
        }
        Ok(())
    }
    pub(super) fn realize(&mut self, lock: &File) -> Result<()> {
        self.verify()?;
        if !self.has_changes() {
            if !self.desired.is_empty()
                && fs::read_link(self.output_root())? != self.evaluation.output
            {
                bail!("artifact root changed after preview");
            }
            self.rooted = true;
            return Ok(());
        }
        safe_parents(&self.state_anchor, &self.roots.join("entry"))?;
        fs::create_dir_all(&self.roots)?;
        let root = self.output_root();
        if let Some(existing) = link(&root)? {
            if existing != self.evaluation.output {
                bail!("artifact GC root has changed");
            }
        }
        let mut command = super::nix_command(&self.nix);
        command
            .args([
                "build",
                "--json",
                "--no-write-lock-file",
                "--no-update-lock-file",
                "--out-link",
            ])
            .arg(&root)
            .arg(format!("{}^out", self.evaluation.root.display()));
        super::retain_apply_lock(&mut command, lock);
        let builds: Vec<super::BuildResult> = serde_json::from_slice(&super::capture(
            &mut command,
            "artifact build or GC registration failed",
        )?)?;
        if builds.len() != 1
            || Path::new(&builds[0].drv_path) != self.evaluation.root
            || builds[0].outputs.out != self.evaluation.output
            || fs::read_link(&root)? != self.evaluation.output
        {
            bail!("artifact realization differs from reviewed derivation/output");
        }
        let actual: Manifest =
            serde_json::from_slice(&fs::read(self.evaluation.output.join("manifest.json"))?)?;
        if actual != self.evaluation.manifest_data {
            bail!("built artifact manifest differs from reviewed manifest");
        }
        for artifact in &actual.artifacts {
            if fs::read_link(self.evaluation.output.join("artifacts").join(&artifact.id))?
                != artifact.store_path
            {
                bail!("artifact root package differs from reviewed source");
            }
            let source = artifact.store_path.join(&artifact.relative_path);
            if !source.exists() {
                bail!("artifact source is absent");
            }
        }
        self.verify()?;
        self.rooted = true;
        Ok(())
    }
    pub(super) fn retiring_copy_targets(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.retiring_copies
            .keys()
            .filter(|path| !self.recovered_retirements.contains(*path))
            .cloned()
    }
    pub(super) fn copy_handoffs(&self) -> BTreeMap<PathBuf, PathBuf> {
        self.copy_handoffs.clone()
    }
    pub(super) fn prepare_home(&mut self, _lock: &File) -> Result<()> {
        if !self.rooted {
            bail!("artifacts must be rooted before ownership handoff");
        }
        self.verify()?;
        if !self.copy_pruning.is_empty() {
            for (directory, receipt) in &self.copy_pruning {
                if directory_receipt(&self.home.join(directory))?
                    .is_some_and(|current| &current != receipt)
                {
                    bail!("copy retirement directory changed");
                }
            }
            self.applied
                .retiring_directories
                .extend(self.copy_pruning.clone());
            self.checkpoint()?;
        }
        for (path, source) in self.copy_handoffs.clone() {
            self.verify()?;
            if active_home_manager_declaration(&self.home, &path)?.is_some() {
                bail!("artifact-to-copy target has an active Home Manager owner");
            }
            let destination = self.home.join(&path);
            safe_parents(&self.home, &destination)?;
            match link(&destination)? {
                Some(current) if current == source => fs::remove_file(&destination)?,
                None => (),
                _ => bail!("artifact-to-copy ownership changed"),
            }
            self.applied.links.remove(&path);
            self.changes.remove(&path);
            self.observed.remove(&path);
            self.checkpoint()?;
        }
        self.copy_handoffs.clear();
        Ok(())
    }
    pub(super) fn apply(&mut self, _lock: &File) -> Result<()> {
        if !self.rooted {
            bail!("artifacts must be Nix-rooted before home deployment");
        }
        // Darwin activation may legitimately move current-home. Inspect the new
        // active declarations before accepting that move; private HM can also
        // switch independently of the Darwin generation.
        for (target, source) in &self.desired {
            if active_home_manager_declaration(&self.home, target)?.is_some()
                && (!self.adopted.contains(target)
                    || !exact_home_manager(&self.home, target, source)?)
            {
                bail!("artifact target acquired an unreviewed Home Manager owner");
            }
        }
        self.home_manager_generation = home_manager_generation(&self.home)?;
        // A reviewed home step may remove children, but surviving directories
        // must be the same observed objects and contain no unowned entries.
        for path in self.retiring_copies.keys() {
            let current = crate::home_copy::fingerprint(&self.home.join(path), false)?;
            if current.is_some() && !receipt_tree(&self.home, path, &self.copy_pruning)? {
                bail!("reviewed copy retirement has not completed");
            }
            self.observed.insert(path.clone(), current);
        }
        for path in &self.adopted {
            if crate::home_copy::fingerprint(&self.home.join(path), false)?.is_none() {
                self.observed.insert(path.clone(), None);
            }
        }
        self.verify()?;
        let mut pruning: Vec<_> = self.copy_pruning.iter().collect();
        pruning.sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        for (path, receipt) in pruning {
            safe_parents(&self.home, &self.home.join(path))?;
            match directory_receipt(&self.home.join(path))? {
                None => (),
                Some(current) if &current == receipt => fs::remove_dir(self.home.join(path))
                    .context("unmanaged content blocks artifact ownership handoff")?,
                _ => bail!("copy retirement directory identity changed"),
            }
        }
        // Only the exact copied resource or old HM link proved during capture
        // may disappear during the reviewed home/activation stage.
        for path in self.retiring_copies.keys() {
            if crate::home_copy::fingerprint(&self.home.join(path), false)?.is_some() {
                bail!("reviewed copy retirement has not completed");
            }
        }
        for path in self.retiring_copies.keys().chain(self.adopted.iter()) {
            if crate::home_copy::fingerprint(&self.home.join(path), false)?.is_none() {
                self.observed.insert(path.clone(), None);
            }
        }
        self.verify()?;
        for (path, change) in self.changes.clone() {
            self.verify()?;
            let destination = self.home.join(&path);
            safe_parents(&self.home, &destination)?;
            if change == '-' {
                if active_home_manager_declaration(&self.home, &path)?.is_some() {
                    bail!("retired artifact still has an active Home Manager owner");
                }
                if link(&destination)?.as_ref() == self.applied.links.get(&path)
                    && link(&destination)?.is_some()
                {
                    fs::remove_file(&destination)?;
                }
                self.applied.links.remove(&path);
            } else {
                let source = self
                    .desired
                    .get(&path)
                    .context("missing reviewed artifact source")?;
                fs::create_dir_all(
                    destination
                        .parent()
                        .context("artifact target has no parent")?,
                )?;
                let temp = tempfile::Builder::new()
                    .prefix(".dotfiles-artifact-")
                    .tempdir_in(destination.parent().unwrap())?;
                let temporary = temp.path().join("link");
                symlink(source, &temporary)?;
                fs::rename(temporary, &destination)?;
                if fs::read_link(&destination)? != *source {
                    bail!("artifact link verification failed");
                }
                self.applied.links.insert(path.clone(), source.clone());
                self.applied
                    .retiring_directories
                    .retain(|directory, _| !directory.starts_with(&path));
            }
            self.observed
                .insert(path, crate::home_copy::fingerprint(&destination, false)?);
            self.checkpoint()?;
        }
        self.changes.clear();
        Ok(())
    }
    fn checkpoint(&mut self) -> Result<()> {
        let directory = self
            .state
            .parent()
            .context("missing artifact ledger parent")?;
        safe_parents(&self.state_anchor, &self.state)?;
        fs::create_dir_all(directory)?;
        let bytes = serde_json::to_vec(&self.applied)?;
        let mut temp = tempfile::NamedTempFile::new_in(directory)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        temp.persist(&self.state)?;
        self.state_bytes = Some(bytes);
        Ok(())
    }
}

fn overlaps(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}
fn relative(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        bail!("unsafe artifact relative path");
    }
    Ok(())
}
fn store_path(path: &Path, derivation: bool) -> Result<()> {
    let name = path
        .file_name()
        .and_then(|p| p.to_str())
        .context("invalid artifact store path")?;
    if path.parent() != Some(Path::new("/nix/store"))
        || name.len() < 34
        || !name.as_bytes()[..32]
            .iter()
            .all(|c| c.is_ascii_alphanumeric())
        || name.as_bytes()[32] != b'-'
        || derivation != name.ends_with(".drv")
    {
        bail!("invalid artifact store path");
    }
    Ok(())
}
fn read_state(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_file() => Ok(Some(fs::read(path)?)),
        Ok(_) => bail!("artifact ledger must be a regular file"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn link(path: &Path) -> Result<Option<PathBuf>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => Ok(Some(fs::read_link(path)?)),
        Ok(_) => bail!("artifact target is not a symlink: {}", path.display()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn resolve_anchor(path: &Path) -> Result<PathBuf> {
    match path.canonicalize() {
        Ok(path) => Ok(path),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if fs::symlink_metadata(path).is_ok() {
                bail!("dangling artifact anchor");
            }
            Ok(
                resolve_anchor(path.parent().context("unresolvable artifact anchor")?)?
                    .join(path.file_name().context("invalid artifact anchor")?),
            )
        }
        Err(e) => Err(e.into()),
    }
}
fn safe_parents(anchor: &Path, path: &Path) -> Result<()> {
    if !anchor.is_absolute() || !path.is_absolute() {
        bail!("artifact paths must be absolute");
    }
    let relative = path
        .strip_prefix(anchor)
        .context("artifact path is outside its anchor")?;
    let mut current = anchor.to_owned();
    if fs::metadata(anchor).is_ok_and(|m| !m.is_dir()) {
        bail!("artifact anchor is not a directory");
    }
    for component in relative
        .parent()
        .context("artifact path has no parent")?
        .components()
    {
        if !matches!(component, Component::Normal(_)) {
            bail!("invalid artifact parent");
        }
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(m) if m.is_dir() => (),
            Ok(_) => bail!(
                "artifact parent is not a real directory: {}",
                current.display()
            ),
            Err(e) if e.kind() == io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}

fn directory_receipt(path: &Path) -> Result<Option<DirectoryReceipt>> {
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => Ok(Some(DirectoryReceipt {
            device: m.dev(),
            inode: m.ino(),
        })),
        Ok(_) => bail!("artifact retirement directory was replaced"),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn capture_directories(
    home: &Path,
    path: &Path,
    receipts: &mut BTreeMap<PathBuf, DirectoryReceipt>,
) -> Result<()> {
    if fs::symlink_metadata(home.join(path)).is_ok_and(|m| m.is_dir()) {
        receipts.insert(
            path.to_owned(),
            directory_receipt(&home.join(path))?.unwrap(),
        );
        for entry in fs::read_dir(home.join(path))? {
            capture_directories(home, &path.join(entry?.file_name()), receipts)?;
        }
    }
    Ok(())
}
fn receipt_tree(
    home: &Path,
    path: &Path,
    receipts: &BTreeMap<PathBuf, DirectoryReceipt>,
) -> Result<bool> {
    let Some(receipt) = receipts.get(path) else {
        return Ok(false);
    };
    if directory_receipt(&home.join(path))?.as_ref() != Some(receipt) {
        return Ok(false);
    }
    for entry in fs::read_dir(home.join(path))? {
        if !receipt_tree(home, &path.join(entry?.file_name()), receipts)? {
            return Ok(false);
        }
    }
    Ok(true)
}
fn home_manager_generation(home: &Path) -> Result<Option<PathBuf>> {
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    let generation = state.join("home-manager/gcroots/current-home");
    match fs::symlink_metadata(&generation) {
        Ok(_) => {
            Ok(Some(generation.canonicalize().context(
                "cannot resolve active Home Manager generation",
            )?))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn active_home_manager_declaration(home: &Path, relative: &Path) -> Result<Option<PathBuf>> {
    let state = env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/state"));
    let generation = state.join("home-manager/gcroots/current-home");
    if fs::symlink_metadata(&generation).is_err_and(|e| e.kind() == io::ErrorKind::NotFound) {
        return Ok(None);
    }
    let mut declaration = generation
        .join("home-files")
        .canonicalize()
        .context("cannot verify active Home Manager files")?;
    for component in relative.components() {
        declaration.push(component);
        match fs::symlink_metadata(&declaration) {
            Ok(m) if m.is_dir() => (),
            Ok(_) => return Ok(Some(declaration)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
    }
    Ok(Some(declaration))
}
fn exact_home_manager(home: &Path, relative: &Path, source: &Path) -> Result<bool> {
    let Some(declaration) = active_home_manager_declaration(home, relative)? else {
        return Ok(false);
    };
    let expected = source.canonicalize().ok();
    if expected.is_none() || declaration.canonicalize().ok() != expected {
        bail!("artifact target has a competing active Home Manager owner");
    }
    let target = home.join(relative);
    Ok(fs::read_link(&target).is_ok() && target.canonicalize().ok() == expected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::PermissionsExt;

    struct Fixture {
        _root: tempfile::TempDir,
        plan: Plan,
        lock: File,
    }
    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let home = root.path().join("home");
            fs::create_dir(&home).unwrap();
            let output = root.path().join("output");
            let package = root.path().join("package");
            fs::create_dir_all(output.join("artifacts")).unwrap();
            fs::create_dir_all(package.join("bin")).unwrap();
            fs::write(package.join("bin/localllm"), "#!/bin/sh\n").unwrap();
            let artifact = Artifact {
                id: "localllm".into(),
                kind: "executable".into(),
                store_path: package.clone(),
                relative_path: "bin/localllm".into(),
                home_target: ".local/bin/localllm".into(),
                model: Some("fixture".into()),
            };
            let manifest_data = Manifest {
                schema_version: 1,
                artifacts: vec![artifact],
            };
            fs::write(
                output.join("manifest.json"),
                serde_json::to_vec(&manifest_data).unwrap(),
            )
            .unwrap();
            symlink(&package, output.join("artifacts/localllm")).unwrap();
            let state = home.join(".local/state/dotfiles/artifacts.json");
            let target = PathBuf::from(".local/bin/localllm");
            let plan = Plan {
                evaluation: Evaluation {
                    manifest_data,
                    manifest: root.path().join("manifest.drv"),
                    root: root.path().join("root.drv"),
                    output,
                },
                nix: root.path().join("nix-fixture"),
                home: home.clone(),
                state,
                state_anchor: home.clone(),
                stable_state_anchor: resolve_anchor(&home).unwrap(),
                stable_home: resolve_anchor(&home).unwrap(),
                state_bytes: None,
                applied: Applied {
                    version: 1,
                    home: home.clone(),
                    links: BTreeMap::new(),
                    retiring_directories: BTreeMap::new(),
                },
                desired: BTreeMap::from([(target.clone(), package.join("bin/localllm"))]),
                observed: BTreeMap::from([(target.clone(), None)]),
                copy_handoffs: BTreeMap::new(),
                retiring_copies: BTreeMap::new(),
                copy_pruning: BTreeMap::new(),
                recovered_retirements: Default::default(),
                adopted: Default::default(),
                changes: BTreeMap::from([(target, '+')]),
                roots: home.join(".local/state/dotfiles/artifact-roots"),
                rooted: false,
                home_manager_generation: None,
            };
            let lock = File::create(root.path().join("lock")).unwrap();
            let fixture = Self {
                _root: root,
                plan,
                lock,
            };
            fixture.nix("", None);
            fixture
        }
        fn nix(&self, extra: &str, result: Option<serde_json::Value>) {
            let result = result.unwrap_or_else(|| serde_json::json!([{"drvPath":self.plan.evaluation.root,"outputs":{"out":self.plan.evaluation.output}}]));
            let script = format!("#!/bin/sh\nset -eu\n[ -e /dev/fd/{} ] || exit 33\nprintf '%s\\n' \"$@\" > '{}'\n{}\nwhile [ \"$1\" != --out-link ]; do shift; done\nshift\nln -sfn '{}' \"$1\"\nprintf '%s\\n' '{}'\n", self.lock.as_raw_fd(), self._root.path().join("calls").display(), extra, self.plan.evaluation.output.display(), result);
            fs::write(&self.plan.nix, script).unwrap();
            fs::set_permissions(&self.plan.nix, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fn target(&self) -> PathBuf {
            self.plan.home.join(".local/bin/localllm")
        }
        fn realize(&mut self) {
            self.plan.realize(&self.lock).unwrap();
        }
    }

    #[test]
    fn realization_registers_root_and_inherits_lock_before_home_writes() {
        let mut f = Fixture::new();
        f.realize();
        assert!(!f.target().exists());
        assert!(!f.plan.state.exists());
        assert_eq!(
            fs::read_link(f.plan.output_root()).unwrap(),
            f.plan.evaluation.output
        );
        let calls = fs::read_to_string(f._root.path().join("calls")).unwrap();
        assert!(calls.contains("--no-write-lock-file\n--no-update-lock-file"));
        assert!(calls.contains(&format!("{}^out", f.plan.evaluation.root.display())));
        f.plan.apply(&f.lock).unwrap();
        assert_eq!(
            fs::read_link(f.target()).unwrap(),
            *f.plan.desired.values().next().unwrap()
        );
        assert!(f.plan.state.exists());
        assert!(!f.plan.home.join(".local/state/dotfiles/home.json").exists());
    }
    #[test]
    fn failed_registration_or_wrong_output_never_deploys() {
        for wrong in [false, true] {
            let mut f = Fixture::new();
            if wrong {
                f.nix("", Some(serde_json::json!([{"drvPath":f.plan.evaluation.root,"outputs":{"out":"/wrong"}}])));
            } else {
                f.nix("exit 9", None);
            }
            assert!(f.plan.realize(&f.lock).is_err());
            assert!(f.plan.apply(&f.lock).is_err());
            assert!(!f.target().exists());
            assert!(!f.plan.state.exists());
        }
    }
    #[test]
    fn manifest_and_package_source_mismatch_refuse_deployment() {
        for manifest in [true, false] {
            let mut f = Fixture::new();
            if manifest {
                fs::write(
                    f.plan.evaluation.output.join("manifest.json"),
                    b"{\"schemaVersion\":1,\"artifacts\":[]}",
                )
                .unwrap();
            } else {
                let path = f.plan.evaluation.output.join("artifacts/localllm");
                fs::remove_file(&path).unwrap();
                symlink("/wrong", path).unwrap();
            }
            assert!(f.plan.realize(&f.lock).is_err());
            assert!(!f.target().exists());
        }
    }
    #[test]
    fn target_and_ledger_races_fail_before_mutation() {
        let mut f = Fixture::new();
        f.realize();
        fs::create_dir_all(f.target().parent().unwrap()).unwrap();
        fs::write(f.target(), "user data").unwrap();
        assert!(f.plan.apply(&f.lock).is_err());
        assert_eq!(fs::read_to_string(f.target()).unwrap(), "user data");
        fs::remove_file(f.target()).unwrap();
        fs::write(&f.plan.state, "{}").unwrap();
        assert!(f.plan.apply(&f.lock).is_err());
    }
    #[test]
    fn per_resource_checkpoint_supports_partial_retry_and_preserves_old_roots() {
        let mut f = Fixture::new();
        let second = PathBuf::from("zz/second");
        f.plan.desired.insert(
            second.clone(),
            f.plan.desired.values().next().unwrap().clone(),
        );
        f.plan.observed.insert(second.clone(), None);
        f.plan.changes.insert(second.clone(), '+');
        f.realize();
        // Cause the second rename to fail without changing its observed absence.
        fs::create_dir(f.plan.home.join("zz")).unwrap();
        fs::set_permissions(f.plan.home.join("zz"), fs::Permissions::from_mode(0o555)).unwrap();
        // An executable fixture is non-root on CI; root cannot test DAC failure.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        assert!(f.plan.apply(&f.lock).is_err());
        assert!(f.target().is_symlink());
        let state: Applied = serde_json::from_slice(&fs::read(&f.plan.state).unwrap()).unwrap();
        assert!(state.links.contains_key(Path::new(".local/bin/localllm")));
        assert!(!state.links.contains_key(&second));
        fs::set_permissions(f.plan.home.join("zz"), fs::Permissions::from_mode(0o755)).unwrap();
        f.plan.apply(&f.lock).unwrap();
        assert!(f.plan.home.join(second).is_symlink());
        assert!(f.plan.output_root().is_symlink());
    }
    #[test]
    fn exact_home_manager_source_adoption_rejects_alias_with_other_source() {
        let f = Fixture::new();
        let files = f._root.path().join("hm/home-files/.local/bin");
        fs::create_dir_all(&files).unwrap();
        let gcroots = f.plan.home.join(".local/state/home-manager/gcroots");
        fs::create_dir_all(&gcroots).unwrap();
        symlink(f._root.path().join("hm"), gcroots.join("current-home")).unwrap();
        let source = f.plan.desired.values().next().unwrap();
        symlink(source, files.join("localllm")).unwrap();
        fs::create_dir_all(f.target().parent().unwrap()).unwrap();
        symlink(files.join("localllm"), f.target()).unwrap();
        assert!(
            exact_home_manager(&f.plan.home, Path::new(".local/bin/localllm"), source).unwrap()
        );
        fs::remove_file(files.join("localllm")).unwrap();
        symlink("/unrelated", files.join("localllm")).unwrap();
        assert!(
            exact_home_manager(&f.plan.home, Path::new(".local/bin/localllm"), source).is_err()
        );
    }
    #[test]
    fn symlinked_parents_and_state_are_rejected() {
        let mut f = Fixture::new();
        symlink(f._root.path(), f.plan.home.join(".local")).unwrap();
        assert!(f.plan.realize(&f.lock).is_err());
        fs::remove_file(f.plan.home.join(".local")).unwrap();
        fs::create_dir_all(f.plan.state.parent().unwrap()).unwrap();
        symlink(f._root.path().join("other"), &f.plan.state).unwrap();
        assert!(f.plan.verify().is_err());
    }
    #[test]
    fn frozen_evaluation_plan_and_cancel_do_not_create_roots_or_ledger() {
        let f = Fixture::new();
        let source = f._root.path().join("source");
        fs::create_dir(&source).unwrap();
        let value = serde_json::json!({
            "root": "/nix/store/00000000000000000000000000000000-root.drv",
            "manifest": "/nix/store/11111111111111111111111111111111-manifest.drv",
            "output": "/nix/store/22222222222222222222222222222222-root",
            "manifestData": {"schemaVersion":1, "artifacts":[{
                "id":"localllm", "kind":"executable", "storePath":"/nix/store/33333333333333333333333333333333-localllm",
                "relativePath":"bin/localllm", "homeTarget":".local/bin/localllm", "model":"fixture"
            }]}
        });
        fs::write(
            &f.plan.nix,
            format!("#!/bin/sh\nprintf '%s\\n' '{}'\n", value),
        )
        .unwrap();
        let plan = Plan::capture(
            &f.plan.nix,
            &source,
            &source.join("inputs.json"),
            &f.plan.home,
            &[],
            false,
        )
        .unwrap();
        assert!(plan.has_changes());
        assert!(plan.render().contains("localllm"));
        plan.verify().unwrap();
        drop(plan); // rejecting the single review has no artifact effects
        assert!(!f.plan.roots.exists());
        assert!(!f.plan.state.exists());
        assert!(!f.target().exists());
    }
    #[test]
    fn disabling_removes_only_exact_recorded_link_and_keeps_registered_root() {
        let mut f = Fixture::new();
        f.realize();
        f.plan.apply(&f.lock).unwrap();
        let target = PathBuf::from(".local/bin/localllm");
        f.plan.desired.clear();
        f.plan.changes.insert(target.clone(), '-');
        f.plan.apply(&f.lock).unwrap();
        assert!(!f.target().exists());
        assert!(f.plan.output_root().is_symlink());
        assert!(f.plan.applied.links.is_empty());
    }
    #[test]
    fn artifact_to_copy_retires_exact_link_before_frozen_home_plan() {
        let mut f = Fixture::new();
        f.realize();
        f.plan.apply(&f.lock).unwrap();
        let target = PathBuf::from(".local/bin/localllm");
        let source = f._root.path().join("copy-source");
        fs::create_dir_all(source.join("home/.local/bin")).unwrap();
        fs::write(source.join("home").join(&target), "copied command").unwrap();
        f.plan
            .copy_handoffs
            .insert(target.clone(), f.plan.desired[&target].clone());
        f.plan.desired.clear();
        f.plan.changes.insert(target.clone(), '-');
        let copy = crate::home_copy::plan_live_with_artifact_handoffs(
            &source,
            &source,
            &f.plan.home,
            &[target.to_string_lossy().into_owned()],
            f.plan.copy_handoffs(),
        )
        .unwrap();
        f.plan.prepare_home(&f.lock).unwrap();
        assert!(!f.target().exists());
        copy.apply().unwrap();
        f.plan.apply(&f.lock).unwrap();
        assert_eq!(fs::read_to_string(f.target()).unwrap(), "copied command");
        assert!(f.plan.applied.links.is_empty());
    }
    #[test]
    fn copy_to_artifact_prunes_only_proved_retired_directory_tree() {
        let mut f = Fixture::new();
        let target = PathBuf::from(".local/bin/localllm");
        let child = ".local/bin/localllm/config";
        let source = f._root.path().join("copy-source");
        fs::create_dir_all(source.join("home/.local/bin/localllm")).unwrap();
        fs::write(source.join("home").join(child), "copied data").unwrap();
        crate::home_copy::plan(&source, &f.plan.home, &[child.into()])
            .unwrap()
            .apply()
            .unwrap();
        let proof = crate::home_copy::retiring_copy_fingerprint(&f.plan.home, &target, &[])
            .unwrap()
            .unwrap();
        f.plan.retiring_copies.insert(target.clone(), proof.clone());
        f.plan.observed.insert(target.clone(), Some(proof));
        let copy = crate::home_copy::plan(&source, &f.plan.home, &[])
            .unwrap()
            .with_artifact_retirements(f.plan.retiring_copy_targets())
            .unwrap();
        f.realize();
        copy.apply().unwrap();
        assert!(!f.target().exists());
        f.plan.apply(&f.lock).unwrap();
        assert!(f.target().is_symlink());
    }
    #[test]
    fn copy_to_artifact_preserves_unmanaged_siblings_and_modified_copies() {
        let f = Fixture::new();
        let target = Path::new(".local/bin/localllm");
        let child = ".local/bin/localllm/config";
        let source = f._root.path().join("copy-source");
        fs::create_dir_all(source.join("home/.local/bin/localllm")).unwrap();
        fs::write(source.join("home").join(child), "copied data").unwrap();
        crate::home_copy::plan(&source, &f.plan.home, &[child.into()])
            .unwrap()
            .apply()
            .unwrap();
        fs::write(f.target().join("runtime"), "do not delete").unwrap();
        assert!(crate::home_copy::retiring_copy_fingerprint(&f.plan.home, target, &[]).is_err());
        fs::remove_file(f.target().join("runtime")).unwrap();
        fs::write(f.target().join("config"), "user changed").unwrap();
        assert!(crate::home_copy::retiring_copy_fingerprint(&f.plan.home, target, &[]).is_err());
        assert_eq!(
            fs::read_to_string(f.target().join("config")).unwrap(),
            "user changed"
        );
    }
    #[test]
    fn mixed_home_retirement_and_interruption_preserve_directory_provenance() {
        let mut f = Fixture::new();
        let target = PathBuf::from(".local/bin/localllm");
        let child = ".local/bin/localllm/config";
        let source = f._root.path().join("copy-source");
        fs::create_dir_all(source.join("home/.local/bin/localllm")).unwrap();
        fs::write(source.join("home").join(child), "copied data").unwrap();
        crate::home_copy::plan(&source, &f.plan.home, &[child.into()])
            .unwrap()
            .apply()
            .unwrap();
        let proof = crate::home_copy::retiring_copy_fingerprint(&f.plan.home, &target, &[])
            .unwrap()
            .unwrap();
        f.plan.retiring_copies.insert(target.clone(), proof.clone());
        f.plan.observed.insert(target.clone(), Some(proof));
        capture_directories(&f.plan.home, &target, &mut f.plan.copy_pruning).unwrap();
        f.realize();
        f.plan.prepare_home(&f.lock).unwrap();
        // The generation's CLI knows only ordinary home copies, so it removes
        // the copied leaf and forgets its receipt but leaves the parent dir.
        crate::home_copy::plan(&source, &f.plan.home, &[])
            .unwrap()
            .apply()
            .unwrap();
        assert!(f.target().is_dir());
        let saved: Applied = serde_json::from_slice(&fs::read(&f.plan.state).unwrap()).unwrap();
        assert!(receipt_tree(&f.plan.home, &target, &saved.retiring_directories).unwrap());
        assert!(
            crate::home_copy::retiring_copy_fingerprint(&f.plan.home, &target, &[])
                .unwrap()
                .is_none()
        );
        f.plan.apply(&f.lock).unwrap();
        assert!(f.target().is_symlink());
        assert!(f.plan.applied.retiring_directories.is_empty());
    }
    #[test]
    fn interrupted_copied_child_removal_can_finish_without_deleting_siblings() {
        let f = Fixture::new();
        let target = PathBuf::from(".local/bin/localllm");
        let child = ".local/bin/localllm/config";
        let source = f._root.path().join("copy-source");
        fs::create_dir_all(source.join("home/.local/bin/localllm")).unwrap();
        fs::write(source.join("home").join(child), "copied data").unwrap();
        crate::home_copy::plan(&source, &f.plan.home, &[child.into()])
            .unwrap()
            .apply()
            .unwrap();
        fs::remove_file(f.plan.home.join(child)).unwrap();
        assert!(
            crate::home_copy::retiring_copy_fingerprint(&f.plan.home, &target, &[])
                .unwrap()
                .is_some()
        );
        fs::write(f.target().join("runtime"), "keep").unwrap();
        assert!(crate::home_copy::retiring_copy_fingerprint(&f.plan.home, &target, &[]).is_err());
        assert_eq!(
            fs::read_to_string(f.target().join("runtime")).unwrap(),
            "keep"
        );
    }
    #[test]
    fn newly_claimed_home_manager_target_and_lost_root_block_apply() {
        let mut f = Fixture::new();
        f.realize();
        fs::remove_file(f.plan.output_root()).unwrap();
        assert!(f.plan.apply(&f.lock).is_err());
        symlink(&f.plan.evaluation.output, f.plan.output_root()).unwrap();
        let files = f._root.path().join("hm/home-files/.local/bin");
        fs::create_dir_all(&files).unwrap();
        symlink(
            f.plan.desired.values().next().unwrap(),
            files.join("localllm"),
        )
        .unwrap();
        let gcroots = f.plan.home.join(".local/state/home-manager/gcroots");
        fs::create_dir_all(&gcroots).unwrap();
        symlink(f._root.path().join("hm"), gcroots.join("current-home")).unwrap();
        assert!(f.plan.apply(&f.lock).is_err());
        assert!(!f.target().exists());
    }
    #[test]
    fn modified_disabled_artifact_is_preserved_on_fresh_plan() {
        let mut f = Fixture::new();
        f.realize();
        f.plan.apply(&f.lock).unwrap();
        fs::remove_file(f.target()).unwrap();
        fs::write(f.target(), "user replacement").unwrap();
        let source = f._root.path().join("source");
        fs::create_dir(&source).unwrap();
        let evaluation = Evaluation {
            root: "/nix/store/00000000000000000000000000000000-empty.drv".into(),
            manifest: "/nix/store/11111111111111111111111111111111-manifest.drv".into(),
            output: "/nix/store/22222222222222222222222222222222-empty".into(),
            manifest_data: Manifest {
                schema_version: 1,
                artifacts: vec![],
            },
        };
        let plan =
            Plan::from_evaluation(&f.plan.nix, &source, &f.plan.home, &[], evaluation).unwrap();
        assert!(!plan.has_changes());
        plan.verify().unwrap();
        assert_eq!(fs::read_to_string(f.target()).unwrap(), "user replacement");
        assert!(f.plan.output_root().is_symlink());
    }
    #[test]
    fn fresh_plan_recovers_retired_directory_receipt_after_home_ledger_forgets_copy() {
        let f = Fixture::new();
        let target = PathBuf::from(".local/bin/localllm");
        fs::create_dir_all(f.target()).unwrap();
        let mut applied = f.plan.applied.clone();
        capture_directories(&f.plan.home, &target, &mut applied.retiring_directories).unwrap();
        fs::create_dir_all(f.plan.state.parent().unwrap()).unwrap();
        fs::write(&f.plan.state, serde_json::to_vec(&applied).unwrap()).unwrap();
        let source = f._root.path().join("source");
        fs::create_dir(&source).unwrap();
        let evaluation = Evaluation {
            root: "/nix/store/00000000000000000000000000000000-root.drv".into(),
            manifest: "/nix/store/11111111111111111111111111111111-manifest.drv".into(),
            output: "/nix/store/22222222222222222222222222222222-root".into(),
            manifest_data: Manifest {
                schema_version: 1,
                artifacts: vec![Artifact {
                    id: "localllm".into(),
                    kind: "executable".into(),
                    store_path: "/nix/store/33333333333333333333333333333333-localllm".into(),
                    relative_path: "bin/localllm".into(),
                    home_target: target.clone(),
                    model: Some("fixture".into()),
                }],
            },
        };
        let plan =
            Plan::from_evaluation(&f.plan.nix, &source, &f.plan.home, &[], evaluation).unwrap();
        assert!(plan.recovered_retirements.contains(&target));
        assert!(plan.retiring_copy_targets().next().is_none());
        assert!(plan.copy_pruning.contains_key(&target));
        assert!(plan.has_changes());
        plan.verify().unwrap();
    }
    #[test]
    fn retry_accepts_already_pruned_nested_receipt_directory() {
        let mut f = Fixture::new();
        let target = PathBuf::from(".local/bin/localllm");
        fs::create_dir_all(f.target().join("nested")).unwrap();
        capture_directories(&f.plan.home, &target, &mut f.plan.copy_pruning).unwrap();
        f.plan.observed.insert(
            target.clone(),
            crate::home_copy::fingerprint(&f.target(), false).unwrap(),
        );
        f.realize();
        f.plan.prepare_home(&f.lock).unwrap();
        fs::remove_dir(f.target().join("nested")).unwrap();
        // A fresh capture observes the surviving unchanged parent object and
        // restores all receipts, including the already completed nested prune.
        f.plan.observed.insert(
            target.clone(),
            crate::home_copy::fingerprint(&f.target(), false).unwrap(),
        );
        f.plan
            .retiring_copies
            .insert(target.clone(), "observed directory".into());
        f.plan.prepare_home(&f.lock).unwrap();
        f.plan.apply(&f.lock).unwrap();
        assert!(f.target().is_symlink());
    }
    #[test]
    fn rooted_final_link_recovers_before_stale_directory_receipts_are_inspected() {
        let f = Fixture::new();
        let target = PathBuf::from(".local/bin/localllm");
        fs::create_dir_all(f.target()).unwrap();
        let mut applied = f.plan.applied.clone();
        capture_directories(&f.plan.home, &target, &mut applied.retiring_directories).unwrap();
        fs::create_dir_all(f.plan.state.parent().unwrap()).unwrap();
        fs::write(&f.plan.state, serde_json::to_vec(&applied).unwrap()).unwrap();
        let output = PathBuf::from("/nix/store/22222222222222222222222222222222-root");
        let package = PathBuf::from("/nix/store/33333333333333333333333333333333-localllm");
        fs::remove_dir(f.target()).unwrap();
        symlink(package.join("bin/localllm"), f.target()).unwrap();
        fs::create_dir_all(&f.plan.roots).unwrap();
        symlink(
            &output,
            f.plan.roots.join(format!(
                "{:x}",
                Sha256::digest(output.as_os_str().as_encoded_bytes())
            )),
        )
        .unwrap();
        let source = f._root.path().join("source");
        fs::create_dir(&source).unwrap();
        let evaluation = Evaluation {
            root: "/nix/store/00000000000000000000000000000000-root.drv".into(),
            manifest: "/nix/store/11111111111111111111111111111111-manifest.drv".into(),
            output,
            manifest_data: Manifest {
                schema_version: 1,
                artifacts: vec![Artifact {
                    id: "localllm".into(),
                    kind: "executable".into(),
                    store_path: package,
                    relative_path: "bin/localllm".into(),
                    home_target: target,
                    model: Some("fixture".into()),
                }],
            },
        };
        let plan =
            Plan::from_evaluation(&f.plan.nix, &source, &f.plan.home, &[], evaluation).unwrap();
        assert!(plan.has_changes());
        assert!(plan.retiring_copies.is_empty());
        plan.verify().unwrap();
    }

    #[test]
    fn external_state_anchor_supports_roots_and_ledger_outside_home() {
        let mut f = Fixture::new();
        let state = f._root.path().join("external-state");
        f.plan.state_anchor = state.clone();
        f.plan.stable_state_anchor = resolve_anchor(&state).unwrap();
        f.plan.state = state.join("dotfiles/artifacts.json");
        f.plan.roots = state.join("dotfiles/artifact-roots");
        f.realize();
        f.plan.apply(&f.lock).unwrap();
        assert!(f.plan.state.is_file());
        assert!(f.plan.output_root().is_symlink());
        assert!(f.target().is_symlink());
        assert!(!f.plan.home.join(".local/state").exists());
    }
    #[test]
    fn trusted_home_alias_is_allowed_but_alias_retargeting_is_rejected() {
        let mut f = Fixture::new();
        let alias = f._root.path().join("home-alias");
        symlink(&f.plan.home, &alias).unwrap();
        f.plan.home = alias.clone();
        f.plan.applied.home = alias.clone();
        f.plan.state_anchor = alias.clone();
        f.plan.state = alias.join(".local/state/dotfiles/artifacts.json");
        f.plan.roots = alias.join(".local/state/dotfiles/artifact-roots");
        f.realize();
        let other = f._root.path().join("other-home");
        fs::create_dir(&other).unwrap();
        fs::remove_file(&alias).unwrap();
        symlink(&other, &alias).unwrap();
        assert!(f.plan.apply(&f.lock).is_err());
        assert!(!other.join(".local").exists());
    }
}
