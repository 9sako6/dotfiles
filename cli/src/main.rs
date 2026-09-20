mod agents;
mod home_copy;
mod inventory;
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
    version = env!("DOTFILES_BUILD_REVISION"),
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
        options: SystemArgs,
    },
    /// Build, show and apply the macOS system and home configuration
    Apply {
        #[command(flatten)]
        options: SystemArgs,
    },
    #[command(about = "Show effective settings and managed resources")]
    Settings,
    #[command(about = "Show the commit used to build this CLI")]
    Version,
}

#[derive(clap::Args, Debug)]
struct SystemArgs {
    #[arg(
        long,
        help = "Show Nix evaluation errors and traces locally; may include private configuration"
    )]
    show_trace: bool,
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

    if let Commands::Version = command {
        print!("{}", Cli::command().render_version());
        return Ok(ExitCode::SUCCESS);
    }

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
        Commands::CompleteApply { .. } | Commands::Version => unreachable!(),
        Commands::Agents { operation } => agents::run(operation, &dotfiles_dir),
        Commands::Plan { options } => {
            system::run(system::Mode::Plan, &dotfiles_dir, options.show_trace)
        }
        Commands::Apply { options } => {
            system::run(system::Mode::Apply, &dotfiles_dir, options.show_trace)
        }
        Commands::Settings => settings::run(&dotfiles_dir),
    }
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

fn terminal_width() -> Option<usize> {
    let terminal = console::Term::stdout();
    (terminal.is_term() && env::var("TERM").is_ok_and(|term| term != "dumb"))
        .then(|| usize::from(terminal.size().1))
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
