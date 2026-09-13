use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::home_copy;
use crate::settings::Setting;

#[derive(Clone, Copy)]
pub enum Mode {
    Plan,
    Apply,
}

#[derive(Deserialize)]
struct Metadata {
    path: PathBuf,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Inputs {
    directory: PathBuf,
    local_file: Option<PathBuf>,
    private_source: Option<PathBuf>,
    public_revision: String,
    public_source: PathBuf,
    user: String,
}

#[derive(Deserialize)]
struct ConfigurationResult<T> {
    errors: Vec<String>,
    config: Option<T>,
}

#[derive(Deserialize)]
struct Configuration {
    copy: Vec<String>,
    localllm: LocalLlm,
    private: Private,
}

#[derive(Deserialize)]
struct Private {
    path: Option<String>,
}

#[derive(Deserialize)]
struct LocalLlm {
    enabled: bool,
}

#[derive(Deserialize)]
struct Outputs {
    brewfile: String,
    system: String,
}

struct Snapshot {
    directory: PathBuf,
    fingerprint: String,
    source: PathBuf,
    revision: String,
}

impl Snapshot {
    fn capture(nix: &Path, directory: &Path, private: bool) -> Result<Self> {
        let directory = directory
            .canonicalize()
            .context("private.path: checkout does not exist")?;
        for file in ["flake.nix", "flake.lock"] {
            if git(&directory, &["ls-files", "--error-unmatch", file]).is_err()
                || !directory.join(file).is_file()
            {
                bail!("checkout requires tracked flake.nix and flake.lock");
            }
        }
        if !git(&directory, &["ls-files", "dotfiles.local.toml"])?.is_empty() {
            bail!("dotfiles.local.toml must remain untracked; remove it from the Git index");
        }
        if private
            && git(&directory, &["show", "HEAD:flake.lock"])?
                != fs::read(directory.join("flake.lock"))?
        {
            bail!("private.path: flake.lock must be committed and unchanged");
        }
        let before = fingerprint(&directory)?;
        let reference = format!(
            "git+{}",
            url::Url::from_directory_path(&directory)
                .map_err(|_| anyhow::anyhow!("invalid checkout path"))?
        );
        let metadata: Metadata = serde_json::from_slice(&capture(
            nix_command(nix).args(["flake", "metadata", "--json", "--no-write-lock-file", "--no-update-lock-file", &reference]),
            "Git snapshot failed; check that flake.lock is complete and its pinned inputs are available",
        )?)?;
        if before != fingerprint(&directory)? {
            bail!("inputs changed while taking a snapshot; run plan again");
        }
        let revision = String::from_utf8(git(&directory, &["rev-parse", "HEAD"])?)?
            .trim()
            .to_owned();
        let revision = if git(
            &directory,
            &["status", "--porcelain", "--untracked-files=no"],
        )?
        .is_empty()
        {
            revision
        } else {
            format!("{revision}-dirty")
        };
        Ok(Self {
            directory,
            fingerprint: before,
            source: metadata.path,
            revision,
        })
    }

