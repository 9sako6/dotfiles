use std::env;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, Copy)]
pub enum Mode {
    Plan,
    Apply,
}

impl Mode {
    fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Apply => "apply",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceRequest {
    Current,
    Default,
    Remote(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceKind {
    Default,
    Remote,
}

impl SourceKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Remote => "remote",
        }
    }
}

#[derive(Debug, Clone)]
struct PreparedSource {
    directory: PathBuf,
    kind: SourceKind,
    previous_target: Option<PathBuf>,
    revision: String,
    url: Option<String>,
}

pub fn source_request(use_default: bool, url: Option<String>) -> Result<SourceRequest> {
    if use_default && url.is_some() {
        bail!("--default and a Git URL cannot be used together");
    }
    if use_default {
        return Ok(SourceRequest::Default);
    }
    match url {
        Some(url) => {
            validate_git_url(&url)?;
            Ok(SourceRequest::Remote(url))
        }
        None => Ok(SourceRequest::Current),
    }
}

pub fn run(mode: Mode, request: SourceRequest, repo_root: &Path) -> Result<ExitCode> {
    let user = current_user()?;
    if user.uid == 0 {
        bail!("run system commands as the login user; sudo is requested when needed");
    }

    let home = env::var_os("HOME").context("HOME is not set")?;
    let data_root = system_source_data_root(
        Path::new(&home),
        env::var_os("XDG_DATA_HOME").as_deref().map(Path::new),
    );
    let source = resolve_system_source(
        request,
        &data_root,
        repo_root,
        Path::new("/etc/nix-darwin/flake.nix"),
    )?;

    match source.kind {
        SourceKind::Default => println!("system source: public dotfiles (local checkout)"),
        SourceKind::Remote => println!("system source: {}", source.url.as_deref().unwrap_or("")),
    }
    println!("source revision: {}", source.revision);

    let backend = repo_root.join("bin/system-backend.sh");
    if !backend.is_file() {
        bail!("system backend not found at {}", backend.display());
    }
    let desired_target = source.directory.join("flake.nix");
    let expected_target = source
        .previous_target
        .as_deref()
        .unwrap_or_else(|| Path::new("missing"));

    let status = Command::new(&backend)
        .arg(mode.as_str())
        .arg(source.kind.as_str())
        .arg(&user.name)
        .arg(&source.directory)
        .arg(expected_target)
        .arg(&desired_target)
        .current_dir(repo_root)
        .env("DOTFILES_DIR", repo_root)
        .status()
        .with_context(|| format!("failed to execute {}", backend.display()))?;

    Ok(exit_code(status.code()))
}

fn resolve_system_source(
    request: SourceRequest,
    data_root: &Path,
    public_directory: &Path,
    selection_path: &Path,
) -> Result<PreparedSource> {
    let selected = inspect_selected_system_source(data_root, public_directory, selection_path)?;
    let desired = match request {
        SourceRequest::Current => match selected.kind {
            SourceKind::Default => SourceRequest::Default,
            SourceKind::Remote => SourceRequest::Remote(
                selected
                    .url
                    .clone()
                    .context("selected remote system source has no URL")?,
            ),
        },
        request => request,
    };

    match desired {
        SourceRequest::Default => {
            let public_flake = public_directory.join("flake.nix");
            require_file(&public_flake, "public dotfiles flake")?;
            Ok(PreparedSource {
                directory: public_directory.to_path_buf(),
                kind: SourceKind::Default,
                previous_target: selected.previous_target,
                revision: git_capture(Some(public_directory), &["rev-parse", "HEAD"])?,
                url: None,
            })
        }
        SourceRequest::Remote(url) => {
            let (directory, revision) = prepare_remote_checkout(data_root, &url)?;
            Ok(PreparedSource {
                directory,
                kind: SourceKind::Remote,
                previous_target: selected.previous_target,
                revision,
                url: Some(url),
            })
        }
        SourceRequest::Current => unreachable!("current source is resolved above"),
    }
}

