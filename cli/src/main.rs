use std::env;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result};
use clap::{CommandFactory, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "dotfiles",
    version,
    about = "Global dotfiles entrypoint — run plan/apply from anywhere",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand, Debug)]
enum Commands {
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
        Err(err) => {
            eprintln!("dotfiles: {err:#}");
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

    let (mode, source) = match command {
        Commands::Plan { source } => ("plan", source),
        Commands::Apply { source } => ("apply", source),
    };

    if source.default && source.url.is_some() {
        anyhow::bail!("--default and a Git URL cannot be used together");
    }
    if let Some(url) = &source.url {
        validate_git_url(url)?;
    }

    let dotfiles_dir = resolve_dotfiles_dir()?;
    let mise_bin = resolve_mise_bin()?;

    let mut args: Vec<OsString> = vec!["run".into(), mode.into()];
    if source.default {
        args.push("--default".into());
    } else if let Some(url) = source.url {
        args.push(url.into());
    }

    let status = Command::new(&mise_bin)
        .args(&args)
        .current_dir(&dotfiles_dir)
        .env("DOTFILES_DIR", &dotfiles_dir)
        .status()
        .with_context(|| format!("failed to execute {}", mise_bin.display()))?;

    Ok(ExitCode::from(status.code().unwrap_or(1) as u8))
}

fn resolve_dotfiles_dir() -> Result<PathBuf> {
    let candidate = if let Ok(dir) = env::var("DOTFILES_DIR") {
        let p = PathBuf::from(dir);
        if p.is_relative() {
            anyhow::bail!("DOTFILES_DIR must be an absolute path");
        }
        p
    } else {
        let home = env::var("HOME").context("HOME is not set")?;
        PathBuf::from(home).join("dotfiles")
    };

    let flake = candidate.join("flake.nix");
    if !flake.is_file() {
        anyhow::bail!(
            "dotfiles checkout not found at {} (expected flake.nix). Set DOTFILES_DIR or clone to ~/dotfiles.",
            candidate.display()
        );
    }
    Ok(candidate)
}

fn resolve_mise_bin() -> Result<PathBuf> {
    if let Ok(path_var) = env::var("PATH") {
        for dir in env::split_paths(&path_var) {
            let candidate = dir.join("mise");
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    if let Ok(home) = env::var("HOME") {
        let fallback = PathBuf::from(home).join(".local/bin/mise");
        if is_executable(&fallback) {
            return Ok(fallback);
        }
    }
    anyhow::bail!("mise not found on PATH or at ~/.local/bin/mise. Run ./bin/install-mise.sh");
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && (meta.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
}

fn validate_git_url(value: &str) -> Result<()> {
    if value
        .chars()
        .any(|c| c.is_whitespace() || c == '\0' || c == '\r' || c == '\n')
    {
        anyhow::bail!("Git URL must not contain whitespace or control characters");
    }
    if let Some(at) = value.find('@') {
        let after_at = &value[at + 1..];
        if after_at.contains(':') && !value.contains("://") {
            let colon = after_at.find(':').unwrap();
            if after_at[colon + 1..].is_empty() {
                anyhow::bail!("Git source must identify a remote repository");
            }
            return Ok(());
        }
    }
    let url = url::Url::parse(value)
        .map_err(|_| anyhow::anyhow!("Git source must be an SSH or HTTPS clone URL"))?;
    if url.scheme() != "ssh" && url.scheme() != "https" {
        anyhow::bail!("Git source must use SSH or HTTPS");
    }
    if url.host_str().unwrap_or("").is_empty() || url.path() == "" || url.path() == "/" {
        anyhow::bail!("Git source must identify a remote repository");
    }
    if url.password().is_some() || (url.scheme() == "https" && !url.username().is_empty()) {
        anyhow::bail!("Git credentials must not be embedded in the URL");
    }
    if url.query().is_some() || url.fragment().is_some() {
        anyhow::bail!("Git source must use the remote default branch");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_git_url;

    #[test]
    fn valid_urls() {
        for url in [
            "git@example.test:owner/config.git",
            "ssh://git@example.test/owner/config.git",
            "https://example.test/owner/config.git",
        ] {
            validate_git_url(url).expect(url);
        }
    }

    #[test]
    fn invalid_urls() {
        for url in [
            "https://token@example.test/owner/config.git",
            "https://example.test/owner/config.git?ref=main",
            "ssh://git@example.test/owner/config.git#main",
            "../config",
        ] {
            assert!(validate_git_url(url).is_err(), "{url} should be rejected");
        }
    }
}
