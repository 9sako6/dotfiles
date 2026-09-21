use std::collections::BTreeMap;
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
use crate::progress::Progress;
use crate::settings::Setting;

#[derive(Clone, Copy)]
pub enum Mode {
    Plan,
    Apply,
}

enum Review {
    Finished,
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
    copy: BTreeMap<String, String>,
    private: Private,
}

#[derive(Deserialize)]
struct Private {
    path: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuildResult {
    drv_path: String,
    outputs: BuildOutputs,
}

#[derive(Deserialize)]
struct BuildOutputs {
    out: PathBuf,
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
    let progress = Progress::start("Preparing configuration");
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
    let previous_generation = current_generation(Path::new("/run/current-system"))?;
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
    let mut retain = nix_command(&nix);
    retain
        .args([
            "build",
            "--offline",
            "--no-write-lock-file",
            "--no-update-lock-file",
            "--out-link",
        ])
        .arg(workspace.path().join("input"))
        .arg(host.trim())
        .arg(&public.source);
    if let Some(private) = &private {
        retain.arg(&private.source);
    }
    capture(&mut retain, "cannot retain frozen inputs")?;
    drop(progress);
    let progress = Progress::start("Checking changes");
    let inventory = evaluate_json(
        nix_command(&nix)
            .args([
                "eval",
                "--raw",
                "--no-write-lock-file",
                "--no-update-lock-file",
            ])
            .arg(format!("{}#inventory.text", host.trim())),
        "cannot evaluate managed resources",
        show_trace,
    )?;
    let mut preview =
        crate::inventory::Preview::from_inventory(previous_generation.as_deref(), inventory)?;
    let mut derivations = None;
    let mut system = None;
    let copy_plan = home_copy::plan(&public.source, &home, &configuration.copy)?;
    if preview.needs_native() || preview.needs_generation_comparison() {
        let [system_drv, brewfile_drv] = host_derivations(&nix, host.trim(), show_trace)?;
        if preview.needs_native() {
            let built = build(&nix, &system_drv, &workspace.path().join("system"))?;
            preview = crate::inventory::Preview::load(previous_generation.as_deref(), &built)?;
            let brewfile = build(&nix, &brewfile_drv, &workspace.path().join("brewfile"))?;
            let mut diagnostics = io::stderr();
            preview.native = String::from_utf8(capture_with_diagnostics(
                Command::new(&backend)
                    .arg("preview")
                    .arg(&nix)
                    .arg(&built)
                    .arg(&brewfile)
                    .arg(
                        previous_generation
                            .as_deref()
                            .unwrap_or(&workspace.path().join("no-active-generation")),
                    ),
                "system preview failed",
                Some(&mut diagnostics),
            )?)?;
            system = Some(built);
        } else {
            preview.compare_generation(
                previous_generation.as_deref(),
                &system_drv.outputs.out,
                &public.revision,
            )?;
        }
        derivations = Some([system_drv, brewfile_drv]);
    }
    preview.copy_changes = copy_plan.changes()?;
    drop(progress);
    if let Review::Finished = review_plan(mode, preview)? {
        return Ok(ExitCode::SUCCESS);
    }
    let verify = || -> Result<()> {
        public.verify()?;
        if let Some(private) = &private {
            private.verify()?;
        }
        verify_local(&local_path, &local)?;
        if selected_target(selection)? != previous {
            bail!("source record changed after preview; nothing was activated");
        }
        if current_generation(Path::new("/run/current-system"))? != previous_generation {
            bail!(
                "active generation changed after preview; nothing was activated. Run plan/apply again"
            );
        }
        Ok(())
    };
    verify()?;
    let system = match system {
        Some(system) => system,
        None => {
            let progress = Progress::start("Evaluating system");
            let [system_drv, _] = match derivations {
                Some(derivations) => derivations,
                None => host_derivations(&nix, host.trim(), show_trace)?,
            };
            drop(progress);
            let _progress = Progress::start("Building system");
            build(&nix, &system_drv, &workspace.path().join("system"))?
        }
    };
    verify()?;
    let paths = workspace.path().join("copy.json");
    fs::write(&paths, serde_json::to_vec(&configuration.copy)?)?;
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

fn current_generation(path: &Path) -> Result<Option<PathBuf>> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).context("cannot inspect active generation"),
        Ok(_) => path
            .canonicalize()
            .map(Some)
            .context("cannot resolve active generation"),
    }
}

