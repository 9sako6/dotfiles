use std::fs;
use std::io;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

// System evaluation may only consume this projection. Public user resources,
// artifact constructors and CLI sources are deliberately outside that boundary.
const SYSTEM_FILES: &[&str] = &[
    "flake.lock",
    "flake.nix",
    "nix/default.nix",
    "nix/home.nix",
    "nix/homebrew-packages.nix",
    "nix/homebrew-shellenv.zsh",
    "nix/host-flake.nix",
    "nix/inventory.nix",
    "nix/macos-settings.nix",
    "nix/system.nix",
];

pub(super) struct SystemSource {
    entries: Vec<PathBuf>,
    pub fingerprint: String,
}

impl SystemSource {
    pub fn inspect(source: &Path, _copy: &[String]) -> Result<Self> {
        let mut entries = Vec::new();
        for name in SYSTEM_FILES {
            let path = Path::new(name);
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                match fs::symlink_metadata(source.join(parent)) {
                    Ok(metadata) if metadata.is_dir() => (),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error).context("cannot inspect system input parent"),
                    Ok(_) => bail!("system input parent must be a real directory"),
                }
            }
            match fs::symlink_metadata(source.join(path)) {
                Ok(metadata) if metadata.is_file() || metadata.file_type().is_symlink() => {
                    entries.push(path.to_owned());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => (),
                Err(error) => return Err(error).context("cannot inspect system inputs"),
                Ok(_) => bail!("system input must be a file: {}", path.display()),
            }
        }
        let mut digest = Sha256::new();
        digest.update(b"dotfiles-system-source-v2\0");
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
            } else {
                digest.update(b"file");
                digest.update([u8::from(metadata.permissions().mode() & 0o111 != 0)]);
                digest.update(Sha256::digest(fs::read(path)?));
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
            fs::create_dir_all(to.parent().context("system input has no parent")?)?;
            let metadata = fs::symlink_metadata(&from)?;
            if metadata.file_type().is_symlink() {
                symlink(fs::read_link(from)?, to)?;
            } else if metadata.is_file() {
                fs::copy(from, to)?;
            } else {
                bail!("system input changed while materializing");
            }
        }
        if Self::inspect(destination, &[])?.fingerprint != self.fingerprint {
            bail!("system inputs changed while materializing; run plan/apply again");
        }
        Ok(())
    }
}

