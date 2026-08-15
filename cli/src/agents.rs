use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::Deserialize;

const TARGETS: &str = "claude,codex,opencode";
const LOCAL_SKILL_PREFIX: &str = "./.apm/skills/";
const LOCAL_SKILL_PREFIX_WITHOUT_DOT: &str = ".apm/skills/";

#[derive(Debug, Subcommand)]
pub enum Operation {
    Build,
    Install {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Update {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    Uninstall {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    RemoveLocal {
        #[arg(value_name = "SKILL_NAME")]
        skill_name: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CommandPlan {
    command: &'static str,
    args: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
enum BuildOperation {
    Build,
    Install,
    Update,
    Uninstall,
}

trait CommandRunner {
    fn run(&self, command: &str, args: &[String], cwd: &Path) -> Result<()>;
}

struct ProcessRunner;

impl CommandRunner for ProcessRunner {
    fn run(&self, command: &str, args: &[String], cwd: &Path) -> Result<()> {
        let executable = if command == "apm" {
            resolve_apm(cwd)?
        } else {
            OsString::from(command)
        };
        let status = Command::new(&executable)
            .args(args)
            .current_dir(cwd)
            .status()
            .with_context(|| format!("failed to execute {command}"))?;
        if status.success() {
            return Ok(());
        }
        bail!(
            "{} {} failed with exit code {}",
            command,
            args.join(" "),
            status
                .code()
                .map_or_else(|| "signal".to_owned(), |code| code.to_string())
        )
    }
}

pub fn run(operation: Operation, repo_root: &Path) -> Result<ExitCode> {
    run_with(operation, repo_root, &ProcessRunner)
}

fn run_with<R: CommandRunner>(
    operation: Operation,
    repo_root: &Path,
    runner: &R,
) -> Result<ExitCode> {
    let home_root = repo_root.join("home");
    let lock_path = home_root.join("apm.lock.yaml");
    let original_lockfile = read_optional(&lock_path)?;
    let result = (|| {
        let source_to_remove = match operation {
            Operation::Build => {
                run_build_operation(BuildOperation::Build, &[], &home_root, runner)?;
                None
            }
            Operation::Install { args } => {
                run_build_operation(BuildOperation::Install, &args, &home_root, runner)?;
                None
            }
            Operation::Update { args } => {
                run_build_operation(BuildOperation::Update, &args, &home_root, runner)?;
                None
            }
            Operation::Uninstall { args } => {
                assert_remote_uninstall_targets(&home_root, &args)?;
                run_build_operation(BuildOperation::Uninstall, &args, &home_root, runner)?;
                None
            }
            Operation::RemoveLocal { skill_name } => {
                let source_path = remove_local_skill(&home_root, &skill_name, runner)?;
                Some(source_path)
            }
        };

        finalize_compiled_agents(&home_root)?;
        if let Some(source_path) = source_to_remove {
            fs::remove_dir_all(&source_path).with_context(|| {
                format!(
                    "failed to remove local skill source {}",
                    source_path.display()
                )
            })?;
        }
        Ok(ExitCode::SUCCESS)
    })();
    let restore_result =
        restore_lockfile_if_only_generated_at_changed(&lock_path, original_lockfile.as_deref());

    match (result, restore_result) {
        (Err(error), Ok(())) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Err(error), Err(restore_error)) => Err(error.context(format!(
            "also failed to restore APM lockfile: {restore_error:#}"
        ))),
        (Ok(code), Ok(())) => Ok(code),
    }
}

fn run_build_operation<R: CommandRunner>(
    operation: BuildOperation,
    args: &[String],
    home_root: &Path,
    runner: &R,
) -> Result<()> {
    for plan in create_build_plan(operation, args)? {
        runner.run(plan.command, &plan.args, home_root)?;
    }
    Ok(())
}

fn create_build_plan(operation: BuildOperation, args: &[String]) -> Result<Vec<CommandPlan>> {
    let compile = CommandPlan {
        command: "apm",
        args: ["compile", "--clean", "--target", TARGETS]
            .into_iter()
            .map(String::from)
            .collect(),
    };
    let mut plans = match operation {
        BuildOperation::Build => vec![CommandPlan {
            command: "apm",
            args: ["install", "--frozen", "--only", "apm", "--target", TARGETS]
                .into_iter()
                .map(String::from)
                .collect(),
        }],
        BuildOperation::Install => vec![CommandPlan {
            command: "apm",
            args: with_target("install", args),
        }],
        BuildOperation::Update => vec![CommandPlan {
            command: "apm",
            args: with_args("deps", "update", args),
        }],
        BuildOperation::Uninstall => {
            for arg in args {
                if is_local_skill_path(arg) {
                    bail!(
                        "Local skill sources are repository-owned; use mise run agents:remove-local"
                    );
                }
            }
            vec![CommandPlan {
                command: "apm",
                args: with_args("uninstall", "", args),
            }]
        }
    };
    plans.push(compile);
    Ok(plans)
}

fn with_target(command: &str, args: &[String]) -> Vec<String> {
    let mut result = vec![command.to_owned()];
    result.extend(args.iter().cloned());
    result.extend(["--target".to_owned(), TARGETS.to_owned()]);
    result
}

fn with_args(first: &str, second: &str, args: &[String]) -> Vec<String> {
    let mut result = vec![first.to_owned()];
    if !second.is_empty() {
        result.push(second.to_owned());
    }
    result.extend(args.iter().cloned());
    result
}

fn remove_local_skill<R: CommandRunner>(
    home_root: &Path,
    skill_name: &str,
    runner: &R,
) -> Result<PathBuf> {
    validate_skill_name(skill_name)?;
    let source_path = home_root.join(".apm/skills").join(skill_name);
    require_file(
        &source_path.join("SKILL.md"),
        &format!("Local skill not found: .apm/skills/{skill_name}"),
    )?;

    let dependency = format!("./.apm/skills/{skill_name}");
    if !has_apm_dependency(&home_root.join("apm.yml"), &dependency)? {
        runner.run(
            "apm",
            &with_target("install", std::slice::from_ref(&dependency)),
            home_root,
        )?;
    }
    runner.run(
        "apm",
        &with_args("uninstall", "", std::slice::from_ref(&dependency)),
        home_root,
    )?;
    run_build_operation(BuildOperation::Build, &[], home_root, runner)?;
    Ok(source_path)
}

fn assert_remote_uninstall_targets(home_root: &Path, args: &[String]) -> Result<()> {
    for arg in args {
        if is_local_skill_path(arg) {
            bail!("Local skill sources are repository-owned; use mise run agents:remove-local");
        }
        if is_valid_skill_name(arg)
            && home_root
                .join(".apm/skills")
                .join(arg)
                .join("SKILL.md")
                .is_file()
        {
            bail!(
                "Local skill sources are repository-owned; use mise run agents:remove-local {arg}"
            );
        }
    }
    Ok(())
}

fn finalize_compiled_agents(root: &Path) -> Result<()> {
    let source_agents = root.join("AGENTS.md");
    require_file(&source_agents, "APM did not generate AGENTS.md")?;

    let codex_dir = root.join(".codex");
    fs::create_dir_all(&codex_dir)
        .with_context(|| format!("failed to create {}", codex_dir.display()))?;
    let codex_agents = codex_dir.join("AGENTS.md");
    fs::rename(&source_agents, &codex_agents).with_context(|| {
        format!(
            "failed to move {} to {}",
            source_agents.display(),
            codex_agents.display()
        )
    })?;

    let opencode_dir = root.join(".config/opencode");
    fs::create_dir_all(&opencode_dir)
        .with_context(|| format!("failed to create {}", opencode_dir.display()))?;
    fs::copy(&codex_agents, opencode_dir.join("AGENTS.md"))
        .with_context(|| format!("failed to copy {} to opencode", codex_agents.display()))?;

    for relative in ["CLAUDE.md", "GEMINI.md", ".codex/config.toml", ".mcp.json"] {
        remove_file_if_exists(&root.join(relative))?;
    }
    require_file(
        &codex_agents,
        "APM finalization did not produce .codex/AGENTS.md",
    )
}

fn has_apm_dependency(config_path: &Path, dependency: &str) -> Result<bool> {
    let raw = fs::read_to_string(config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let config: ApmConfig = serde_yaml_ng::from_str(&raw)
        .with_context(|| format!("failed to parse {} as YAML", config_path.display()))?;
    Ok(config
        .dependencies
        .and_then(|dependencies| dependencies.apm)
        .is_some_and(|dependencies| dependencies.iter().any(|value| value == dependency)))
}

#[derive(Debug, Deserialize)]
struct ApmConfig {
    dependencies: Option<ApmDependencies>,
}

#[derive(Debug, Deserialize)]
struct ApmDependencies {
    apm: Option<Vec<String>>,
}

fn restore_lockfile_if_only_generated_at_changed(
    lock_path: &Path,
    original: Option<&[u8]>,
) -> Result<()> {
    let Some(original) = original else {
        return Ok(());
    };
    let Some(current) = read_optional(lock_path)? else {
        return Ok(());
    };
    if current != original && ignore_generated_at(&current) == ignore_generated_at(original) {
        fs::write(lock_path, original)
            .with_context(|| format!("failed to restore {}", lock_path.display()))?;
    }
    Ok(())
}

fn ignore_generated_at(content: &[u8]) -> Vec<u8> {
    let mut result = Vec::with_capacity(content.len());
    let mut start = 0;
    while start < content.len() {
        let end = content[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(content.len(), |offset| start + offset);
        let line = &content[start..end];
        if line.starts_with(b"generated_at: ") {
            result.extend_from_slice(b"generated_at: <ignored>");
        } else {
            result.extend_from_slice(line);
        }
        if end < content.len() {
            result.push(b'\n');
            start = end + 1;
        } else {
            start = end;
        }
    }
    result
}

fn read_optional(path: &Path) -> Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("failed to read {}", path.display())),
    }
}

fn resolve_apm(cwd: &Path) -> Result<OsString> {
    let mise_cwd = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| cwd.to_path_buf());
    let output = Command::new("mise")
        .args(["which", "apm"])
        .current_dir(mise_cwd)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()
        .context("failed to execute mise which apm")?;
    if !output.status.success() {
        bail!(
            "mise which apm failed with exit code {:?}",
            output.status.code()
        );
    }
    let path = String::from_utf8(output.stdout).context("mise which apm output was not UTF-8")?;
    let path = path.trim();
    if path.is_empty() {
        bail!("mise which apm did not return a command path");
    }
    Ok(OsString::from(path))
}

fn require_file(path: &Path, message: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("{message}")
    }
}

fn remove_file_if_exists(path: &Path) -> Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("failed to remove {}", path.display())),
    }
}