pub struct InventoryInputs {
    local: Option<Vec<u8>>,
    nix: PathBuf,
    private: Private,
    public: Snapshot,
    root: PathBuf,
    workspace: tempfile::TempDir,
}

#[derive(Deserialize)]
struct Inspection {
    private: Private,
    settings: Vec<Setting>,
}

pub fn load_settings(root: &Path) -> Result<(Vec<Setting>, InventoryInputs)> {
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
    let inspection: Inspection =
        evaluate_configuration(&nix, &public.source, &manifest, "inspection", false)?;
    public.verify()?;
    verify_local(&local_path, &local)?;
    Ok((
        inspection.settings,
        InventoryInputs {
            local,
            nix,
            private: inspection.private,
            public,
            root: root.to_owned(),
            workspace,
        },
    ))
}

impl InventoryInputs {
    pub fn load<T: serde::de::DeserializeOwned>(self) -> Result<(T, PathBuf)> {
        let Self {
            local,
            nix,
            private,
            public,
            root,
            workspace,
        } = self;
        let user = String::from_utf8(capture(
            Command::new("/usr/bin/id").arg("-un"),
            "cannot identify login user",
        )?)?
        .trim()
        .to_owned();
        let local_path = root.join("dotfiles.local.toml");
        let frozen_local = local.as_ref().map(|_| PathBuf::from("dotfiles.local.toml"));
        let manifest = workspace.path().join("inputs.json");
        let mut inputs = Inputs {
            directory: root.to_owned(),
            local_file: frozen_local,
            private_flake: None,
            public_flake: public.reference.clone(),
            public_revision: public.revision.clone(),
            public_source: public.source.clone(),
            user,
        };
        let private = private
            .path
            .as_ref()
            .map(|path| Snapshot::capture(&nix, &root.join(path), true))
            .transpose()?;
        inputs.private_flake = private.as_ref().map(|snapshot| snapshot.reference.clone());
        fs::write(&manifest, serde_json::to_vec(&inputs)?)?;
        fs::copy(
            public.source.join("nix/host-flake.nix"),
            workspace.path().join("flake.nix"),
        )?;
        let host = String::from_utf8(capture(
            nix_command(&nix)
                .args(["store", "add-path", "--name", "source"])
                .arg(workspace.path()),
            "cannot freeze inventory inputs",
        )?)?;
        let builds: Vec<serde_json::Value> = evaluate_json(
            nix_command(&nix)
                .args([
                    "build",
                    "--no-link",
                    "--json",
                    "--no-substitute",
                    "--option",
                    "builders",
                    "",
                    "--no-write-lock-file",
                    "--no-update-lock-file",
                ])
                .arg(format!("{}#inventory", host.trim())),
            "cannot evaluate managed resources",
            false,
        )?;
        let path = builds
            .first()
            .and_then(|build| build["outputs"]["out"].as_str())
            .context("Nix did not return the inventory data path")?;
        let inventory =
            serde_json::from_slice(&fs::read(path).context("cannot read inventory data")?)
                .context("Nix returned invalid inventory data")?;
        public.verify()?;
        if let Some(private) = &private {
            private.verify()?;
        }
        verify_local(&local_path, &local)?;
        Ok((inventory, public.source))
    }
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

fn review_plan(mode: Mode, preview: crate::inventory::Preview) -> Result<Review> {
    if !preview.has_changes() {
        return Ok(Review::Finished);
    }
    preview.show()?;
    if matches!(mode, Mode::Plan) {
        return Ok(Review::Finished);
    }
    confirm_apply(&mut io::stdin().lock(), &mut io::stdout().lock())?;
    Ok(Review::Apply)
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
                "--no-substitute",
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
    let mut stderr = io::stderr();
    let diagnostics = show_trace.then_some(&mut stderr as &mut dyn Write);
    let bytes = capture_with_diagnostics(command, &error, diagnostics)?;
    serde_json::from_slice(&bytes).context("Nix returned invalid JSON")
}

fn build(nix: &Path, derivation: &BuildResult, link: &Path) -> Result<PathBuf> {
    if derivation.outputs.out.exists() {
        let output = nix_command(nix)
            .args(["build", "--offline", "--out-link"])
            .arg(link)
            .arg(&derivation.outputs.out)
            .output()?;
        if output.status.success() {
            return link
                .canonicalize()
                .context("Nix did not retain the requested output");
        }
    }
    let output = nix_command(nix)
        .arg("build")
        .arg("--out-link")
        .arg(link)
        .arg(format!("{}^*", derivation.drv_path))
        .output()?;
    if !output.status.success() {
        io::stderr().write_all(&output.stderr)?;
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
    fn build_fixture() {
        let Some(root) = env::var_os("DOTFILES_TEST_BUILD_ROOT").map(PathBuf::from) else {
            return;
        };
        let derivation =
            serde_json::from_slice(&fs::read(root.join("build.json")).unwrap()).unwrap();
        build(Path::new("nix"), &derivation, &root.join("result")).unwrap();
    }

    #[test]
    fn review_fixture() {
        let Some(root) = env::var_os("DOTFILES_TEST_REVIEW_ROOT").map(PathBuf::from) else {
            return;
        };
        if env::var_os("DOTFILES_TEST_REVIEW_PROGRESS").is_some() {
            let _progress = Progress::start("Evaluating system");
            evaluate_json::<serde_json::Value>(
                Command::new("/bin/sh").args(["-c", "sleep 11; printf '{}' "]),
                "evaluation failed",
                false,
            )
            .unwrap();
        }
        let mut preview =
            crate::inventory::Preview::load(Some(&root.join("before")), &root.join("after"))
                .unwrap();
        if root.join("copy.json").exists() {
            let paths: BTreeMap<String, String> =
                serde_json::from_slice(&fs::read(root.join("copy.json")).unwrap()).unwrap();
            preview.copy_changes =
                home_copy::plan(&root.join("source"), &root.join("home"), &paths)
                    .unwrap()
                    .changes()
                    .unwrap();
        }
        let mode = if env::var_os("DOTFILES_TEST_REVIEW_APPLY").is_some() {
            Mode::Apply
        } else {
            Mode::Plan
        };
        let code = match review_plan(mode, preview) {
            Ok(Review::Finished) => 0,
            Ok(Review::Apply) => {
                fs::write(root.join("activation-requested"), "yes").unwrap();
                0
            }
            Err(error) => {
                eprintln!("{error:#}");
                1
            }
        };
        std::process::exit(code);
    }

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
    fn active_generation_tracks_switches_and_rejects_broken_links() {
        let root = tempfile::tempdir().unwrap();
        let link = root.path().join("current-system");
        assert_eq!(current_generation(&link).unwrap(), None);
        for name in ["old", "new"] {
            let generation = root.path().join(name);
            fs::create_dir(&generation).unwrap();
            let _ = fs::remove_file(&link);
            std::os::unix::fs::symlink(&generation, &link).unwrap();
            assert_eq!(
                current_generation(&link).unwrap(),
                Some(generation.canonicalize().unwrap())
            );
        }
        fs::remove_dir(root.path().join("new")).unwrap();
        assert!(current_generation(&link).is_err());
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
