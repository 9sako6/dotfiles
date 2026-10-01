use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    home: PathBuf,
    copies: BTreeMap<String, String>,
    #[serde(default)]
    links: BTreeMap<String, PathBuf>,
}

pub(super) struct State {
    path: PathBuf,
    manifest: Manifest,
}

impl State {
    pub(super) fn load(home: &Path) -> Result<Self> {
        if !home.is_absolute() {
            bail!("home copy root must be absolute");
        }
        let directory = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".local/state"))
            .join("dotfiles");
        let path = directory.join("home.json");
        let manifest = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() => {
                let manifest: Manifest = serde_json::from_slice(&fs::read(&path)?)
                    .with_context(|| format!("invalid home copy state: {}", path.display()))?;
                if manifest.version != 1 || manifest.home != home {
                    bail!(
                        "home copy state has an unsupported version or another home: {}",
                        path.display()
                    );
                }
                manifest
            }
            Ok(_) => bail!("home copy state must be a regular file: {}", path.display()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Manifest {
                version: 1,
                home: home.to_path_buf(),
                copies: BTreeMap::new(),
                links: BTreeMap::new(),
            },
            Err(error) => return Err(error).context("cannot inspect home copy state"),
        };
        let state = Self { path, manifest };
        let paths: Vec<_> = state.manifest.copies.keys().cloned().collect();
        super::validate_paths(&paths).context("invalid home copy state paths")?;
        for (relative, fingerprint) in &state.manifest.copies {
            state.validate_destination(&home.join(relative))?;
            if fingerprint.len() != 64
                || !fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            {
                bail!(
                    "invalid home copy state fingerprint: {}",
                    state.path.display()
                );
            }
        }
        let paths: Vec<_> = state.manifest.links.keys().cloned().collect();
        super::validate_paths(&paths).context("invalid home link state paths")?;
        for (relative, target) in &state.manifest.links {
            state.validate_destination(&home.join(relative))?;
            if !target.is_absolute() {
                bail!("invalid home link state target: {}", state.path.display());
            }
        }
        Ok(state)
    }

    pub(super) fn copies(&self) -> &BTreeMap<String, String> {
        &self.manifest.copies
    }

    pub(super) fn links(&self) -> &BTreeMap<String, PathBuf> {
        &self.manifest.links
    }

    pub(super) fn record_link(&mut self, relative: &Path, target: &Path) -> Result<()> {
        let relative = relative.to_str().context("invalid home link state path")?;
        if self
            .manifest
            .links
            .get(relative)
            .is_some_and(|previous| previous == target)
        {
            return Ok(());
        }
        self.manifest
            .links
            .insert(relative.to_owned(), target.to_owned());
        self.save()
    }

    pub(super) fn forget_link(&mut self, relative: &str) -> Result<()> {
        self.manifest.links.remove(relative);
        self.save()
    }

    pub(super) fn validate_destination(&self, destination: &Path) -> Result<()> {
        if self.path.starts_with(destination) || destination.starts_with(&self.path) {
            bail!(
                "copy destination overlaps home copy state: {}",
                destination.display()
            );
        }
        Ok(())
    }

    pub(super) fn record(&mut self, relative: &Path, fingerprint: String) -> Result<()> {
        let relative = relative.to_str().context("invalid home copy state path")?;
        if self.manifest.copies.get(relative) == Some(&fingerprint) {
            return Ok(());
        }
        self.manifest
            .copies
            .insert(relative.to_owned(), fingerprint);
        self.save()
    }

    pub(super) fn forget(&mut self, relative: &str) -> Result<()> {
        self.manifest.copies.remove(relative);
        self.save()
    }

    fn save(&self) -> Result<()> {
        let directory = self
            .path
            .parent()
            .context("home copy state has no parent")?;
        fs::create_dir_all(directory).context("cannot create home copy state directory")?;
        let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
        temporary.write_all(&serde_json::to_vec(&self.manifest)?)?;
        temporary.as_file().sync_all()?;
        temporary
            .persist(&self.path)
            .context("cannot record successful home copy")?;
        Ok(())
    }
}
