use std::fs;
use std::io;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

pub(super) struct SystemSource {
    entries: Vec<PathBuf>,
    pub fingerprint: String,
}

impl SystemSource {
    pub fn inspect(source: &Path, copy: &[String]) -> Result<Self> {
        let mut excluded: Vec<_> = copy
            .iter()
            .map(|path| Path::new("home").join(path))
            .collect();
        excluded.extend(["home/.apm", "home/apm.lock.yaml", "home/apm.yml"].map(PathBuf::from));
        let mut entries = Vec::new();
        for name in [
            ".mise.toml",
            "cli",
            "dotfiles.toml",
            "flake.lock",
            "flake.nix",
            "home",
            "nix",
        ] {
            collect(source, Path::new(name), &excluded, &mut entries)?;
        }
        let mut digest = Sha256::new();
        for relative in &entries {
            let name = relative.as_os_str().as_encoded_bytes();
            digest.update((name.len() as u64).to_be_bytes());
            digest.update(name);
            let path = source.join(relative);
            let metadata = fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                digest.update(b"link");
                digest.update(Sha256::digest(
                    fs::read_link(path)?.as_os_str().as_encoded_bytes(),
                ));
            } else if metadata.is_dir() {
                digest.update(b"directory");
            } else if metadata.is_file() {
                digest.update(b"file");
                digest.update([u8::from(metadata.permissions().mode() & 0o111 != 0)]);
                digest.update(Sha256::digest(fs::read(path)?));
            } else {
                bail!("unsupported system input: {}", relative.display());
            }
        }
        Ok(Self {
            entries,
            fingerprint: format!("{:x}", digest.finalize()),
        })
    }

    pub fn materialize(&self, source: &Path, destination: &Path) -> Result<()> {
        fs::create_dir(destination)?;
        for relative in &self.entries {
            let from = source.join(relative);
            let to = destination.join(relative);
            let metadata = fs::symlink_metadata(&from)?;
            if metadata.file_type().is_symlink() {
                symlink(fs::read_link(from)?, to)?;
            } else if metadata.is_dir() {
                fs::create_dir(to)?;
            } else {
                fs::copy(from, to)?;
            }
        }
        Ok(())
    }
}

fn collect(
    source: &Path,
    path: &Path,
    excluded: &[PathBuf],
    entries: &mut Vec<PathBuf>,
) -> Result<()> {
    if excluded.iter().any(|excluded| path.starts_with(excluded)) {
        return Ok(());
    }
    let metadata = match fs::symlink_metadata(source.join(path)) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("cannot inspect system inputs"),
    };
    entries.push(path.to_owned());
    if metadata.is_dir() {
        let mut children = fs::read_dir(source.join(path))?.collect::<io::Result<Vec<_>>>()?;
        children.sort_by_key(|child| child.file_name());
        for child in children {
            collect(source, &path.join(child.file_name()), excluded, entries)?;
        }
    }
    Ok(())
}

pub(super) fn identity(
    source: &SystemSource,
    inputs: &super::Inputs,
    local: &Option<Vec<u8>>,
    home: &Path,
) -> Result<String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 1,
        "public": source.fingerprint,
        "private": inputs.private_flake,
        "local": local,
        "directory": inputs.directory,
        "user": inputs.user,
        "home": home,
    }))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

