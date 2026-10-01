mod artifacts;
mod configuration;
#[cfg(test)]
mod fast_path_tests;
mod inputs;
mod rootless_inventory;
mod snapshot;
mod tools;
mod user_services;
mod user_settings;

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
#[cfg(test)]
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
#[serde(rename_all = "camelCase")]
struct BuildResult {
    drv_path: String,
    outputs: BuildOutputs,
}

#[derive(Deserialize)]
struct BuildOutputs {
    out: PathBuf,
}

struct Runtime {
    launchctl: PathBuf,
    mise: PathBuf,
    home: PathBuf,
    selection: PathBuf,
    current_generation: PathBuf,
    executable: PathBuf,
    lock: PathBuf,
    confirm: Box<dyn FnOnce() -> Result<()>>,
}

struct PlanInputs {
    public: snapshot::Snapshot,
    private: Option<snapshot::Snapshot>,
    local_path: PathBuf,
    local: Option<Vec<u8>>,
    selection: PathBuf,
    previous: Option<PathBuf>,
    current_generation: PathBuf,
    previous_generation: Option<PathBuf>,
    workspace: tempfile::TempDir,
}

impl PlanInputs {
    fn verify(&self) -> Result<()> {
        self.public.verify()?;
        if let Some(private) = &self.private {
            private.verify()?;
        }
        verify_local(&self.local_path, &self.local)?;
        if selected_target(&self.selection)? != self.previous {
            bail!("source record changed after preview; nothing was activated");
        }
        if current_generation(&self.current_generation)? != self.previous_generation {
            bail!(
                "active generation changed after preview; nothing was activated. Run plan/apply again"
            );
        }
        Ok(())
    }

    fn verify_activated(&self, source_record: &Path, generation: &Path) -> Result<()> {
        self.public.verify()?;
        if let Some(private) = &self.private {
            private.verify()?;
        }
        verify_local(&self.local_path, &self.local)?;
        if selected_target(&self.selection)?.as_deref() != Some(source_record)
            || current_generation(&self.current_generation)?.as_deref() != Some(generation)
        {
            bail!("activated generation or source record changed; user resources were not reconciled. Run plan/apply again");
        }
        Ok(())
    }
}

struct Plan {
    user_settings: Option<user_settings::Plan>,
    artifacts: artifacts::Plan,
    user_services: user_services::Plan,
    tools: tools::Plan,
    inputs: PlanInputs,
    home: home_copy::CopyPlan,
    system: crate::inventory::Preview,
}

impl Plan {
    fn review(&mut self, mode: Mode, confirm: impl FnOnce() -> Result<()>) -> Result<Review> {
        self.inputs.verify()?;
        self.artifacts.verify()?;
        self.system.copy_changes = self.home.changes()?;
        review_plan_with_artifacts(
            mode,
            &self.system,
            Some(&self.tools),
            Some(&self.user_services),
            Some(&self.artifacts),
            self.user_settings.as_ref(),
            confirm,
        )
    }
}

pub fn run(mode: Mode, root: &Path, show_trace: bool) -> Result<ExitCode> {
    run_with(
        mode,
        root,
        show_trace,
        Runtime {
            launchctl: "/bin/launchctl".into(),
            mise: "mise".into(),
            home: PathBuf::from(env::var_os("HOME").context("HOME is not set")?),
            selection: "/etc/nix-darwin/flake.nix".into(),
            current_generation: "/run/current-system".into(),
            executable: env::current_exe()?,
            lock: env::temp_dir().join(format!("dotfiles-{}-apply.lock", unsafe {
                libc::geteuid()
            })),
            confirm: Box::new(|| confirm_apply(&mut io::stdin().lock(), &mut io::stdout().lock())),
        },
    )
}