fn inspect_selected_system_source(
    data_root: &Path,
    public_directory: &Path,
    selection_path: &Path,
) -> Result<PreparedSource> {
    let public_flake = public_directory.join("flake.nix");
    let accepted_public_flakes = [
        normalize_path(&public_flake),
        normalize_path(&public_directory.join("darwin/flake.nix")),
    ];
    let (request, target) = inspect_selection(&accepted_public_flakes, data_root, selection_path)?;

    match request {
        SourceRequest::Default => {
            require_file(&public_flake, "public dotfiles flake")?;
            Ok(PreparedSource {
                directory: public_directory.to_path_buf(),
                kind: SourceKind::Default,
                previous_target: target,
                revision: git_capture(Some(public_directory), &["rev-parse", "HEAD"])?,
                url: None,
            })
        }
        SourceRequest::Remote(url) => {
            let directory = managed_checkout_path(data_root, &url);
            require_file(&directory.join("flake.nix"), "managed system source flake")?;
            Ok(PreparedSource {
                revision: git_capture(Some(&directory), &["rev-parse", "HEAD"])?,
                directory,
                kind: SourceKind::Remote,
                previous_target: target,
                url: Some(url),
            })
        }
        SourceRequest::Current => unreachable!("selection never returns current"),
    }
}

fn inspect_selection(
    public_flakes: &[PathBuf],
    data_root: &Path,
    selection_path: &Path,
) -> Result<(SourceRequest, Option<PathBuf>)> {
    let metadata = match fs::symlink_metadata(selection_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok((SourceRequest::Default, None));
        }
        Err(error) => {
            return Err(error)
                .with_context(|| format!("failed to inspect {}", selection_path.display()));
        }
    };
    if !metadata.file_type().is_symlink() {
        bail!("system source selection is not a symlink");
    }

    let raw_target = fs::read_link(selection_path)
        .with_context(|| format!("failed to read {}", selection_path.display()))?;
    let target = if raw_target.is_absolute() {
        normalize_path(&raw_target)
    } else {
        let parent = selection_path.parent().unwrap_or_else(|| Path::new("/"));
        normalize_path(&parent.join(raw_target))
    };

    if public_flakes.contains(&target) {
        return Ok((SourceRequest::Default, Some(target)));
    }

    let checkout = target
        .parent()
        .context("system source selection has no checkout directory")?;
    let checkout_parent = checkout
        .parent()
        .context("system source checkout has no parent directory")?;
    let checkout_name = checkout
        .file_name()
        .and_then(OsStr::to_str)
        .context("system source checkout has an invalid directory name")?;

    if target.file_name() != Some(OsStr::new("flake.nix"))
        || normalize_path(checkout_parent) != normalize_path(data_root)
        || !is_hex_digest_prefix(checkout_name)
    {
        bail!("system source selection is not managed by dotfiles");
    }

    let url = git_capture(Some(checkout), &["remote", "get-url", "origin"])?;
    validate_git_url(&url)?;
    if normalize_path(&managed_checkout_path(data_root, &url)) != normalize_path(checkout) {
        bail!("system source selection does not match its origin");
    }

    Ok((SourceRequest::Remote(url), Some(target)))
}

fn prepare_remote_checkout(data_root: &Path, git_url: &str) -> Result<(PathBuf, String)> {
    let directory = managed_checkout_path(data_root, git_url);
    fs::create_dir_all(data_root)
        .with_context(|| format!("failed to create {}", data_root.display()))?;
    let mut created = false;

    let result = (|| -> Result<String> {
        match fs::symlink_metadata(&directory) {
            Ok(metadata) => {
                if !metadata.is_dir() {
                    bail!("managed checkout is not a directory");
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                created = true;
                git_clone(git_url, &directory)?;
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("failed to inspect {}", directory.display()));
            }
        }

        if !directory.join(".git").exists() {
            bail!("managed checkout is not a Git repository");
        }
        if git_capture(Some(&directory), &["remote", "get-url", "origin"])? != git_url {
            bail!("managed checkout origin does not match its source URL");
        }
        if !created && !git_capture(Some(&directory), &["status", "--porcelain"])?.is_empty() {
            bail!("managed checkout contains local changes");
        }

        git_run(Some(&directory), &["fetch", "--quiet", "origin"])?;
        git_run(
            Some(&directory),
            &["remote", "set-head", "origin", "--auto"],
        )?;
        let revision = git_capture(Some(&directory), &["rev-parse", "refs/remotes/origin/HEAD"])?;
        git_run(
            Some(&directory),
            &["checkout", "--quiet", "--detach", &revision],
        )?;
        require_file(&directory.join("flake.nix"), "managed system source flake")?;
        Ok(revision)
    })();

    match result {
        Ok(revision) => Ok((directory, revision)),
        Err(error) => {
            if created {
                let _ = fs::remove_dir_all(&directory);
            }
            Err(error)
        }
    }
}