    fn verify(&self) -> Result<()> {
        if self.fingerprint != fingerprint(&self.directory)? {
            bail!("inputs changed after preview; nothing was activated. Run plan/apply again");
        }
        Ok(())
    }
}

pub fn run(mode: Mode, root: &Path) -> Result<ExitCode> {
    let user = String::from_utf8(capture(
        Command::new("/usr/bin/id").arg("-un"),
        "cannot identify login user",
    )?)?
    .trim()
    .to_owned();
    let uid = String::from_utf8(capture(
        Command::new("/usr/bin/id").arg("-u"),
        "cannot identify login user",
    )?)?
    .trim()
    .to_owned();
    if uid == "0" {
        bail!("run system commands as the login user");
    }
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    let selection = Path::new("/etc/nix-darwin/flake.nix");
    let previous = selected_target(selection)?;
    validate_record(root, previous.as_deref(), &home)?;
    let local_path = root.join("dotfiles.local.toml");
    let local = read_local(&local_path)?;
    migration_guard(root, previous.as_deref(), local.is_some())?;
    let _lock = if matches!(mode, Mode::Apply) {
        Some(acquire_lock(
            &env::temp_dir().join(format!("dotfiles-{uid}-apply.lock")),
        )?)
    } else {
        None
    };
    let backend = root.join("bin/system-backend.sh");
    let nix = resolve_nix(root, matches!(mode, Mode::Apply))?;
    let workspace = tempfile::Builder::new()
        .prefix("dotfiles-input-")
        .tempdir()?;
    let frozen_local = freeze_local(workspace.path(), &local)?;
    let public = Snapshot::capture(&nix, root, false)?;
    let mut inputs = Inputs {
        directory: root.to_path_buf(),
        local_file: frozen_local,
        private_source: None,
        public_revision: public.revision.clone(),
        public_source: public.source.clone(),
        user: user.clone(),
    };
    let manifest = workspace.path().join("inputs.json");
    fs::write(&manifest, serde_json::to_vec(&inputs)?)?;
    let configuration: Configuration =
        evaluate_configuration(&nix, &public.source, &manifest, "configuration")?;
    let private = configuration.private.path.as_ref().map(|path| -> Result<Snapshot> {
        let directory = root.join(path).canonicalize().context("dotfiles.local.toml: private.path: checkout does not exist")?;
        let data = env::var_os("XDG_DATA_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home.join(".local/share"));
        if directory.starts_with(data.join("dotfiles/nix-darwin")) {
            bail!("private.path must use an independent checkout, not the legacy auto-sync cache; see docs/operations.md");
        }
        Snapshot::capture(&nix, &directory, true)
    }).transpose()?;
    if let Some(private) = &private {
        inputs.private_source = Some(private.source.clone());
    }
    if previous
        .as_ref()
        .is_some_and(|p| !is_public_record(root, p))
        && private.is_none()
    {
        bail!("legacy private source is still active; configure private.path before migrating. See docs/operations.md");
    }
    if previous
        .as_ref()
        .is_some_and(|p| !is_public_record(root, p))
        && configuration.localllm.enabled
    {
        bail!("migrate the private configuration with localllm.enabled = false first; see docs/operations.md");
    }
    fs::write(&manifest, serde_json::to_vec(&inputs)?)?;
    public.verify()?;
    if let Some(private) = &private {
        private.verify()?;
    }
    verify_local(&local_path, &local)?;
    println!("public revision: {}", public.revision);
    if let Some(private) = &private {
        println!("private revision: {}", private.revision);
    }
    println!(
        "local input hash: {}",
        local
            .as_ref()
            .map(|b| hash(b))
            .unwrap_or_else(|| "absent".into())
    );
    if configuration.localllm.enabled {
        println!("Local LLM is enabled: the first build downloads approximately 16 GB of pinned model data and its runtime.");
    }
    let outputs: Outputs = evaluate(&nix, &public.source, &manifest, "outputs")?;
    let copy_plan = home_copy::plan(&public.source, &home, &configuration.copy)?;
    let system = build(&nix, &outputs.system, &workspace.path().join("system"))?;
    let brewfile = build(&nix, &outputs.brewfile, &workspace.path().join("brewfile"))?;
    let status = Command::new(&backend)
        .arg("preview")
        .arg(&nix)
        .arg(&system)
        .arg(&brewfile)
        .status()?;
    if !status.success() {
        bail!("system preview failed");
    }
    println!("{}", copy_plan.preview());
    if matches!(mode, Mode::Plan) {
        return Ok(ExitCode::SUCCESS);
    }
    confirm_apply(&mut io::stdin().lock(), &mut io::stdout().lock())?;
    public.verify()?;
    if let Some(private) = &private {
        private.verify()?;
    }
    verify_local(&local_path, &local)?;
    if selected_target(selection)? != previous {
        bail!("source record changed after preview; nothing was activated");
    }
    let paths = workspace.path().join("copy.json");
    fs::write(&paths, serde_json::to_vec(&configuration.copy)?)?;
    let previous_generation = Path::new("/run/current-system").canonicalize().ok();
    let status = Command::new(&backend)
        .arg("activate")
        .arg(&nix)
        .arg(&user)
        .arg(&system)
        .arg(previous.as_deref().unwrap_or_else(|| Path::new("missing")))
        .arg(root.join("flake.nix"))
        .arg(env::current_exe()?)
        .arg(&public.source)
        .arg(&paths)
        .arg(&home)
        .status()?;
    if !status.success() {
        if let Some(previous) = previous_generation {
            eprintln!("Restore the previous profile: sudo nix-env -p /nix/var/nix/profiles/system --set {}", previous.display());
            eprintln!(
                "Reactivate it: sudo {}/sw/bin/darwin-rebuild activate",
                previous.display()
            );
        }
        bail!("activation/copy failed; the previous source record is retained. The system may be partially changed. Run: sudo darwin-rebuild switch --rollback. See docs/operations.md for profile and Homebrew recovery");
    }
    Ok(ExitCode::SUCCESS)
}

pub fn load_settings(root: &Path) -> Result<Vec<Setting>> {
    let nix = resolve_nix(root, false)?;
    let local_path = root.join("dotfiles.local.toml");
    let local = read_local(&local_path)?;
    let workspace = tempfile::Builder::new()
        .prefix("dotfiles-settings-")
        .tempdir()?;
    let frozen_local = freeze_local(workspace.path(), &local)?;
    let public = Snapshot::capture(&nix, root, false)?;
    let manifest = workspace.path().join("inputs.json");
    fs::write(
        &manifest,
        serde_json::to_vec(&serde_json::json!({
            "localFile": frozen_local,
            "publicSource": public.source,
        }))?,
    )?;
    let settings = evaluate_configuration(&nix, &public.source, &manifest, "settings")?;
    public.verify()?;
    verify_local(&local_path, &local)?;
    Ok(settings)
}

fn resolve_nix(root: &Path, install: bool) -> Result<PathBuf> {
    Ok(PathBuf::from(
        String::from_utf8(capture(
            Command::new(root.join("bin/system-backend.sh")).arg(if install {
                "ensure-nix"
            } else {
                "require-nix"
            }),
            "Lix is unavailable; install it with bin/install-lix.sh",
        )?)?
        .trim(),
    ))
}

fn freeze_local(workspace: &Path, local: &Option<Vec<u8>>) -> Result<Option<PathBuf>> {
    local
        .as_ref()
        .map(|bytes| -> Result<PathBuf> {
            let path = workspace.join("dotfiles.local.toml");
            fs::write(&path, bytes)?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            Ok(path)
        })
        .transpose()
}

fn evaluate_configuration<T: serde::de::DeserializeOwned>(
    nix: &Path,
    source: &Path,
    manifest: &Path,
    operation: &str,
) -> Result<T> {
    let parsed: ConfigurationResult<T> = evaluate(nix, source, manifest, operation)?;
    if !parsed.errors.is_empty() {
        bail!("{}", parsed.errors.join("\n"));
    }
    parsed
        .config
        .context("configuration was not returned by Nix")
}

fn confirm_apply(input: &mut impl io::BufRead, output: &mut impl Write) -> Result<()> {
    write!(output, "Apply this system plan? Type yes: ")?;
    output.flush()?;
    let mut answer = String::new();
    input.read_line(&mut answer)?;
    if answer.trim() != "yes" {
        bail!("system apply cancelled");
    }
    Ok(())
}

fn nix_command(nix: &Path) -> Command {
    let mut command = Command::new(nix);
    command.args(["--extra-experimental-features", "nix-command flakes"]);
    command
}

fn evaluate<T: serde::de::DeserializeOwned>(
    nix: &Path,
    source: &Path,
    manifest: &Path,
    operation: &str,
) -> Result<T> {
    let bytes = capture(
        nix_command(nix)
            .args([
                "eval",
                "--impure",
                "--json",
                "--no-write-lock-file",
                "--no-update-lock-file",
                "--file",
            ])
            .arg(source.join("nix/host-input.nix"))
            .env("DOTFILES_INPUT_MANIFEST", manifest)
            .env("DOTFILES_INPUT_OPERATION", operation),
        if matches!(operation, "configuration" | "settings") {
            "dotfiles.toml / dotfiles.local.toml: invalid TOML or unreadable configuration (source values omitted)"
        } else {
            "Nix host composition failed: check private darwinModules.default, locked dependencies, option conflicts and package compatibility (private evaluation output omitted)"
        },
    )?;
    serde_json::from_slice(&bytes).context("Nix returned invalid JSON")
}

fn build(nix: &Path, derivation: &str, link: &Path) -> Result<PathBuf> {
    let status = nix_command(nix)
        .arg("build")
        .arg("--out-link")
        .arg(link)
        .arg(format!("{derivation}^*"))
        .status()?;
    if !status.success() {
        bail!("building the frozen generation failed");
    }
    link.canonicalize()
        .context("Nix did not produce the requested output")
}

fn capture(command: &mut Command, error: &str) -> Result<Vec<u8>> {
    let output = command.output().with_context(|| error.to_owned())?;
    if !output.status.success() {
        bail!("{error}");
    }
    Ok(output.stdout)
}

fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    capture(
        Command::new("git").arg("-C").arg(root).args(args),
        "Git input inspection failed",
    )
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn fingerprint(root: &Path) -> Result<String> {
    let mut digest = Sha256::new();
    digest.update(git(root, &["rev-parse", "HEAD"])?);
    digest.update(git(root, &["ls-files", "--stage", "-z"])?);
    for name in git(root, &["ls-files", "-z"])?
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
    {
        let name = std::str::from_utf8(name).context("tracked paths must be UTF-8")?;
        let path = root.join(name);
        digest.update(name.as_bytes());
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                digest.update(meta.permissions().mode().to_be_bytes());
                let bytes = if meta.file_type().is_symlink() {
                    fs::read_link(&path)?
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else {
                    fs::read(&path).context("tracked input cannot be read")?
                };
                digest.update(Sha256::digest(bytes));
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => digest.update(b"deleted"),
            Err(e) => return Err(e).context("tracked input cannot be inspected"),
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn read_local(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(_) => bail!("dotfiles.local.toml: cannot inspect local configuration"),
        Ok(_) => fs::read(path)
            .map(Some)
            .context("dotfiles.local.toml: cannot read local configuration"),
    }
}

fn verify_local(path: &Path, expected: &Option<Vec<u8>>) -> Result<()> {
    if &read_local(path)? != expected {
        bail!("dotfiles.local.toml changed after snapshot; run plan/apply again");
    }
    Ok(())
}

fn selected_target(path: &Path) -> Result<Option<PathBuf>> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e).context("cannot inspect source record"),
        Ok(meta) if meta.file_type().is_symlink() => Ok(Some(fs::read_link(path)?)),
        Ok(_) => bail!("source record is not a symlink; refusing to replace it"),
    }
}

fn is_public_record(root: &Path, target: &Path) -> bool {
    target == root.join("flake.nix") || target == root.join("darwin/flake.nix")
}

fn validate_record(root: &Path, previous: Option<&Path>, home: &Path) -> Result<()> {
    let Some(target) = previous else {
        return Ok(());
    };
    if is_public_record(root, target) {
        return Ok(());
    }
    let data = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".local/share"));
    let checkout = target.parent().context("invalid source record")?;
    if target.file_name().and_then(|s| s.to_str()) != Some("flake.nix")
        || checkout.parent() != Some(data.join("dotfiles/nix-darwin").as_path())
    {
        bail!("source record is not managed by dotfiles; refusing to replace it");
    }
    let origin = git(checkout, &["remote", "get-url", "origin"])?;
    let expected = hash(String::from_utf8(origin)?.trim().as_bytes());
    if checkout.file_name().and_then(|s| s.to_str()) != Some(&expected[..24]) {
        bail!("legacy source record does not match its checkout origin");
    }
    Ok(())
}