fn run_with(mode: Mode, root: &Path, show_trace: bool, runtime: Runtime) -> Result<ExitCode> {
    let progress = Progress::start("Preparing configuration");
    let user = String::from_utf8(capture(
        Command::new("/usr/bin/id").arg("-un"),
        "cannot identify login user",
    )?)?
    .trim()
    .to_owned();
    if unsafe { libc::geteuid() } == 0 {
        bail!("run system commands as the login user");
    }
    let _lock = if matches!(mode, Mode::Apply) {
        Some(acquire_lock(&runtime.lock)?)
    } else {
        None
    };
    let home = runtime.home;
    let selection = runtime.selection.as_path();
    let previous = selected_target(selection)?;
    validate_record(root, previous.as_deref())?;
    let previous_generation = current_generation(&runtime.current_generation)?;
    let local_path = root.join("dotfiles.local.toml");
    let local = read_local(&local_path)?;
    let backend = root.join("bin/system-backend.sh");
    let workspace = tempfile::Builder::new()
        .prefix("dotfiles-input-")
        .tempdir()?;
    let public = snapshot::Snapshot::capture(root, false)?;
    let configuration = configuration::Configuration::load(&public.source, local.as_deref())?;
    let private = configuration
        .private
        .path
        .as_ref()
        .map(|path| snapshot::Snapshot::capture(&root.join(path), true))
        .transpose()?;
    let snapshot = PlanInputs {
        public,
        private,
        local_path,
        local,
        selection: runtime.selection,
        previous,
        current_generation: runtime.current_generation,
        previous_generation,
        workspace,
    };
    snapshot.verify()?;
    let public = &snapshot.public;
    let private = &snapshot.private;
    let previous_generation = &snapshot.previous_generation;
    let workspace = &snapshot.workspace;
    let tools = tools::Plan::capture(&public.source, &home, &runtime.mise)?;
    let user_services =
        user_services::Plan::capture(&public.source, &home, &runtime.launchctl, _lock.as_ref())?;
    let system_source = inputs::SystemSource::inspect(&public.source, &configuration.copy)?;
    // Private modules retain their existing configuration interface, so any
    // value they could consume stays system-affecting when private is selected.
    let system_configuration = if private.is_some() {
        Some(serde_json::to_vec(&configuration)?)
    } else {
        None
    };
    let identity = inputs::identity_v2(
        &system_source,
        root,
        &user,
        &home,
        private
            .as_ref()
            .map(|snapshot| snapshot.content_fingerprint.as_str()),
        &system_configuration,
    )?;
    let unchanged_system = snapshot.previous.is_some()
        && inputs::matches_generation(previous_generation.as_deref(), &identity)?;
    let artifact_identity = artifacts::input_identity(&public.source, &configuration.localllm)?;
    let cached_artifacts = artifacts::Plan::cached(
        &public.source,
        &home,
        &configuration.copy,
        &artifact_identity,
    )?;
    let nix = if !unchanged_system || cached_artifacts.is_none() {
        Some(resolve_nix(root)?)
    } else {
        None
    };
    let mut retained = Vec::new();
    let frozen_public = if cached_artifacts.is_none() {
        let path = store_source(
            nix.as_deref().context("artifact Nix is unavailable")?,
            &public.source,
        )?;
        retained.push(path.clone());
        Some(path)
    } else {
        None
    };
    let mut artifacts = match cached_artifacts {
        Some(plan) => plan,
        None => {
            let input = workspace.path().join("artifact-input.json");
            let local_file = freeze_local(workspace.path(), &snapshot.local)?;
            fs::write(
                &input,
                serde_json::to_vec(&serde_json::json!({
                    "publicSource": frozen_public, "localFile": local_file,
                    "privateSource": null
                }))?,
            )?;
            let result = artifacts::Plan::capture(
                nix.as_deref().unwrap(),
                &public.source,
                &input,
                &home,
                &configuration.copy,
                show_trace,
            )?;
            fs::remove_file(input)?;
            result
        }
    };
    artifacts.set_cache_identity(artifact_identity);
    let nightlight = artifacts
        .source_for_id("nightlight")
        .unwrap_or_else(|| home.join(".local/bin/nightlight"));
    let user_settings =
        user_settings::Plan::capture(&public.source, &home, &nightlight, _lock.as_ref())?;
    let copy_plan = home_copy::plan_live_with_artifact_handoffs(
        &public.source,
        root,
        &home,
        &configuration.copy,
        artifacts.copy_handoffs(),
    )?
    .with_artifact_retirements(artifacts.retiring_copy_targets())?;
    let host = if unchanged_system {
        None
    } else {
        let nix = nix.as_deref().context("system Nix is unavailable")?;
        let projected = tempfile::Builder::new()
            .prefix("dotfiles-system-")
            .tempdir()?;
        let source = projected.path().join("source");
        system_source.materialize(&public.source, &source)?;
        let source = store_source(nix, &source)?;
        let private = private
            .as_ref()
            .map(|private| store_source(nix, &private.source))
            .transpose()?;
        retained.push(source.clone());
        retained.extend(private.iter().cloned());
        let manifest = serde_json::json!({
            "configuration": configuration, "directory": root, "localFile": null,
            "privateFlake": private.as_ref().map(|path| locked_reference(nix, path)).transpose()?,
            "publicFlake": locked_reference(nix, &source)?,
            "publicRevision": public.revision, "publicSource": source,
            "resourceFlake": locked_reference(nix, &source)?,
            "systemInputs": identity, "user": user,
        });
        fs::write(
            workspace.path().join("inputs.json"),
            serde_json::to_vec(&manifest)?,
        )?;
        fs::copy(
            public.source.join("nix/host-flake.nix"),
            workspace.path().join("flake.nix"),
        )?;
        let host = store_source(nix, workspace.path())?;
        retained.push(host.clone());
        Some(host.to_string_lossy().into_owned())
    };
    if !retained.is_empty() {
        let mut retain = nix_command(nix.as_deref().context("input Nix is unavailable")?);
        retain
            .args([
                "build",
                "--offline",
                "--no-write-lock-file",
                "--no-update-lock-file",
                "--out-link",
            ])
            .arg(workspace.path().join("input"))
            .args(retained);
        capture(&mut retain, "cannot retain frozen inputs")?;
    }
    drop(progress);
    let nix = nix.as_deref().unwrap_or_else(|| Path::new(""));
    let progress = Progress::start("Checking changes");
    let mut preview = if let Some(host) = &host {
        let inventory = evaluate_json(
            nix_command(nix)
                .args([
                    "eval",
                    "--raw",
                    "--no-write-lock-file",
                    "--no-update-lock-file",
                ])
                .arg(format!("{host}#inventory.text")),
            "cannot evaluate managed resources",
            show_trace,
        )?;
        crate::inventory::Preview::from_inventory(previous_generation.as_deref(), inventory)?
    } else {
        crate::inventory::Preview::copy_only_with_ownership(previous_generation.as_deref())?
    };
    preview.validate_artifact_ownership(artifacts.targets())?;
    let mut home_targets = crate::home_copy::live_paths(&public.source)?;
    home_targets.extend(configuration.copy.iter().map(PathBuf::from));
    preview.validate_artifact_ownership(home_targets.iter())?;
    let mut derivations = None;
    if preview.needs_native() || preview.needs_generation_comparison() {
        let [system_drv, brewfile_drv] = host_derivations(
            nix,
            host.as_deref().context("system inputs are unavailable")?,
            show_trace,
        )?;
        // Old inventories cannot provide comparable resource fields. Show the
        // immutable target identity; never build a generation before approval.
        if preview.needs_native() {
            preview.native = format!(
                "system generation: {} -> {}",
                previous_generation
                    .as_deref()
                    .map(|path| path.display().to_string())
                    .unwrap_or_else(|| "none".into()),
                system_drv.outputs.out.display()
            );
        } else {
            preview.compare_generation(
                previous_generation.as_deref(),
                &system_drv.outputs.out,
                &public.revision,
            )?;
        }
        derivations = Some([system_drv, brewfile_drv]);
    }
    drop(progress);
    let mut plan = Plan {
        user_settings: Some(user_settings),
        artifacts,
        user_services,
        tools,
        inputs: snapshot,
        home: copy_plan,
        system: preview,
    };
    plan.user_services.verify(_lock.as_ref())?;
    if let Some(settings) = &plan.user_settings {
        settings.verify(_lock.as_ref())?;
    }
    if let Review::Finished = plan.review(mode, runtime.confirm)? {
        if matches!(mode, Mode::Apply) {
            plan.inputs.verify()?;
            let result = plan.home.record_current();
            plan.inputs
                .verify()
                .context("home state may have been recorded; run plan/apply again")?;
            result?;
            plan.artifacts.verify()?;
            plan.artifacts.record_current()?;
        }
        return Ok(ExitCode::SUCCESS);
    }
    plan.inputs.verify()?;
    plan.artifacts.verify()?;
    plan.user_services.verify(_lock.as_ref())?;
    let lock = _lock.as_ref().context("apply lock is unavailable")?;
    if plan.artifacts.has_changes() || plan.system.has_system_changes() {
        let executables: Vec<_> = plan
            .artifacts
            .targets()
            .filter_map(|target| {
                target
                    .strip_prefix(".local/bin")
                    .ok()
                    .and_then(|path| path.to_str())
                    .map(str::to_owned)
            })
            .collect();
        verify_artifact_package_profile_for(
            nix,
            &plan.system,
            plan.inputs.workspace.path(),
            lock,
            &executables,
        )?;
        plan.inputs.verify()?;
        plan.artifacts.verify()?;
    }
    let result = plan.artifacts.realize(lock);
    plan.inputs
        .verify()
        .context("artifacts may have been built and retained")?;
    result?;
    plan.artifacts.prepare_home(lock)?;
    plan.inputs.verify()?;
    let result = plan.home.apply();
    plan.inputs
        .verify()
        .context("home files may be partially changed; run plan/apply again")?;
    result.context(
        "home deployment failed; some home files may be partially changed. Run plan/apply again",
    )?;
    let result = plan.tools.apply(lock);
    plan.inputs
        .verify()
        .context("tools may be partially installed")?;
    result?;
    let result = plan.artifacts.apply(lock);
    plan.inputs
        .verify()
        .context("artifacts may be partially deployed")?;
    result?;
    if let Some(settings) = &mut plan.user_settings {
        let result = settings.apply(lock);
        plan.inputs
            .verify()
            .context("user settings may be partially changed")?;
        result?;
    }
    let result = plan.user_services.apply(lock);
    plan.inputs
        .verify()
        .context("user services may be partially changed")?;
    result?;
    if !plan.system.has_system_changes() {
        return Ok(ExitCode::SUCCESS);
    }
    let workspace = &plan.inputs.workspace;
    let progress = Progress::start("Evaluating system");
    let [system_drv, _] = match derivations {
        Some(derivations) => derivations,
        None => host_derivations(
            nix,
            host.as_deref().context("system inputs are unavailable")?,
            show_trace,
        )?,
    };
    drop(progress);
    let system = {
        let _progress = Progress::start("Building system");
        build(nix, &system_drv, &workspace.path().join("system"))?
    };
    plan.inputs.verify()?;
    let paths = workspace.path().join("copy.json");
    fs::write(&paths, serde_json::to_vec(&configuration.copy)?)?;
    let activated_generation = system.canonicalize()?;
    let mut activation = Command::new(&backend);
    activation
        .arg("activate")
        .arg(nix)
        .arg(&user)
        .arg(&system)
        .arg(
            plan.inputs
                .previous
                .as_deref()
                .unwrap_or_else(|| Path::new("missing")),
        )
        .arg(root.join("flake.nix"))
        .arg(&runtime.executable)
        .arg(&plan.inputs.public.source)
        .arg(&paths)
        .arg(&home)
        .arg("--system-only");
    retain_apply_lock(
        &mut activation,
        _lock.as_ref().context("apply lock is unavailable")?,
    );
    plan.inputs.verify()?;
    let status = activation.status()?;
    if !status.success() {
        if let Some(previous) = &plan.inputs.previous_generation {
            eprintln!("Restore the previous profile: sudo nix-env -p /nix/var/nix/profiles/system --set {}", previous.display());
            eprintln!(
                "Reactivate it: sudo {}/sw/bin/darwin-rebuild activate",
                previous.display()
            );
        }
        bail!("activation/home deployment failed; the previous source record is retained. The system may be partially changed. Run: sudo darwin-rebuild switch --rollback. See docs/operations.md for profile and Homebrew recovery");
    }
    plan.inputs
        .verify_activated(&root.join("flake.nix"), &activated_generation)?;
    Ok(ExitCode::SUCCESS)
}