pub(super) fn identity_v2(
    source: &SystemSource,
    directory: &Path,
    user: &str,
    home: &Path,
    private_identity: Option<&str>,
    local: &Option<Vec<u8>>,
) -> Result<String> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "version": 2,
        "public": source.fingerprint,
        "private": private_identity,
        "local": local,
        "directory": directory,
        "user": user,
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
    fn public_user_inputs_do_not_change_the_system_projection() {
        let root = tempfile::tempdir().unwrap();
        for path in SYSTEM_FILES {
            write(root.path(), path, "system");
        }
        let inspect = || SystemSource::inspect(root.path(), &[".agents/skills".into()]).unwrap();
        let original = inspect().fingerprint;
        for path in [
            ".mise.toml",
            "cli/Cargo.lock",
            "cli/src/main.rs",
            "docs/operations.md",
            "dotfiles.toml",
            "home/.agents/skills/a/SKILL.md",
            "home/.apm/skills/a/SKILL.md",
            "home/.config/mise/config.toml",
            "home/.config/mise/mise.lock",
            "home/.zsh.d/init",
            "home/apm.lock.yaml",
            "home/apm.yml",
            "home/non-live",
            "nix/artifacts.nix",
            "nix/configuration.nix",
            "nix/localllm/catalog.json",
            "nix/localllm/package.nix",
            "nix/packages.nix",
        ] {
            write(root.path(), path, "first");
            assert_eq!(original, inspect().fingerprint, "{path}");
            write(root.path(), path, "changed");
            fs::set_permissions(root.path().join(path), fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(original, inspect().fingerprint, "{path}");
            fs::remove_file(root.path().join(path)).unwrap();
            symlink("changed-link", root.path().join(path)).unwrap();
            assert_eq!(original, inspect().fingerprint, "{path}");
        }
        assert_eq!(
            original,
            SystemSource::inspect(root.path(), &[]).unwrap().fingerprint
        );
        let projected = root.path().join("projection");
        inspect().materialize(root.path(), &projected).unwrap();
        for path in SYSTEM_FILES {
            assert_eq!(fs::read_to_string(projected.join(path)).unwrap(), "system");
        }
        for path in [
            "cli",
            "home",
            "docs",
            "dotfiles.toml",
            "nix/localllm",
            "nix/artifacts.nix",
            "nix/packages.nix",
        ] {
            assert!(!projected.join(path).exists(), "{path}");
        }
        assert_eq!(
            original,
            SystemSource::inspect(&projected, &[]).unwrap().fingerprint
        );
    }

    #[test]
    fn every_owned_system_file_invalidates_identity() {
        let root = tempfile::tempdir().unwrap();
        for path in SYSTEM_FILES {
            write(root.path(), path, "original");
        }
        let inspect = || SystemSource::inspect(root.path(), &[]).unwrap().fingerprint;
        let original = inspect();
        for path in SYSTEM_FILES {
            write(root.path(), path, "changed");
            assert_ne!(original, inspect(), "{path}");
            write(root.path(), path, "original");
            assert_eq!(original, inspect(), "{path}");
        }
    }

    #[test]
    fn file_types_executable_bits_and_link_targets_invalidate_the_system() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "nix/system.nix", "system");
        let inspect = || SystemSource::inspect(root.path(), &[]).unwrap().fingerprint;
        let original = inspect();
        let file = root.path().join("nix/system.nix");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(original, inspect());
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(original, inspect());
        fs::rename(&file, file.with_file_name("renamed")).unwrap();
        assert_ne!(original, inspect());
        fs::rename(file.with_file_name("renamed"), &file).unwrap();
        fs::remove_file(&file).unwrap();
        symlink("first", &file).unwrap();
        let first_link = inspect();
        assert_ne!(original, first_link);
        fs::remove_file(&file).unwrap();
        symlink("second", &file).unwrap();
        assert_ne!(first_link, inspect());
    }

    #[test]
    fn materialization_rejects_input_change_and_symlinked_input_directories() {
        let root = tempfile::tempdir().unwrap();
        write(root.path(), "nix/system.nix", "system");
        let source = SystemSource::inspect(root.path(), &[]).unwrap();
        write(root.path(), "nix/system.nix", "changed");
        assert!(source
            .materialize(root.path(), &root.path().join("projection"))
            .is_err());
        let linked = root.path().join("linked");
        fs::create_dir(&linked).unwrap();
        symlink(root.path().join("nix"), linked.join("nix")).unwrap();
        assert!(SystemSource::inspect(&linked, &[]).is_err());
    }

    #[test]
    fn generation_match_requires_every_host_input_and_falls_back_for_legacy_generations() {
        let root = tempfile::tempdir().unwrap();
        let source = SystemSource::inspect(root.path(), &[]).unwrap();
        let directory = Path::new("/checkout");
        let local = Some(b"local configuration".to_vec());
        let home = Path::new("/home");
        let original = identity_v2(&source, directory, "fixture", home, None, &local).unwrap();
        assert!(!matches_generation(None, &original).unwrap());
        assert!(!matches_generation(Some(root.path()), &original).unwrap());
        let record = root.path().join("dotfiles-system-inputs");
        fs::write(&record, &original).unwrap();
        assert!(matches_generation(Some(root.path()), &original).unwrap());
        for changed in [None, Some(b"other configuration".to_vec())] {
            assert_ne!(
                original,
                identity_v2(&source, directory, "fixture", home, None, &changed).unwrap()
            );
        }
        for changed in [
            identity_v2(
                &source,
                Path::new("/other-checkout"),
                "fixture",
                home,
                None,
                &local,
            ),
            identity_v2(&source, directory, "other-user", home, None, &local),
            identity_v2(
                &source,
                directory,
                "fixture",
                Path::new("/other-home"),
                None,
                &local,
            ),
            identity_v2(
                &source,
                directory,
                "fixture",
                home,
                Some("private-content"),
                &local,
            ),
        ] {
            assert_ne!(original, changed.unwrap());
        }
        let private = identity_v2(
            &source,
            directory,
            "fixture",
            home,
            Some("private-a"),
            &local,
        )
        .unwrap();
        assert_ne!(
            private,
            identity_v2(
                &source,
                directory,
                "fixture",
                home,
                Some("private-b"),
                &local
            )
            .unwrap()
        );
        fs::write(record, "old-format").unwrap();
        assert!(!matches_generation(Some(root.path()), &original).unwrap());
    }
}