fn migration_guard(root: &Path, previous: Option<&Path>, has_local: bool) -> Result<()> {
    if previous.is_some_and(|p| !is_public_record(root, p)) && !has_local {
        bail!("legacy private source requires migration: create dotfiles.local.toml with private.path pointing to an independent module checkout; see docs/operations.md. The active configuration is retained");
    }
    Ok(())
}

fn acquire_lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    fs2::FileExt::try_lock_exclusive(&file).context("system apply is already running")?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_requires_explicit_yes() {
        assert!(confirm_apply(&mut b"yes\n".as_slice(), &mut Vec::new()).is_ok());
        for input in ["", "y\n", "no\n"] {
            assert!(confirm_apply(&mut input.as_bytes(), &mut Vec::new()).is_err());
        }
    }

    #[test]
    fn legacy_private_without_local_stops() {
        assert!(migration_guard(
            Path::new("/public"),
            Some(Path::new("/old/flake.nix")),
            false
        )
        .is_err());
        assert!(migration_guard(Path::new("/public"), None, false).is_ok());
    }

    #[test]
    fn local_changes_and_unreadable_files_stop() {
        let temp = tempfile::tempdir().unwrap();
        let file = temp.path().join("dotfiles.local.toml");
        let before = read_local(&file).unwrap();
        fs::write(&file, "[localllm]\nenabled = false").unwrap();
        assert!(verify_local(&file, &before).is_err());
        fs::remove_file(&file).unwrap();
        fs::create_dir(&file).unwrap();
        assert!(read_local(&file).is_err());
    }

    #[test]
    fn lock_excludes_another_apply_and_releases_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("apply.lock");
        let first = acquire_lock(&path).unwrap();
        assert!(acquire_lock(&path).is_err());
        drop(first);
        assert!(acquire_lock(&path).is_ok());
    }
}