#[cfg(test)]
fn verify_artifact_package_profile(
    nix: &Path,
    preview: &crate::inventory::Preview,
    workspace: &Path,
    lock: &File,
) -> Result<()> {
    verify_artifact_package_profile_for(nix, preview, workspace, lock, &["localllm".into()])
}

fn verify_artifact_package_profile_for(
    nix: &Path,
    preview: &crate::inventory::Preview,
    workspace: &Path,
    lock: &File,
    executables: &[String],
) -> Result<()> {
    if executables.is_empty() {
        return Ok(());
    }
    let Some((output, derivation)) = preview.artifact_package_profile() else {
        return Ok(());
    };
    for path in [output, derivation] {
        if path.parent() != Some(Path::new("/nix/store")) || !path.is_absolute() {
            bail!("invalid Home Manager package profile store path");
        }
    }
    if derivation
        .extension()
        .is_none_or(|extension| extension != "drv")
    {
        bail!("invalid Home Manager package profile derivation");
    }
    if output.exists() {
        return inspect_artifact_package_profile_for(output, executables);
    }
    if nix.as_os_str().is_empty() {
        bail!("active Home Manager profile disappeared; run plan/apply again");
    }
    let link = workspace.join("artifact-home-manager-profile");
    let mut command = nix_command(nix);
    command
        .args([
            "build",
            "--no-write-lock-file",
            "--no-update-lock-file",
            "--out-link",
        ])
        .arg(&link)
        .arg(format!("{}^out", derivation.display()));
    retain_apply_lock(&mut command, lock);
    capture(
        &mut command,
        "cannot build the reviewed Home Manager package profile",
    )?;
    if link
        .canonicalize()
        .context("Home Manager package profile was not retained")?
        != output
    {
        bail!("Home Manager package profile differs from the reviewed output");
    }
    inspect_artifact_package_profile_for(output, executables)
}

