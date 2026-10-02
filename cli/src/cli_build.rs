use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

struct Source {
    files: Vec<(PathBuf, Vec<u8>, u32)>,
    rust: String,
    identity: String,
}

impl Source {
    fn capture(root: &Path) -> Result<Self> {
        let config: toml::Value = toml::from_str(&fs::read_to_string(
            root.join("home/.config/mise/config.toml"),
        )?)
        .map_err(|_| anyhow::anyhow!("cannot parse mise configuration"))?;
        let rust = config
            .get("tools")
            .and_then(|tools| tools.get("rust"))
            .context("mise Rust must have an exact version")?;
        let rust = rust
            .as_str()
            .or_else(|| rust.get("version").and_then(toml::Value::as_str))
            .context("mise Rust must have an exact version")?
            .to_owned();
        if rust.split('.').count() != 3
            || !rust
                .split('.')
                .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
        {
            bail!("mise Rust must have an exact version");
        }
        let output = Command::new("git")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_COMMON_DIR")
            .arg("--no-optional-locks")
            .arg("-C")
            .arg(root)
            .args([
                "ls-files",
                "-z",
                "--",
                "cli/Cargo.lock",
                "cli/Cargo.toml",
                "cli/build.rs",
                "cli/src",
            ])
            .output()?;
        if !output.status.success() {
            bail!("cannot inspect tracked CLI inputs");
        }
        let mut paths = output
            .stdout
            .split(|b| *b == 0)
            .filter(|p| !p.is_empty())
            .map(|p| std::str::from_utf8(p).map(PathBuf::from))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        paths.sort();
        paths.dedup();
        let mut files = Vec::new();
        let mut hash = Sha256::new();
        hash.update(b"dotfiles-rootless-cli-v1\0");
        hash.update(rust.as_bytes());
        for path in paths {
            let metadata = match fs::symlink_metadata(root.join(&path)) {
                Ok(value) => value,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                bail!("CLI inputs must be regular files: {}", path.display());
            }
            let bytes = fs::read(root.join(&path))?;
            let mode = metadata.permissions().mode() & 0o777;
            let name = path.as_os_str().as_encoded_bytes();
            hash.update((name.len() as u64).to_be_bytes());
            hash.update(name);
            hash.update(mode.to_be_bytes());
            hash.update(Sha256::digest(&bytes));
            files.push((path, bytes, mode));
        }
        for required in ["cli/Cargo.toml", "cli/Cargo.lock", "cli/build.rs"] {
            if !files.iter().any(|(path, _, _)| path == Path::new(required)) {
                bail!("CLI build requires tracked {required}");
            }
        }
        Ok(Self {
            files,
            rust,
            identity: format!("source-{:x}", hash.finalize()),
        })
    }

    fn materialize(&self, destination: &Path) -> Result<()> {
        for (path, bytes, mode) in &self.files {
            let path = destination.join(path);
            fs::create_dir_all(path.parent().unwrap())?;
            fs::write(&path, bytes)?;
            fs::set_permissions(path, fs::Permissions::from_mode(*mode))?;
        }
        Ok(())
    }
}

struct BuildLock(fs::File);

impl Drop for BuildLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.0);
    }
}

pub fn refresh(root: &Path) -> Result<()> {
    if unsafe { libc::geteuid() } == 0 {
        bail!("run plan/apply as the login user");
    }
    if Source::capture(root)?.identity == env!("DOTFILES_BUILD_REVISION") {
        return Ok(());
    }
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    let binary = install(root, &home)?;
    let mut arguments = env::args_os();
    let original_name = arguments.next().context("missing executable name")?;
    Err(Command::new(binary)
        .arg0(original_name)
        .args(arguments)
        .exec())
    .context("cannot restart refreshed CLI")
}

pub fn install(root: &Path, home: &Path) -> Result<PathBuf> {
    install_with(root, home, Path::new("mise"))
}

fn install_with(root: &Path, home: &Path, mise: &Path) -> Result<PathBuf> {
    if !home.is_absolute() {
        bail!("HOME must be an absolute path");
    }
    let bin = home.join(".local/bin");
    fs::create_dir_all(&bin)?;
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(bin.join(".dotfiles-build.lock"))?;
    if !lock.metadata()?.is_file() {
        bail!("CLI build lock must be a regular file");
    }
    fs2::FileExt::try_lock_exclusive(&lock).context("CLI build is already running")?;
    let _lock = BuildLock(lock);
    let source = Source::capture(root)?;
    let installed = bin.join("dotfiles");
    if matches_identity(&installed, &source.identity) {
        return Ok(installed);
    }
    let workspace = tempfile::tempdir()?;
    source.materialize(workspace.path())?;
    let target = home.join(".cache/dotfiles/cli-target");
    let status = Command::new(mise)
        .args([
            "exec",
            &format!("rust@{}", source.rust),
            "--",
            "cargo",
            "build",
            "--locked",
            "--release",
        ])
        .current_dir(workspace.path().join("cli"))
        .env("CARGO_TARGET_DIR", &target)
        .env("DOTFILES_BUILD_REVISION", &source.identity)
        .env("MISE_AUTO_INSTALL", "false")
        .status()
        .context("cannot start mise Rust build")?;
    if !status.success() {
        bail!("CLI build failed; installed binary was retained");
    }
    let artifact = target.join("release/dotfiles");
    if !matches_identity(&artifact, &source.identity) {
        bail!("built CLI identity does not match its inputs; installed binary was retained");
    }
    if Source::capture(root)?.identity != source.identity {
        bail!("CLI inputs changed during build; installed binary was retained");
    }
    let mut replacement = tempfile::NamedTempFile::new_in(&bin)?;
    io::copy(&mut fs::File::open(artifact)?, replacement.as_file_mut())?;
    replacement.as_file_mut().flush()?;
    replacement
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o755))?;
    replacement.as_file().sync_all()?;
    let replacement = replacement.into_temp_path();
    if !matches_identity(&replacement, &source.identity) {
        bail!("replacement CLI cannot run from its destination; installed binary was retained");
    }
    replacement
        .persist(&installed)
        .context("cannot atomically install CLI")?;
    Ok(installed)
}

