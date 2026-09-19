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
    locked: LockedSource,
    path: PathBuf,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LockedSource {
    nar_hash: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Inputs {
    directory: PathBuf,
    local_file: Option<PathBuf>,
    private_flake: Option<String>,
    public_flake: String,
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
#[serde(rename_all = "camelCase")]
struct BuildResult {
    drv_path: String,
}

struct Snapshot {
    directory: PathBuf,
    fingerprint: String,
    reference: String,
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
        let mut reference = url::Url::parse(&format!("path:{}", metadata.path.display()))?;
        reference
            .query_pairs_mut()
            .append_pair("narHash", &metadata.locked.nar_hash);
        Ok(Self {
            directory,
            fingerprint: before,
            reference: reference.into(),
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

pub fn run(mode: Mode, root: &Path, show_trace: bool) -> Result<ExitCode> {
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
    validate_record(root, previous.as_deref())?;
    let local_path = root.join("dotfiles.local.toml");
    let local = read_local(&local_path)?;
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
        private_flake: None,
        public_flake: public.reference.clone(),
        public_revision: public.revision.clone(),
        public_source: public.source.clone(),
        user: user.clone(),
    };
    let manifest = workspace.path().join("inputs.json");
    fs::write(&manifest, serde_json::to_vec(&inputs)?)?;
    let configuration: Configuration =
        evaluate_configuration(&nix, &public.source, &manifest, "configuration", show_trace)?;
    let private = configuration
        .private
        .path
        .as_ref()
        .map(|path| -> Result<Snapshot> {
            let directory = root
                .join(path)
                .canonicalize()
                .context("dotfiles.local.toml: private.path: checkout does not exist")?;
            Snapshot::capture(&nix, &directory, true)
        })
        .transpose()?;
    if let Some(private) = &private {
        inputs.private_flake = Some(private.reference.clone());
    }
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
        println!("Local LLM is enabled: the first build may download several GB of pinned model data and its runtime.");
    }
    inputs.local_file = inputs
        .local_file
        .map(|_| PathBuf::from("dotfiles.local.toml"));
    fs::write(&manifest, serde_json::to_vec(&inputs)?)?;
    fs::copy(
        public.source.join("nix/host-flake.nix"),
        workspace.path().join("flake.nix"),
    )?;
    let host = String::from_utf8(capture(
        nix_command(&nix)
            .args(["store", "add-path", "--name", "source"])
            .arg(workspace.path()),
        "cannot freeze host evaluation inputs",
    )?)?;
    let [system_drv, brewfile_drv] = host_derivations(&nix, host.trim(), show_trace)?;
    let copy_plan = home_copy::plan(&public.source, &home, &configuration.copy)?;
    let system = build(&nix, &system_drv.drv_path, &workspace.path().join("system"))?;
    let brewfile = build(
        &nix,
        &brewfile_drv.drv_path,
        &workspace.path().join("brewfile"),
    )?;
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
    let settings = evaluate_configuration(&nix, &public.source, &manifest, "settings", false)?;
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
    show_trace: bool,
) -> Result<T> {
    let parsed: ConfigurationResult<T> = evaluate(nix, source, manifest, operation, show_trace)?;
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
    show_trace: bool,
) -> Result<T> {
    let mut command = nix_command(nix);
    command
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
        .env("DOTFILES_INPUT_OPERATION", operation);
    evaluate_json(
        &mut command,
        "dotfiles.toml / dotfiles.local.toml: invalid TOML or unreadable configuration",
        show_trace,
    )
}

fn host_derivations(nix: &Path, source: &str, show_trace: bool) -> Result<[BuildResult; 2]> {
    let builds: Vec<BuildResult> = evaluate_json(
        nix_command(nix)
            .args([
                "build",
                "--dry-run",
                "--json",
                "--no-write-lock-file",
                "--no-update-lock-file",
            ])
            .args([format!("{source}#system"), format!("{source}#brewfile")]),
        "Nix host composition failed: check private darwinModules.default, locked dependencies, option conflicts and package compatibility",
        show_trace,
    )?;
    builds
        .try_into()
        .map_err(|_| anyhow::anyhow!("Nix did not return both host derivations"))
}

fn evaluate_json<T: serde::de::DeserializeOwned>(
    command: &mut Command,
    error: &str,
    show_trace: bool,
) -> Result<T> {
    if show_trace {
        command.arg("--show-trace");
    }
    let error = if show_trace {
        error.to_owned()
    } else {
        format!("{error} (details omitted; run dotfiles plan --show-trace to inspect locally)")
    };
    let mut stderr = io::stderr().lock();
    let diagnostics = show_trace.then_some(&mut stderr as &mut dyn Write);
    let bytes = capture_with_diagnostics(command, &error, diagnostics)?;
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
    capture_with_diagnostics(command, error, None)
}

fn capture_with_diagnostics(
    command: &mut Command,
    error: &str,
    diagnostics: Option<&mut dyn Write>,
) -> Result<Vec<u8>> {
    let output = command.output().with_context(|| error.to_owned())?;
    if !output.status.success() {
        if let Some(diagnostics) = diagnostics {
            diagnostics.write_all(&output.stderr)?;
        }
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

fn validate_record(root: &Path, previous: Option<&Path>) -> Result<()> {
    if previous.is_some_and(|target| target != root.join("flake.nix")) {
        bail!("source record does not match this dotfiles checkout; refusing to replace it");
    }
    Ok(())
}

struct ApplyLock {
    file: File,
}

impl Drop for ApplyLock {
    fn drop(&mut self) {
        let _ = fs2::FileExt::unlock(&self.file);
    }
}

fn acquire_lock(path: &Path) -> Result<ApplyLock> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(path)?;
    fs2::FileExt::try_lock_exclusive(&file).context("system apply is already running")?;
    Ok(ApplyLock { file })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_command_diagnostics_require_explicit_output() {
        for show_trace in [false, true] {
            let mut diagnostics = Vec::new();
            let error = capture_with_diagnostics(
                Command::new("/bin/sh").args([
                    "-c",
                    "printf private-stdout; printf private-error >&2; exit 1",
                ]),
                "evaluation failed",
                show_trace.then_some(&mut diagnostics as &mut dyn Write),
            )
            .unwrap_err();
            assert_eq!(error.to_string(), "evaluation failed");
            assert_eq!(
                diagnostics,
                if show_trace {
                    b"private-error".as_slice()
                } else {
                    b""
                }
            );
        }
    }

    #[test]
    fn successful_command_preserves_stdout_without_emitting_diagnostics() {
        let mut diagnostics = Vec::new();
        let output = capture_with_diagnostics(
            Command::new("/bin/sh").args(["-c", "printf result; printf warning >&2"]),
            "evaluation failed",
            Some(&mut diagnostics),
        )
        .unwrap();
        assert_eq!(output, b"result");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn confirmation_requires_explicit_yes() {
        assert!(confirm_apply(&mut b"yes\n".as_slice(), &mut Vec::new()).is_ok());
        for input in ["", "y\n", "no\n"] {
            assert!(confirm_apply(&mut input.as_bytes(), &mut Vec::new()).is_err());
        }
    }

    #[test]
    fn source_record_accepts_this_checkout_or_initial_setup() {
        let root = Path::new("/public");
        assert!(validate_record(root, None).is_ok());
        assert!(validate_record(root, Some(&root.join("flake.nix"))).is_ok());
        assert!(validate_record(root, Some(Path::new("/other/flake.nix"))).is_err());
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

    #[test]
    fn lock_releases_on_drop_while_a_duplicated_descriptor_remains_open() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("apply.lock");
        let first = acquire_lock(&path).unwrap();
        let duplicate = first.file.try_clone().unwrap();
        assert!(acquire_lock(&path).is_err());
        drop(first);
        let second = acquire_lock(&path).unwrap();
        drop(duplicate);
        assert!(acquire_lock(&path).is_err());
        drop(second);
        assert!(acquire_lock(&path).is_ok());
    }
}
