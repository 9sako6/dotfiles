use std::env;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use anyhow::{bail, Context, Result};

pub fn run(repo_root: &Path) -> Result<ExitCode> {
    let mise = resolve_mise_bin()?;
    let steps: &[&[&str]] = &[
        &["bun", "install", "--frozen-lockfile"],
        &["bun", "run", "tsc", "--noEmit"],
        &[
            "bun",
            "test",
            "./tests",
            "./home/.apm/skills/create-anki-cards/tools/anki-cards.test.ts",
        ],
        &[
            "cargo",
            "test",
            "--locked",
            "--manifest-path",
            "cli/Cargo.toml",
        ],
    ];

    for step in steps {
        println!("$ {}", step.join(" "));
        let status = Command::new(&mise)
            .arg("exec")
            .arg("--")
            .args(*step)
            .current_dir(repo_root)
            .env("DOTFILES_DIR", repo_root)
            .status()
            .with_context(|| format!("failed to execute {}", mise.display()))?;
        if !status.success() {
            return Ok(exit_code(status.code()));
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn resolve_mise_bin() -> Result<PathBuf> {
    if let Some(path_var) = env::var_os("PATH") {
        for directory in env::split_paths(&path_var) {
            let candidate = directory.join("mise");
            if is_executable(&candidate) {
                return Ok(candidate);
            }
        }
    }
    if let Some(home) = env::var_os("HOME") {
        let fallback = PathBuf::from(home).join(".local/bin/mise");
        if is_executable(&fallback) {
            return Ok(fallback);
        }
    }
    bail!("mise not found on PATH or at ~/.local/bin/mise. Run ./bin/install-mise.sh");
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    match std::fs::metadata(path) {
        Ok(metadata) => metadata.is_file() && (metadata.permissions().mode() & 0o111) != 0,
        Err(_) => false,
    }
}

fn exit_code(code: Option<i32>) -> ExitCode {
    match code.and_then(|code| u8::try_from(code).ok()) {
        Some(code) => ExitCode::from(code),
        None => ExitCode::from(1),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    use tempfile::tempdir;

    use super::is_executable;

    #[test]
    fn executable_check_requires_execute_bit() {
        let temp = tempdir().unwrap();
        let file = temp.path().join("mise");
        fs::write(&file, "#!/bin/sh\n").unwrap();
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_executable(&file));
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_executable(&file));
    }
}
