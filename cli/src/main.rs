mod agents;
mod home_copy;
mod settings;
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
    #[command(hide = true)]
    CompleteApply {
        source: PathBuf,
        paths: PathBuf,
        home: PathBuf,
    },
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
    #[command(about = "Show all effective settings and their sources")]
    Settings,
}

#[derive(clap::Args, Debug, Clone)]
struct SourceArgs {
    #[arg(long, hide = true)]
    default: bool,

    #[arg(value_name = "REMOVED_SOURCE", hide = true)]
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

    if let Commands::CompleteApply {
        source,
        paths,
        home,
    } = command
    {
        let paths: Vec<String> = serde_json::from_slice(&std::fs::read(paths)?)?;
        home_copy::plan(&source, &home, &paths)?.apply()?;
        return Ok(ExitCode::SUCCESS);
    }
    let dotfiles_dir = resolve_dotfiles_dir()?;
    match command {
        Commands::CompleteApply { .. } => unreachable!(),
        Commands::Agents { operation } => agents::run(operation, &dotfiles_dir),
        Commands::Plan { source } => run_system_command(system::Mode::Plan, source, &dotfiles_dir),
        Commands::Apply { source } => {
            run_system_command(system::Mode::Apply, source, &dotfiles_dir)
        }
        Commands::Settings => settings::run(&dotfiles_dir),
    }
}

fn run_system_command(
    mode: system::Mode,
    source: SourceArgs,
    dotfiles_dir: &Path,
) -> Result<ExitCode> {
    if source.default || source.url.is_some() {
        anyhow::bail!("URL/--default source selection was removed. Migrate private modules with dotfiles.local.toml; see docs/operations.md.");
    }
    system::run(mode, dotfiles_dir)
}

fn resolve_dotfiles_dir() -> Result<PathBuf> {
    let candidate = if let Ok(directory) = env::var("DOTFILES_DIR") {
        let path = PathBuf::from(directory);
        if path.is_relative() {
            anyhow::bail!("DOTFILES_DIR must be an absolute path");
        }
        path
    } else if let Some(root) = env::current_dir()?
        .ancestors()
        .find(|p| p.join("flake.nix").is_file() && p.join("dotfiles.toml").is_file())
    {
        root.to_path_buf()
    } else {
        let home = env::var("HOME").context("HOME is not set")?;
        PathBuf::from(home).join("dotfiles")
    };

    require_flake(&candidate)?;
    Ok(candidate.canonicalize()?)
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