fn matches_identity(binary: &Path, identity: &str) -> bool {
    Command::new(binary)
        .arg("--version")
        .output()
        .is_ok_and(|output| {
            output.status.success() && output.stdout == format!("dotfiles {identity}\n").as_bytes()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        for (path, contents) in [
            ("cli/Cargo.toml", "fixture"),
            ("cli/Cargo.lock", "fixture"),
            ("cli/build.rs", "fixture"),
            ("cli/src/main.rs", "first"),
            (
                "home/.config/mise/config.toml",
                "[tools]\nrust = { version = '1.2.3' }\n",
            ),
        ] {
            fs::create_dir_all(root.join(path).parent().unwrap()).unwrap();
            fs::write(root.join(path), contents).unwrap();
        }
        for args in [vec!["init", "-q"], vec!["add", "cli", "home"]] {
            assert!(Command::new("git")
                .args(args)
                .current_dir(root)
                .status()
                .unwrap()
                .success());
        }
        temp
    }

    fn executable(path: &Path, body: &str) {
        fs::write(path, format!("#!/bin/sh\nset -eu\n{body}\n")).unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    fn identity_tracks_dirty_inputs_and_rust_but_not_unrelated_files() {
        let root = fixture();
        let first = Source::capture(root.path()).unwrap().identity;
        fs::write(root.path().join("unrelated"), "change").unwrap();
        fs::create_dir(root.path().join("cli/tests")).unwrap();
        fs::write(root.path().join("cli/tests/behavior.rs"), "test only").unwrap();
        fs::write(root.path().join("cli/README.md"), "documentation only").unwrap();
        assert!(Command::new("git")
            .args(["add", "cli"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        assert_eq!(first, Source::capture(root.path()).unwrap().identity);
        fs::write(root.path().join("cli/src/main.rs"), "dirty").unwrap();
        let dirty = Source::capture(root.path()).unwrap().identity;
        assert_ne!(first, dirty);
        fs::write(
            root.path().join("home/.config/mise/config.toml"),
            "[tools]\nrust = '1.2.4'\n",
        )
        .unwrap();
        assert_ne!(dirty, Source::capture(root.path()).unwrap().identity);
    }

    #[test]
    fn installs_atomically_skips_identical_inputs_and_retains_binary_on_failure_or_change() {
        use std::os::unix::fs::MetadataExt;
        let root = fixture();
        let home = tempfile::tempdir().unwrap();
        let mise = root.path().join("mise");
        executable(&mise, "test \"$1\" = exec\ntest \"$3\" = --\ntest \"$4 $5 $6 $7\" = 'cargo build --locked --release'\nmkdir -p \"$CARGO_TARGET_DIR/release\"\nprintf '#!/bin/sh\\necho \"dotfiles %s\"\\n' \"$DOTFILES_BUILD_REVISION\" > \"$CARGO_TARGET_DIR/release/dotfiles\"\nchmod +x \"$CARGO_TARGET_DIR/release/dotfiles\"");
        let installed = install_with(root.path(), home.path(), &mise).unwrap();
        let old = fs::read(&installed).unwrap();
        let inode = fs::metadata(&installed).unwrap().ino();
        executable(&mise, "exit 42");
        fs::create_dir(root.path().join("cli/tests")).unwrap();
        fs::write(root.path().join("cli/tests/behavior.rs"), "test only").unwrap();
        fs::write(root.path().join("cli/README.md"), "documentation only").unwrap();
        assert!(Command::new("git")
            .args(["add", "cli"])
            .current_dir(root.path())
            .status()
            .unwrap()
            .success());
        install_with(root.path(), home.path(), &mise).unwrap();
        assert_eq!(inode, fs::metadata(&installed).unwrap().ino());
        fs::write(root.path().join("cli/src/main.rs"), "second").unwrap();
        assert!(install_with(root.path(), home.path(), &mise).is_err());
        assert_eq!(old, fs::read(&installed).unwrap());
        executable(&mise, &format!("printf '#!/bin/sh\\necho \"dotfiles %s\"\\n' \"$DOTFILES_BUILD_REVISION\" > \"$CARGO_TARGET_DIR/release/dotfiles\"\nchmod +x \"$CARGO_TARGET_DIR/release/dotfiles\"\nprintf changed > '{}/cli/src/main.rs'", root.path().display()));
        assert!(install_with(root.path(), home.path(), &mise).is_err());
        assert_eq!(old, fs::read(&installed).unwrap());
    }
}
