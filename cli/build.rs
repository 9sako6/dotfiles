use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() {
    println!("cargo::rerun-if-env-changed=DOTFILES_BUILD_REVISION");
    println!("cargo::rerun-if-changed=build.rs");
    let revision = env::var("DOTFILES_BUILD_REVISION")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(git_revision)
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo::rustc-env=DOTFILES_BUILD_REVISION={revision}");
}

fn git_revision() -> Option<String> {
    let manifest = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR")?);
    let root = manifest.parent()?;
    println!("cargo::rerun-if-changed={}", root.join(".git").display());
    let git_root = PathBuf::from(git(root, &["rev-parse", "--show-toplevel"])?);
    if git_root.canonicalize().ok()? != root.canonicalize().ok()? {
        return None;
    }
    for path in git(root, &["ls-files", "-z"])?.split('\0') {
        if !path.is_empty() {
            println!("cargo::rerun-if-changed={}", root.join(path).display());
        }
    }
    for name in ["HEAD", "index", "packed-refs", "refs"] {
        let path = git(root, &["rev-parse", "--git-path", name])?;
        let path = root.join(path);
        if path.exists() {
            println!("cargo::rerun-if-changed={}", path.display());
        }
    }
    let revision = git(root, &["rev-parse", "HEAD"])?;
    let status = git(root, &["status", "--porcelain", "--untracked-files=no"])?;
    Some(if status.is_empty() {
        revision
    } else {
        format!("{revision}-dirty")
    })
}

fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("--no-optional-locks")
        .args(arguments)
        .current_dir(root)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8(output.stdout).ok())?
        .map(|value| value.trim_end_matches('\n').to_owned())
}