pub(super) fn matches_generation(generation: Option<&Path>, expected: &str) -> Result<bool> {
    let Some(generation) = generation else {
        return Ok(false);
    };
    match fs::read_to_string(generation.join("dotfiles-system-inputs")) {
        Ok(value) => Ok(value == expected),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).context("cannot read active system inputs"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(root: &Path, name: &str, contents: &str) {
        let path = root.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    #[test]
    fn copy_and_agent_sources_are_absent_from_system_evaluation_inputs() {
        let root = tempfile::tempdir().unwrap();
        for path in [
            "flake.nix",
            "flake.lock",
            "dotfiles.toml",
            "cli/src/main.rs",
            "nix/home.nix",
            "home/.config/mise/config.toml",
            "home/.agents/skills/a/SKILL.md",
            "home/apm.yml",
            "home/apm.lock.yaml",
            "home/.apm/skills/a/SKILL.md",
            "docs/operations.md",
        ] {
            write(root.path(), path, "original");
        }
        let copy = vec![".agents/skills".to_owned()];
        let original = SystemSource::inspect(root.path(), &copy).unwrap();
        for path in [
            "home/.agents/skills/a/SKILL.md",
            "home/apm.yml",
            "home/apm.lock.yaml",
            "home/.apm/skills/a/SKILL.md",
            "docs/operations.md",
        ] {
            write(root.path(), path, "updated");
            assert_eq!(
                original.fingerprint,
                SystemSource::inspect(root.path(), &copy)
                    .unwrap()
                    .fingerprint,
                "{path}"
            );
        }
        write(root.path(), "home/.agents/skills/new/SKILL.md", "new skill");
        assert_eq!(
            original.fingerprint,
            SystemSource::inspect(root.path(), &copy)
                .unwrap()
                .fingerprint
        );
        let projected = root.path().join("projection");
        original.materialize(root.path(), &projected).unwrap();
        assert!(!projected.join("home/.agents/skills").exists());
        assert!(!projected.join("home/apm.yml").exists());
        assert!(!projected.join("home/.apm").exists());
        assert!(!projected.join("docs").exists());
        assert_eq!(
            fs::read_to_string(projected.join("nix/home.nix")).unwrap(),
            "original"
        );
        assert_eq!(
            original.fingerprint,
            SystemSource::inspect(&projected, &copy)
                .unwrap()
                .fingerprint
        );
        for path in [
            "flake.nix",
            "flake.lock",
            "dotfiles.toml",
            "cli/src/main.rs",
            "nix/home.nix",
            "home/.config/mise/config.toml",
        ] {
            write(root.path(), path, "updated");
            assert_ne!(
                original.fingerprint,
                SystemSource::inspect(root.path(), &copy)
                    .unwrap()
                    .fingerprint,
                "{path}"
            );
            write(root.path(), path, "original");
        }
        assert_ne!(
            original.fingerprint,
            SystemSource::inspect(root.path(), &[]).unwrap().fingerprint
        );
    }

    #[test]
    fn file_names_types_executable_bits_and_link_targets_invalidate_the_system() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "home/mybin/tool", "tool");
        let inspect = || SystemSource::inspect(root.path(), &[]).unwrap().fingerprint;
        let original = inspect();
        let tool = root.path().join("home/mybin/tool");
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(original, inspect());
        fs::set_permissions(&tool, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(original, inspect());
        fs::rename(&tool, tool.with_file_name("renamed")).unwrap();
        assert_ne!(original, inspect());
        fs::rename(tool.with_file_name("renamed"), &tool).unwrap();
        fs::remove_file(&tool).unwrap();
        symlink("first", &tool).unwrap();
        let first_link = inspect();
        assert_ne!(original, first_link);
        fs::remove_file(&tool).unwrap();
        symlink("second", &tool).unwrap();
        assert_ne!(first_link, inspect());
    }

    #[test]
    fn generation_match_requires_every_host_input_and_falls_back_for_legacy_generations() {
        let root = tempfile::tempdir().unwrap();
        let source = SystemSource::inspect(root.path(), &[]).unwrap();
        let mut inputs = super::super::Inputs {
            directory: "/checkout".into(),
            local_file: None,
            private_flake: None,
            public_flake: "path:/source".into(),
            public_revision: "commit-a".into(),
            public_source: "/source".into(),
            resource_flake: "path:/source".into(),
            system_inputs: None,
            user: "fixture".into(),
        };
        let local = Some(b"local configuration".to_vec());
        let home = Path::new("/home");
        let original = identity(&source, &inputs, &local, home).unwrap();
        assert!(!matches_generation(None, &original).unwrap());
        assert!(!matches_generation(Some(root.path()), &original).unwrap());
        let record = root.path().join("dotfiles-system-inputs");
        fs::write(&record, &original).unwrap();
        assert!(matches_generation(Some(root.path()), &original).unwrap());
        inputs.public_revision = "commit-b".into();
        inputs.resource_flake = "path:/other-resource-snapshot".into();
        assert_eq!(original, identity(&source, &inputs, &local, home).unwrap());
        for changed in [None, Some(b"other configuration".to_vec())] {
            assert_ne!(
                original,
                identity(&source, &inputs, &changed, home).unwrap()
            );
        }
        assert_ne!(
            original,
            identity(&source, &inputs, &local, Path::new("/other-home")).unwrap()
        );
        inputs.private_flake = Some("path:/private?narHash=changed".into());
        assert_ne!(original, identity(&source, &inputs, &local, home).unwrap());
        inputs.private_flake = None;
        inputs.user = "another".into();
        assert_ne!(original, identity(&source, &inputs, &local, home).unwrap());
        inputs.user = "fixture".into();
        inputs.directory = "/other-checkout".into();
        let changed = identity(&source, &inputs, &local, home).unwrap();
        assert!(!matches_generation(Some(root.path()), &changed).unwrap());
        fs::write(record, "old-format").unwrap();
        assert!(!matches_generation(Some(root.path()), &original).unwrap());
    }
}