fn git_clone(url: &str, directory: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["clone", "--filter=blob:none", "--no-checkout", "--"])
        .arg(url)
        .arg(directory)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to execute git clone")?;
    ensure_success(output, "git clone").map(|_| ())
}

fn git_capture(repo: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut command = Command::new("git");
    if let Some(repo) = repo {
        command.arg("-C").arg(repo);
    }
    let output = command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to execute git {}", args.join(" ")))?;
    ensure_success(output, &format!("git {}", args.join(" ")))
}

fn git_run(repo: Option<&Path>, args: &[&str]) -> Result<()> {
    git_capture(repo, args).map(|_| ())
}

fn ensure_success(output: std::process::Output, label: &str) -> Result<String> {
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            bail!("{label} exited with status {}", output.status);
        }
        bail!("{stderr}");
    }
    Ok(String::from_utf8(output.stdout)
        .context("command output was not UTF-8")?
        .trim()
        .to_owned())
}

fn validate_git_url(value: &str) -> Result<()> {
    if value
        .chars()
        .any(|character| character.is_whitespace() || matches!(character, '\0' | '\r' | '\n'))
    {
        bail!("Git URL must not contain whitespace or control characters");
    }

    if let Some(at) = value.find('@') {
        let after_at = &value[at + 1..];
        if !value.contains("://") {
            if let Some(colon) = after_at.find(':') {
                if !after_at[..colon].is_empty() && !after_at[colon + 1..].is_empty() {
                    return Ok(());
                }
            }
        }
    }

    let url = url::Url::parse(value)
        .map_err(|_| anyhow::anyhow!("Git source must be an SSH or HTTPS clone URL"))?;
    if url.scheme() != "ssh" && url.scheme() != "https" {
        bail!("Git source must use SSH or HTTPS");
    }
    if url.host_str().unwrap_or("").is_empty() || url.path().is_empty() || url.path() == "/" {
        bail!("Git source must identify a remote repository");
    }
    if url.password().is_some() || (url.scheme() == "https" && !url.username().is_empty()) {
        bail!("Git credentials must not be embedded in the URL");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("Git source must use the remote default branch");
    }
    Ok(())
}

fn system_source_data_root(home: &Path, data_home: Option<&Path>) -> PathBuf {
    let root = match data_home {
        Some(path) if path.is_absolute() => path.to_path_buf(),
        _ => home.join(".local/share"),
    };
    root.join("dotfiles/nix-darwin")
}

fn managed_checkout_path(data_root: &Path, git_url: &str) -> PathBuf {
    let digest = sha256_hex(git_url.as_bytes());
    data_root.join(&digest[..24])
}

fn is_hex_digest_prefix(value: &str) -> bool {
    value.len() == 24
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn require_file(path: &Path, description: &str) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("{description} not found at {}", path.display())
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new("/")),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized
}

#[derive(Debug)]
struct User {
    name: String,
    uid: u32,
}

fn current_user() -> Result<User> {
    let name = id_output("-un")?;
    let uid = id_output("-u")?
        .parse::<u32>()
        .context("id -u returned a non-numeric UID")?;
    Ok(User { name, uid })
}

fn id_output(argument: &str) -> Result<String> {
    let output = Command::new("/usr/bin/id")
        .arg(argument)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .context("failed to execute /usr/bin/id")?;
    ensure_success(output, &format!("/usr/bin/id {argument}"))
}

fn exit_code(code: Option<i32>) -> ExitCode {
    match code.and_then(|code| u8::try_from(code).ok()) {
        Some(code) => ExitCode::from(code),
        None => ExitCode::from(1),
    }
}

