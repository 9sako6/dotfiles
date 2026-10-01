//! A frozen tracked working tree, captured without starting Nix.
use std::fs;
use std::io;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

pub(super) struct Snapshot {
    pub directory: PathBuf,
    pub fingerprint: String,
    pub content_fingerprint: String,
    pub source: PathBuf,
    pub revision: String,
    frozen_fingerprint: String,
    _workspace: tempfile::TempDir,
}

struct Observation {
    fingerprint: String,
    content_fingerprint: String,
    revision: String,
    files: Vec<(PathBuf, Entry)>,
}

enum Entry {
    Missing,
    File(Vec<u8>, u32),
    Link(PathBuf),
}

impl Snapshot {
    pub fn capture(directory: &Path, private: bool) -> Result<Self> {
        let directory = directory
            .canonicalize()
            .context("checkout does not exist")?;
        let before = observe(&directory, private)?;
        let workspace = tempfile::Builder::new()
            .prefix("dotfiles-snapshot-")
            .tempdir()?;
        let source = workspace.path().join("source");
        fs::create_dir(&source)?;
        for (relative, entry) in &before.files {
            if matches!(entry, Entry::Missing) {
                continue;
            }
            let path = source.join(relative);
            fs::create_dir_all(path.parent().context("tracked path has no parent")?)?;
            match entry {
                Entry::File(bytes, mode) => {
                    fs::write(&path, bytes)?;
                    fs::set_permissions(&path, fs::Permissions::from_mode(*mode))?;
                }
                Entry::Link(target) => symlink(target, &path)?,
                Entry::Missing => unreachable!(),
            }
        }
        if observe(&directory, private)?.fingerprint != before.fingerprint {
            bail!("inputs changed while taking a snapshot; run plan again");
        }
        Ok(Self {
            directory,
            fingerprint: before.fingerprint,
            content_fingerprint: before.content_fingerprint,
            frozen_fingerprint: frozen_fingerprint(&source)?,
            source,
            revision: before.revision,
            _workspace: workspace,
        })
    }

    pub fn verify(&self) -> Result<()> {
        if frozen_fingerprint(&self.source)? != self.frozen_fingerprint {
            bail!("frozen inputs changed after preview; run plan/apply again");
        }
        if observe(&self.directory, false)?.fingerprint != self.fingerprint {
            bail!("inputs changed after preview; nothing was activated. Run plan/apply again");
        }
        Ok(())
    }
}

fn observe(directory: &Path, private: bool) -> Result<Observation> {
    let head = git(directory, &["rev-parse", "HEAD"])?;
    let index = git(directory, &["ls-files", "--stage", "-z"])?;
    let mut entries = Vec::new();
    for record in index
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
    {
        let text = std::str::from_utf8(record).context("tracked paths must be UTF-8")?;
        let (metadata, name) = text.split_once('\t').context("invalid Git index entry")?;
        let mut metadata = metadata.split(' ');
        let mode = metadata.next().context("missing Git file mode")?;
        let _object = metadata.next().context("missing Git object")?;
        let stage = metadata.next().context("missing Git stage")?;
        if stage != "0" || metadata.next().is_some() {
            bail!("resolve Git index conflicts before taking a snapshot");
        }
        if !matches!(mode, "100644" | "100755" | "120000") {
            bail!("unsupported tracked file type; submodules are not snapshot inputs");
        }
        let path = PathBuf::from(name);
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|part| !matches!(part, Component::Normal(_)))
        {
            bail!("invalid tracked path");
        }
        if path == Path::new("dotfiles.local.toml") {
            bail!("dotfiles.local.toml must remain untracked; remove it from the Git index");
        }
        entries.push(path);
    }
    entries.sort();
    if entries.windows(2).any(|pair| pair[0] == pair[1]) {
        bail!("duplicate tracked path");
    }
    for file in ["flake.nix", "flake.lock"] {
        let path = directory.join(file);
        if !entries.iter().any(|entry| entry == Path::new(file))
            || !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file())
        {
            bail!("checkout requires tracked regular flake.nix and flake.lock");
        }
    }
    if private
        && git(directory, &["show", "HEAD:flake.lock"])? != fs::read(directory.join("flake.lock"))?
    {
        bail!("private.path: flake.lock must be committed and unchanged");
    }
    let mut content = Sha256::new();
    content.update(b"dotfiles-tracked-source-v1\0");
    let mut files = Vec::new();
    for relative in entries {
        check_parents(directory, &relative)?;
        let path = directory.join(&relative);
        let entry = match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => Entry::Missing,
            Err(error) => return Err(error).context("tracked input cannot be inspected"),
            Ok(metadata) if metadata.file_type().is_symlink() => Entry::Link(fs::read_link(&path)?),
            Ok(metadata) if metadata.is_file() => Entry::File(
                fs::read(&path).context("tracked input cannot be read")?,
                metadata.permissions().mode() & 0o777,
            ),
            Ok(_) => bail!("tracked input must be a regular file or symbolic link"),
        };
        // A deleted tracked path contributes through the index to verification,
        // but is absent from the source identity before and after its commit.
        if matches!(entry, Entry::Missing) {
            files.push((relative, entry));
            continue;
        }
        let name = relative.as_os_str().as_encoded_bytes();
        content.update((name.len() as u64).to_be_bytes());
        content.update(name);
        match &entry {
            Entry::Missing => content.update(b"missing\0"),
            Entry::File(bytes, mode) => {
                content.update(b"file\0");
                content.update(mode.to_be_bytes());
                content.update(Sha256::digest(bytes));
            }
            Entry::Link(target) => {
                content.update(b"link\0");
                content.update(Sha256::digest(target.as_os_str().as_encoded_bytes()));
            }
        }
        files.push((relative, entry));
    }
    let content_fingerprint = format!("{:x}", content.finalize());
    let mut fingerprint = Sha256::new();
    fingerprint.update(b"dotfiles-working-tree-v1\0");
    fingerprint.update(&head);
    fingerprint.update(&index);
    fingerprint.update(content_fingerprint.as_bytes());
    let mut revision = String::from_utf8(head)?.trim().to_owned();
    if !git(
        directory,
        &["status", "--porcelain", "--untracked-files=no"],
    )?
    .is_empty()
    {
        revision.push_str("-dirty");
    }
    Ok(Observation {
        fingerprint: format!("{:x}", fingerprint.finalize()),
        content_fingerprint,
        revision,
        files,
    })
}

