mod agents;
mod home_copy;
mod system;

use std::env;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dotfiles",
    version,
    about = "Global dotfiles entrypoint",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Agents {
        #[command(subcommand)]
        operation: agents::Operation,
    },
    /// Build and show the macOS system and home plan without changing state
    Plan {
        #[command(flatten)]
        source: SourceArgs,
    },
    /// Build, show and apply the macOS system and home configuration
    Apply {
        #[command(flatten)]
        source: SourceArgs,
    },
}

#[derive(clap::Args, Debug, Clone)]
struct SourceArgs {
    /// Use the public dotfiles checkout as the system source
    #[arg(long, conflicts_with = "url")]
    default: bool,

    /// Private system source git clone URL (SSH or HTTPS, no credentials)
    #[arg(value_name = "GIT_URL")]
    url: Option<String>,
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("dotfiles: {error:#}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<ExitCode> {
    let cli = Cli::parse();

    let Some(command) = cli.command else {
        Cli::command().print_help().ok();
        println!();
        return Ok(ExitCode::from(1));
    };

    let dotfiles_dir = resolve_dotfiles_dir()?;
    match command {
        Commands::Agents { operation } => agents::run(operation, &dotfiles_dir),
        Commands::Plan { source } => run_system_command(system::Mode::Plan, source, &dotfiles_dir),
        Commands::Apply { source } => {
            run_system_command(system::Mode::Apply, source, &dotfiles_dir)
        }
    }
}

fn run_system_command(
    mode: system::Mode,
    source: SourceArgs,
    dotfiles_dir: &Path,
) -> Result<ExitCode> {
    let home = env::var_os("HOME").context("HOME is not set")?;
    let copy_plan = home_copy::plan(dotfiles_dir, Path::new(&home))?;
    env::set_var("DOTFILES_HOME_COPY_PLAN", copy_plan.preview());

    let exit = system::run(
        mode,
        system::source_request(source.default, source.url)?,
        dotfiles_dir,
    )?;
    if matches!(mode, system::Mode::Apply) && exit == ExitCode::SUCCESS {
        copy_plan.apply()?;
    }
    Ok(exit)
}

fn resolve_dotfiles_dir() -> Result<PathBuf> {
    let candidate = if let Ok(directory) = env::var("DOTFILES_DIR") {
        let path = PathBuf::from(directory);
        if path.is_relative() {
            anyhow::bail!("DOTFILES_DIR must be an absolute path");
        }
        path
    } else {
        let home = env::var("HOME").context("HOME is not set")?;
        PathBuf::from(home).join("dotfiles")
    };

    require_flake(&candidate)?;
    Ok(candidate)
}

fn require_flake(candidate: &Path) -> Result<()> {
    let flake = candidate.join("flake.nix");
    if !flake.is_file() {
        anyhow::bail!(
            "dotfiles checkout not found at {} (expected flake.nix). Set DOTFILES_DIR or clone to ~/dotfiles.",
            candidate.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::require_flake;

    #[test]
    fn rejects_directory_without_flake() {
        let temp = tempfile::tempdir().unwrap();
        assert!(require_flake(temp.path()).is_err());
    }

    #[test]
    fn accepts_directory_with_flake() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("flake.nix"), "{}\n").unwrap();
        require_flake(temp.path()).unwrap();
    }
}