fn sha256_hex(input: &[u8]) -> String {
    const INITIAL: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    let bit_length = (input.len() as u64).wrapping_mul(8);
    let mut padded = input.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_length.to_be_bytes());

    let mut hash = INITIAL;
    for chunk in padded.chunks_exact(64) {
        let mut words = [0u32; 64];
        for (index, word) in words.iter_mut().take(16).enumerate() {
            let offset = index * 4;
            *word = u32::from_be_bytes([
                chunk[offset],
                chunk[offset + 1],
                chunk[offset + 2],
                chunk[offset + 3],
            ]);
        }
        for index in 16..64 {
            let s0 = words[index - 15].rotate_right(7)
                ^ words[index - 15].rotate_right(18)
                ^ (words[index - 15] >> 3);
            let s1 = words[index - 2].rotate_right(17)
                ^ words[index - 2].rotate_right(19)
                ^ (words[index - 2] >> 10);
            words[index] = words[index - 16]
                .wrapping_add(s0)
                .wrapping_add(words[index - 7])
                .wrapping_add(s1);
        }

        let mut a = hash[0];
        let mut b = hash[1];
        let mut c = hash[2];
        let mut d = hash[3];
        let mut e = hash[4];
        let mut f = hash[5];
        let mut g = hash[6];
        let mut h = hash[7];

        for index in 0..64 {
            let sum1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let choose = (e & f) ^ ((!e) & g);
            let temp1 = h
                .wrapping_add(sum1)
                .wrapping_add(choose)
                .wrapping_add(K[index])
                .wrapping_add(words[index]);
            let sum0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let majority = (a & b) ^ (a & c) ^ (b & c);
            let temp2 = sum0.wrapping_add(majority);

            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(temp1);
            d = c;
            c = b;
            b = a;
            a = temp1.wrapping_add(temp2);
        }

        hash[0] = hash[0].wrapping_add(a);
        hash[1] = hash[1].wrapping_add(b);
        hash[2] = hash[2].wrapping_add(c);
        hash[3] = hash[3].wrapping_add(d);
        hash[4] = hash[4].wrapping_add(e);
        hash[5] = hash[5].wrapping_add(f);
        hash[6] = hash[6].wrapping_add(g);
        hash[7] = hash[7].wrapping_add(h);
    }

    let mut result = String::with_capacity(64);
    for word in hash {
        write!(&mut result, "{word:08x}").expect("writing to String cannot fail");
    }
    result
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        managed_checkout_path, sha256_hex, source_request, system_source_data_root,
        validate_git_url, SourceRequest,
    };

    #[test]
    fn accepts_supported_git_urls() {
        for url in [
            "git@example.test:owner/config.git",
            "ssh://git@example.test/owner/config.git",
            "https://example.test/owner/config.git",
        ] {
            validate_git_url(url).expect(url);
        }
    }

    #[test]
    fn rejects_unsafe_git_urls() {
        for url in [
            "https://token@example.test/owner/config.git",
            "https://example.test/owner/config.git?ref=main",
            "ssh://git@example.test/owner/config.git#main",
            "git@example.test:",
            "../config",
        ] {
            assert!(validate_git_url(url).is_err(), "{url} should be rejected");
        }
    }

    #[test]
    fn parses_source_request() {
        assert_eq!(source_request(false, None).unwrap(), SourceRequest::Current);
        assert_eq!(source_request(true, None).unwrap(), SourceRequest::Default);
        assert_eq!(
            source_request(false, Some("git@example.test:owner/config.git".into())).unwrap(),
            SourceRequest::Remote("git@example.test:owner/config.git".into())
        );
        assert!(source_request(true, Some("git@example.test:owner/config.git".into())).is_err());
    }

    #[test]
    fn matches_previous_managed_checkout_digest() {
        let path =
            managed_checkout_path(Path::new("/tmp/data"), "git@example.test:owner/config.git");
        assert_eq!(path, Path::new("/tmp/data/8aa2680b443b115c8db9d9f4"));
    }

    #[test]
    fn chooses_xdg_data_home_only_when_absolute() {
        assert_eq!(
            system_source_data_root(Path::new("/Users/test"), Some(Path::new("/custom/data"))),
            Path::new("/custom/data/dotfiles/nix-darwin")
        );
        assert_eq!(
            system_source_data_root(Path::new("/Users/test"), Some(Path::new("relative"))),
            Path::new("/Users/test/.local/share/dotfiles/nix-darwin")
        );
    }

    #[test]
    fn sha256_vectors() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