fn frozen_fingerprint(root: &Path) -> Result<String> {
    fn walk(root: &Path, directory: &Path, digest: &mut Sha256) -> Result<()> {
        let mut entries = fs::read_dir(root.join(directory))?.collect::<io::Result<Vec<_>>>()?;
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let relative = directory.join(entry.file_name());
            let name = relative.as_os_str().as_encoded_bytes();
            digest.update((name.len() as u64).to_be_bytes());
            digest.update(name);
            let metadata = fs::symlink_metadata(entry.path())?;
            if metadata.file_type().is_symlink() {
                digest.update(b"link\0");
                digest.update(Sha256::digest(
                    fs::read_link(entry.path())?.as_os_str().as_encoded_bytes(),
                ));
            } else if metadata.is_dir() {
                digest.update(b"directory\0");
                walk(root, &relative, digest)?;
            } else if metadata.is_file() {
                digest.update(b"file\0");
                digest.update((metadata.permissions().mode() & 0o777).to_be_bytes());
                digest.update(Sha256::digest(fs::read(entry.path())?));
            } else {
                bail!("unsupported frozen input type");
            }
        }
        Ok(())
    }
    if !fs::symlink_metadata(root)?.is_dir() {
        bail!("frozen source is not a real directory");
    }
    let mut digest = Sha256::new();
    walk(root, Path::new(""), &mut digest)?;
    Ok(format!("{:x}", digest.finalize()))
}

