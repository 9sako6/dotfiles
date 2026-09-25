use std::collections::HashSet;
use std::env;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{bail, ensure, Context, Result};
use clap::{Args, Subcommand};

#[derive(Debug, Subcommand)]
pub enum Operation {
    #[command(
        about = "Print verified repositories in input order; report rejected plugins on stderr"
    )]
    Verify(VerifyArgs),
}

#[derive(Debug, Args)]
pub struct VerifyArgs {
    #[arg(
        long,
        help = "mise config.toml (defaults to $XDG_CONFIG_HOME/mise/config.toml)"
    )]
    config: Option<PathBuf>,
    #[arg(long, help = "Zinit's absolute plugin directory")]
    plugins_dir: PathBuf,
    #[arg(
        required = true,
        help = "Repositories to verify, in shell loading order (owner/name)"
    )]
    repositories: Vec<String>,
}

pub fn run(operation: Operation) -> Result<ExitCode> {
    let Operation::Verify(args) = operation;
    let home = PathBuf::from(env::var_os("HOME").context("HOME is not set")?);
    ensure!(home.is_absolute(), "HOME must be an absolute path");
    ensure!(
        args.plugins_dir.is_absolute(),
        "plugin directory must be an absolute path"
    );
    let config = args.config.unwrap_or_else(|| {
        env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
            .unwrap_or_else(|| home.join(".config"))
            .join("mise/config.toml")
    });
    let text = fs::read_to_string(&config)
        .with_context(|| format!("cannot read mise configuration {}", config.display()))?;
    let document: toml::Table = text
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid TOML in {}", config.display()))?;
    let repositories = document
        .get("bootstrap")
        .and_then(|value| value.get("repos"))
        .and_then(toml::Value::as_table)
        .context("mise configuration has no bootstrap.repos table")?;

    let mut seen = HashSet::new();
    let mut code = ExitCode::SUCCESS;
    for repository in args.repositories {
        if !seen.insert(repository.clone()) {
            continue;
        }
        match verify(&repository, &args.plugins_dir, &home, repositories) {
            Ok(()) => println!("{repository}"),
            Err(error) => {
                eprintln!("zinit: refusing {repository}; {error:#}");
                code = ExitCode::from(1);
            }
        }
    }
    Ok(code)
}

fn verify(repository: &str, plugins_dir: &Path, home: &Path, pins: &toml::Table) -> Result<()> {
    let parts: Vec<_> = repository.split('/').collect();
    ensure!(
        parts.len() == 2
            && parts.iter().all(|part| {
                !part.is_empty()
                    && *part != "."
                    && *part != ".."
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || b"-_.".contains(&byte))
            }),
        "expected an owner/name repository"
    );
    let directory = normalize(&plugins_dir.join(repository.replace('/', "---")));
    let mut matches = pins.iter().filter(|(path, _)| {
        let expanded = path
            .strip_prefix("~/")
            .map(|relative| home.join(relative))
            .unwrap_or_else(|| PathBuf::from(path));
        expanded.is_absolute() && normalize(&expanded) == directory
    });
    let (_, pin) = matches.next().context("no pin for the plugin directory")?;
    ensure!(
        matches.next().is_none(),
        "plugin directory has multiple pins"
    );
    let revision = pin
        .get("ref")
        .and_then(toml::Value::as_str)
        .context("plugin pin must contain a ref string")?;
    ensure!(
        revision.len() == 40
            && revision
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "plugin pin must be a complete lowercase Git commit hash"
    );
    ensure!(
        directory.join(".git").exists(),
        "plugin directory is not a Git checkout"
    );
    let head = git(&directory, &["rev-parse", "--verify", "HEAD"])?;
    ensure!(
        head.trim() == revision,
        "checkout does not match pinned commit {revision}"
    );
    ensure!(
        git(
            &directory,
            &[
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "--ignore-submodules=none"
            ]
        )?
        .is_empty(),
        "checkout has uncommitted changes"
    );
    Ok(())
}

fn git(directory: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .output()
        .context("cannot run Git to verify plugin")?;
    if !output.status.success() {
        bail!("Git {} failed while verifying plugin", args[0]);
    }
    String::from_utf8(output.stdout).context("Git returned invalid UTF-8")
}

fn normalize(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                result.pop();
            }
            component => result.push(component.as_os_str()),
        }
    }
    result
}