fn validate_skill_name(value: &str) -> Result<()> {
    if is_valid_skill_name(value) {
        Ok(())
    } else {
        bail!("Expected exactly one local skill name")
    }
}

fn is_valid_skill_name(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

fn is_local_skill_path(value: &str) -> bool {
    let Some(name) = value
        .strip_prefix(LOCAL_SKILL_PREFIX)
        .or_else(|| value.strip_prefix(LOCAL_SKILL_PREFIX_WITHOUT_DOT))
    else {
        return false;
    };
    !name.is_empty()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::{
        create_build_plan, has_apm_dependency, ignore_generated_at, run_with, BuildOperation,
        CommandRunner, Operation,
    };

    #[derive(Default)]
    struct FakeRunner {
        calls: std::cell::RefCell<Vec<(String, Vec<String>, String)>>,
        fail_at: Option<usize>,
        mutate_lock: bool,
    }

    impl CommandRunner for FakeRunner {
        fn run(&self, command: &str, args: &[String], cwd: &Path) -> anyhow::Result<()> {
            let mut calls = self.calls.borrow_mut();
            calls.push((command.to_owned(), args.to_vec(), cwd.display().to_string()));
            if self.mutate_lock {
                fs::write(cwd.join("apm.lock.yaml"), "generated_at: new\nvalue: 1\n").unwrap();
            }
            if self.fail_at == Some(calls.len()) {
                anyhow::bail!("fake command failed")
            }
            Ok(())
        }
    }

    fn setup_repo() -> tempfile::TempDir {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("home");
        fs::create_dir_all(home.join(".apm/skills/example-skill")).unwrap();
        fs::write(home.join("AGENTS.md"), "# agents\n").unwrap();
        fs::write(home.join("apm.yml"), "dependencies:\n  apm: []\n").unwrap();
        fs::write(home.join("apm.lock.yaml"), "generated_at: old\nvalue: 1\n").unwrap();
        fs::write(home.join(".apm/skills/example-skill/SKILL.md"), "# skill\n").unwrap();
        temp
    }

    #[test]
    fn creates_expected_build_plans() {
        assert_eq!(
            create_build_plan(BuildOperation::Build, &[]).unwrap()[0].args,
            [
                "install",
                "--frozen",
                "--only",
                "apm",
                "--target",
                "claude,codex,opencode"
            ]
        );
        assert_eq!(
            create_build_plan(BuildOperation::Install, &["owner/skill".into()]).unwrap()[0].args,
            [
                "install",
                "owner/skill",
                "--target",
                "claude,codex,opencode"
            ]
        );
        assert_eq!(
            create_build_plan(BuildOperation::Update, &["owner/skill".into()]).unwrap()[0].args,
            ["deps", "update", "owner/skill"]
        );
        assert_eq!(
            create_build_plan(BuildOperation::Uninstall, &["owner/skill".into()]).unwrap()[0].args,
            ["uninstall", "owner/skill"]
        );
    }

    #[test]
    fn rejects_local_uninstall_paths() {
        assert!(create_build_plan(
            BuildOperation::Uninstall,
            &["./.apm/skills/example-skill".into()]
        )
        .is_err());
        assert!(create_build_plan(
            BuildOperation::Uninstall,
            &[".apm/skills/example-skill".into()]
        )
        .is_err());
    }

    #[test]
    fn parses_yaml_dependencies_instead_of_matching_lines() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("apm.yml");
        fs::write(
            &path,
            "dependencies:\n  apm:\n    - \"./.apm/skills/example-skill\" # owned\n    - owner/remote\n",
        )
        .unwrap();
        assert!(has_apm_dependency(&path, "./.apm/skills/example-skill").unwrap());
        assert!(!has_apm_dependency(&path, "./.apm/skills/missing").unwrap());
    }

    #[test]
    fn removes_source_only_after_build_and_finalize_succeed() {
        let temp = setup_repo();
        let runner = FakeRunner::default();
        run_with(
            Operation::RemoveLocal {
                skill_name: "example-skill".into(),
            },
            temp.path(),
            &runner,
        )
        .unwrap();
        assert!(!temp.path().join("home/.apm/skills/example-skill").exists());
        assert_eq!(runner.calls.borrow().len(), 4);
    }

    #[test]
    fn leaves_source_when_compile_fails() {
        let temp = setup_repo();
        let runner = FakeRunner {
            fail_at: Some(4),
            ..FakeRunner::default()
        };
        assert!(run_with(
            Operation::RemoveLocal {
                skill_name: "example-skill".into(),
            },
            temp.path(),
            &runner,
        )
        .is_err());
        assert!(temp.path().join("home/.apm/skills/example-skill").exists());
    }

    #[test]
    fn restores_lockfile_when_only_generated_at_changed_even_after_failure() {
        let temp = setup_repo();
        let runner = FakeRunner {
            fail_at: Some(1),
            mutate_lock: true,
            ..FakeRunner::default()
        };
        assert!(run_with(Operation::Build, temp.path(), &runner).is_err());
        assert_eq!(
            fs::read_to_string(temp.path().join("home/apm.lock.yaml")).unwrap(),
            "generated_at: old\nvalue: 1\n"
        );
    }

    #[test]
    fn preserves_other_lockfile_changes() {
        assert_ne!(
            ignore_generated_at(b"generated_at: old\nvalue: 1\n"),
            ignore_generated_at(b"generated_at: new\nvalue: 2\n")
        );
    }
}