fn check_parents(root: &Path, relative: &Path) -> Result<()> {
    let mut path = root.to_owned();
    let parts: Vec<_> = relative.components().collect();
    for part in &parts[..parts.len() - 1] {
        path.push(part);
        match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_dir() => (),
            Err(error) if error.kind() == io::ErrorKind::NotFound => break,
            Err(error) => return Err(error).context("cannot inspect tracked input parent"),
            Ok(_) => bail!("tracked input parent is not a real directory"),
        }
    }
    Ok(())
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .arg("--no-optional-locks")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .context("Git input inspection failed")?;
    if !output.status.success() {
        bail!("Git input inspection failed");
    }
    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("flake.nix"), "{}").unwrap();
        fs::write(root.path().join("flake.lock"), "{}").unwrap();
        fs::create_dir(root.path().join("home")).unwrap();
        fs::write(root.path().join("home/owned"), "original").unwrap();
        git(root.path(), &["init", "-q"]).unwrap();
        git(root.path(), &["add", "."]).unwrap();
        commit(root.path());
        root
    }

    fn commit(root: &Path) {
        git(
            root,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "-qm",
                "fixture",
            ],
        )
        .unwrap();
    }

    #[test]
    fn freezes_dirty_tracked_bytes_modes_links_and_omits_untracked_or_deleted_files() {
        let root = fixture();
        fs::write(root.path().join("home/new"), "tracked addition").unwrap();
        fs::write(root.path().join("home/deleted"), "deleted").unwrap();
        symlink("owned", root.path().join("home/link")).unwrap();
        git(root.path(), &["add", "home"]).unwrap();
        fs::remove_file(root.path().join("home/deleted")).unwrap();
        fs::write(root.path().join("home/owned"), "dirty").unwrap();
        fs::set_permissions(
            root.path().join("home/owned"),
            fs::Permissions::from_mode(0o751),
        )
        .unwrap();
        fs::write(root.path().join("untracked"), "not included").unwrap();
        let snapshot = Snapshot::capture(root.path(), false).unwrap();
        assert_eq!(snapshot.directory, root.path().canonicalize().unwrap());
        assert!(snapshot.revision.ends_with("-dirty"));
        assert_eq!(
            fs::read(snapshot.source.join("home/owned")).unwrap(),
            b"dirty"
        );
        assert_eq!(
            fs::metadata(snapshot.source.join("home/owned"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o751
        );
        assert_eq!(
            fs::read_link(snapshot.source.join("home/link")).unwrap(),
            Path::new("owned")
        );
        assert!(snapshot.source.join("home/new").is_file());
        assert!(!snapshot.source.join("home/deleted").exists());
        assert!(!snapshot.source.join("untracked").exists());
        snapshot.verify().unwrap();
        fs::write(root.path().join("home/owned"), "changed after preview").unwrap();
        assert!(snapshot.verify().is_err());
        assert_eq!(
            fs::read(snapshot.source.join("home/owned")).unwrap(),
            b"dirty"
        );
    }

    #[test]
    fn private_lock_must_be_committed_and_unchanged() {
        let root = fixture();
        Snapshot::capture(root.path(), true).unwrap();
        fs::write(root.path().join("flake.lock"), "changed").unwrap();
        assert!(Snapshot::capture(root.path(), true).is_err());
        git(root.path(), &["add", "flake.lock"]).unwrap();
        assert!(Snapshot::capture(root.path(), true).is_err());
        Snapshot::capture(root.path(), false).unwrap();
        commit(root.path());
        Snapshot::capture(root.path(), true).unwrap();
    }

    #[test]
    fn content_identity_ignores_commit_progress_but_verification_does_not() {
        let root = fixture();
        let old = Snapshot::capture(root.path(), false).unwrap();
        git(
            root.path(),
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "advance",
            ],
        )
        .unwrap();
        let next = Snapshot::capture(root.path(), false).unwrap();
        assert_eq!(old.content_fingerprint, next.content_fingerprint);
        assert_ne!(old.fingerprint, next.fingerprint);
        assert!(old.verify().is_err());
    }

    #[test]
    fn rejects_tracked_local_settings_and_symlink_ancestors() {
        let root = fixture();
        fs::write(root.path().join("dotfiles.local.toml"), "").unwrap();
        git(root.path(), &["add", "dotfiles.local.toml"]).unwrap();
        assert!(Snapshot::capture(root.path(), false).is_err());
        git(root.path(), &["reset", "-q", "HEAD", "dotfiles.local.toml"]).unwrap();
        let other = tempfile::tempdir().unwrap();
        fs::write(other.path().join("owned"), "outside").unwrap();
        fs::remove_dir_all(root.path().join("home")).unwrap();
        symlink(other.path(), root.path().join("home")).unwrap();
        assert!(Snapshot::capture(root.path(), false).is_err());
    }

    #[test]
    fn committing_a_deletion_preserves_content_identity() {
        let root = fixture();
        fs::remove_file(root.path().join("home/owned")).unwrap();
        let before = Snapshot::capture(root.path(), false).unwrap();
        git(root.path(), &["add", "home/owned"]).unwrap();
        commit(root.path());
        let after = Snapshot::capture(root.path(), false).unwrap();
        assert_eq!(before.content_fingerprint, after.content_fingerprint);
        assert!(before.verify().is_err());
    }

    #[test]
    fn detects_tampering_with_the_frozen_snapshot() {
        let root = fixture();
        let snapshot = Snapshot::capture(root.path(), false).unwrap();
        fs::write(snapshot.source.join("home/owned"), "tampered").unwrap();
        assert!(snapshot.verify().is_err());
        let snapshot = Snapshot::capture(root.path(), false).unwrap();
        fs::create_dir(snapshot.source.join("unexpected-directory")).unwrap();
        assert!(snapshot.verify().is_err());
    }

    #[test]
    fn private_snapshot_remains_frozen_and_detects_later_changes() {
        let root = fixture();
        let snapshot = Snapshot::capture(root.path(), true).unwrap();
        fs::write(root.path().join("flake.lock"), "changed").unwrap();
        assert!(snapshot.verify().is_err());
        assert_eq!(fs::read(snapshot.source.join("flake.lock")).unwrap(), b"{}");
    }
}