#[cfg(test)]
fn inspect_artifact_package_profile(output: &Path) -> Result<()> {
    inspect_artifact_package_profile_for(output, &["localllm".into()])
}

fn inspect_artifact_package_profile_for(output: &Path, executables: &[String]) -> Result<()> {
    for name in executables {
        match fs::symlink_metadata(output.join("bin").join(name)) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => (),
            Err(error) => return Err(error).context("cannot inspect Home Manager executable export"),
            Ok(_) => bail!("Home Manager package profile exports {name} and conflicts with the public artifact owner"),
        }
    }
    Ok(())
}

fn locked_reference(nix: &Path, source: &Path) -> Result<String> {
    let hash = String::from_utf8(capture(
        nix_command(nix).args(["hash", "path", "--sri"]).arg(source),
        "cannot identify frozen Nix source",
    )?)?;
    let mut reference = url::Url::parse(&format!("path:{}", source.display()))?;
    reference
        .query_pairs_mut()
        .append_pair("narHash", hash.trim());
    Ok(reference.into())
}

fn store_source(nix: &Path, source: &Path) -> Result<PathBuf> {
    let output = capture(
        nix_command(nix)
            .args(["store", "add-path", "--name", "source"])
            .arg(source),
        "cannot freeze Nix inputs",
    )?;
    Ok(PathBuf::from(String::from_utf8(output)?.trim()))
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

pub fn rootless_settings_report(root: &Path, width: Option<usize>) -> Result<String> {
    let local_path = root.join("dotfiles.local.toml");
    let local = read_local(&local_path)?;
    let source = snapshot::Snapshot::capture(root, false)?;
    let configuration = configuration::Configuration::load(&source.source, local.as_deref())?;
    let private = configuration
        .private
        .path
        .as_ref()
        .map(|path| snapshot::Snapshot::capture(&root.join(path), true))
        .transpose()?;
    let settings: Vec<Setting> = serde_json::from_value(rootless_inventory::settings_json(
        &fs::read(source.source.join("dotfiles.toml"))?,
        local.as_deref(),
        &configuration,
    )?)?;
    let generation = current_generation(Path::new("/run/current-system"))?;
    let report =
        rootless_inventory::report(&source.source, &configuration, generation.as_deref(), width)?;
    source.verify()?;
    if let Some(private) = private {
        private.verify()?;
    }
    verify_local(&local_path, &local)?;
    if current_generation(Path::new("/run/current-system"))? != generation {
        bail!("active generation changed while reading settings; run settings again");
    }
    Ok(format!(
        "{}\n{}\n",
        crate::settings::render(&settings, width),
        report
    ))
}

fn resolve_nix(root: &Path) -> Result<PathBuf> {
    Ok(PathBuf::from(
        String::from_utf8(capture(
            Command::new(root.join("bin/system-backend.sh")).arg("require-nix"),
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

#[cfg(test)]
fn review_plan(mode: Mode, preview: crate::inventory::Preview) -> Result<Review> {
    review_plan_with(mode, &preview, None, None, || {
        confirm_apply(&mut io::stdin().lock(), &mut io::stdout().lock())
    })
}

#[cfg(test)]
fn review_plan_with(
    mode: Mode,
    preview: &crate::inventory::Preview,
    tools: Option<&tools::Plan>,
    user_services: Option<&user_services::Plan>,
    confirm: impl FnOnce() -> Result<()>,
) -> Result<Review> {
    review_plan_with_artifacts(mode, preview, tools, user_services, None, None, confirm)
}

fn review_plan_with_artifacts(
    mode: Mode,
    preview: &crate::inventory::Preview,
    tools: Option<&tools::Plan>,
    user_services: Option<&user_services::Plan>,
    artifacts: Option<&artifacts::Plan>,
    user_settings: Option<&user_settings::Plan>,
    confirm: impl FnOnce() -> Result<()>,
) -> Result<Review> {
    if !preview.has_changes()
        && !tools.is_some_and(tools::Plan::has_changes)
        && !user_services.is_some_and(user_services::Plan::has_changes)
        && !artifacts.is_some_and(artifacts::Plan::has_changes)
        && !user_settings.is_some_and(user_settings::Plan::has_changes)
    {
        return Ok(Review::Finished);
    }
    let width = crate::terminal_width();
    let text = [
        tools.map(|tools| tools.render(width)).unwrap_or_default(),
        preview.render(width),
        artifacts.map(artifacts::Plan::render).unwrap_or_default(),
        user_settings
            .map(user_settings::Plan::render)
            .unwrap_or_default(),
        user_services
            .map(user_services::Plan::render)
            .unwrap_or_default(),
    ]
    .into_iter()
    .filter(|text| !text.is_empty())
    .collect::<Vec<_>>()
    .join("\n\n");
    let mut output = io::stdout().lock();
    writeln!(output, "{text}")?;
    output.flush()?;
    drop(output);
    if matches!(mode, Mode::Plan) {
        return Ok(Review::Finished);
    }
    confirm()?;
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

#[cfg(test)]
fn git(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    capture(
        Command::new("git").arg("-C").arg(root).args(args),
        "Git input inspection failed",
    )
}

#[cfg(test)]
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

fn acquire_lock(path: &Path) -> Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        bail!("apply lock is not a regular file");
    }
    fs2::FileExt::try_lock_exclusive(&file).context("dotfiles apply is already running")?;
    Ok(file)
}

fn retain_apply_lock(command: &mut Command, lock: &File) {
    let descriptor = lock.as_raw_fd();
    unsafe {
        command.pre_exec(move || {
            let flags = libc::fcntl(descriptor, libc::F_GETFD);
            if flags < 0 || libc::fcntl(descriptor, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn competing_home_manager_export_is_rejected_even_when_dangling() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("bin")).unwrap();
        assert!(inspect_artifact_package_profile(root.path()).is_ok());
        let executable = root.path().join("bin/localllm");
        std::os::unix::fs::symlink("/missing/private-package/bin/localllm", &executable).unwrap();
        assert!(inspect_artifact_package_profile(root.path()).is_err());
        fs::remove_file(&executable).unwrap();
        fs::write(&executable, "private executable").unwrap();
        assert!(inspect_artifact_package_profile(root.path()).is_err());
    }

    #[test]
    fn package_profile_build_uses_reviewed_derivation_and_rejects_other_output() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("home")).unwrap();
        fs::write(
            root.path().join("home/apm.yml"),
            "dependencies:\n  apm: []\n",
        )
        .unwrap();
        let profile = root.path().join("unexpected-profile");
        fs::create_dir(&profile).unwrap();
        let fake_nix = root.path().join("nix");
        let log = root.path().join("arguments");
        fs::write(&fake_nix, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nwhile [ \"$1\" != --out-link ]; do shift; done\nshift\nln -s '{}' \"$1\"\n", log.display(), profile.display())).unwrap();
        fs::set_permissions(&fake_nix, fs::Permissions::from_mode(0o755)).unwrap();
        let output = "/nix/store/00000000000000000000000000000000-reviewed-profile";
        let drv = format!("{output}.drv");
        let inventory = serde_json::from_value(serde_json::json!({
            "source": root.path(), "packages": [], "system": [], "services": [],
            "tools": [], "timeZone": "UTC", "localllm": {"enabled": true, "default_model": null},
            "homeManagerPackageProfile": output, "homeManagerPackageProfileDrv": drv,
        }))
        .unwrap();
        let preview = crate::inventory::Preview::from_inventory(None, inventory).unwrap();
        let lock = acquire_lock(&root.path().join("lock")).unwrap();
        let error =
            verify_artifact_package_profile(&fake_nix, &preview, root.path(), &lock).unwrap_err();
        assert!(error
            .to_string()
            .contains("differs from the reviewed output"));
        let arguments = fs::read_to_string(log).unwrap();
        assert!(arguments.contains(&format!("{drv}^out")));
        assert!(arguments.contains("--no-update-lock-file"));
        assert!(arguments.contains("--no-write-lock-file"));
    }

    #[test]
    fn absent_home_manager_profile_requires_no_nix_process() {
        let root = tempfile::tempdir().unwrap();
        let lock = acquire_lock(&root.path().join("lock")).unwrap();
        let preview = crate::inventory::Preview::copy_only();
        verify_artifact_package_profile(
            Path::new("/must-not-run-nix"),
            &preview,
            root.path(),
            &lock,
        )
        .unwrap();
    }

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
            let paths: Vec<String> =
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

    // Parallel test processes can briefly inherit unrelated descriptors between
    // fork and exec. After our last holder exits, require bounded eventual
    // release rather than assuming no other test is in that window.
    fn lock_after_release(path: &Path) -> File {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match acquire_lock(path) {
                Ok(file) => return file,
                Err(error)
                    if error
                        .downcast_ref::<io::Error>()
                        .is_some_and(|error| error.kind() == io::ErrorKind::WouldBlock) =>
                {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "lock did not release: {error}"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("cannot reacquire test lock: {error}"),
            }
        }
    }

    #[test]
    fn lock_excludes_another_apply_and_releases_on_drop() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("apply.lock");
        let first = acquire_lock(&path).unwrap();
        assert!(acquire_lock(&path).is_err());
        drop(first);
        drop(lock_after_release(&path));
    }

    #[test]
    fn lock_is_retained_until_the_last_descriptor_closes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("apply.lock");
        let first = acquire_lock(&path).unwrap();
        let duplicate = first.try_clone().unwrap();
        assert!(acquire_lock(&path).is_err());
        drop(first);
        assert!(acquire_lock(&path).is_err());
        drop(duplicate);
        let second = lock_after_release(&path);
        assert!(acquire_lock(&path).is_err());
        drop(second);
        drop(lock_after_release(&path));
    }

    #[test]
    fn running_backends_retain_the_common_lock_after_the_owner_closes_it() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("apply.lock");
        let lock = acquire_lock(&path).unwrap();
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", "read answer"])
            .stdin(std::process::Stdio::piped());
        retain_apply_lock(&mut command, &lock);
        let mut child = command.spawn().unwrap();
        drop(lock);
        let excluded = acquire_lock(&path).is_err();
        drop(child.stdin.take());
        child.wait().unwrap();
        assert!(excluded);
        drop(lock_after_release(&path));
    }

    #[test]
    fn unsafe_common_lock_paths_are_rejected_without_modifying_the_target() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::write(&target, "untouched").unwrap();
        let link = temp.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(acquire_lock(&link).is_err());
        assert_eq!(fs::read_to_string(&target).unwrap(), "untouched");
        assert!(acquire_lock(temp.path()).is_err());
    }
}
