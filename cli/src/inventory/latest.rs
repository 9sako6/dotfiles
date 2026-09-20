use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::Package;

type Update = (Vec<usize>, String);
type LookupResult = Result<String, String>;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Query {
    Brew { name: String, cask: bool },
    Github(String),
    GitTags(String),
    Go,
    Mise(String),
    Rust,
    Unsupported,
}

pub(super) struct Checks {
    receiver: mpsc::Receiver<Update>,
    cancelled: Arc<AtomicBool>,
    workers: Vec<JoinHandle<()>>,
}

impl Checks {
    pub(super) fn start(packages: &[Package]) -> Self {
        let mut queries: BTreeMap<Query, Vec<usize>> = BTreeMap::new();
        for (index, package) in packages.iter().enumerate() {
            queries.entry(query(package)).or_default().push(index);
        }
        let queue = Arc::new(Mutex::new(VecDeque::from_iter(queries)));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (sender, receiver) = mpsc::channel();
        let deadline = Instant::now() + Duration::from_secs(12);
        let workers = (0..16)
            .map(|_| {
                let queue = queue.clone();
                let cancelled = cancelled.clone();
                let sender = sender.clone();
                thread::spawn(move || loop {
                    let next = queue.lock().unwrap().pop_front();
                    let Some((query, indices)) = next else { break };
                    let result = lookup(&query, deadline, &cancelled)
                        .unwrap_or_else(|error| format!("error: {error}"));
                    if sender.send((indices, result)).is_err() {
                        break;
                    }
                })
            })
            .collect();
        Self {
            receiver,
            cancelled,
            workers,
        }
    }

    pub(super) fn poll(&self) -> Result<Update, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }

    pub(super) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Drop for Checks {
    fn drop(&mut self) {
        self.cancel();
        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

fn query(package: &Package) -> Query {
    let field = |name| package.lookup[name].as_str().unwrap_or_default().to_owned();
    match package.lookup["kind"].as_str() {
        Some("mise") => Query::Mise(field("tool")),
        Some("brew") => Query::Brew {
            name: field("name"),
            cask: package.lookup["cask"].as_bool().unwrap_or(false),
        },
        _ => match package.name.as_str() {
            "antigravity-cli" => Query::Github("google-antigravity/antigravity-cli".into()),
            "cargo" | "clippy" | "rustc" | "rustfmt" => Query::Rust,
            "go" => Query::Go,
            "awscli2" => Query::Github("aws/aws-cli".into()),
            "ffmpeg" => Query::Github("FFmpeg/FFmpeg".into()),
            "git" => Query::Github("git/git".into()),
            "anki-addon-anki-connect" => {
                Query::GitTags("https://git.sr.ht/~foosoft/anki-connect".into())
            }
            _ => package.lookup["urls"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .find_map(github_repository)
                .map(Query::Github)
                .unwrap_or(Query::Unsupported),
        },
    }
}

fn github_repository(raw: &str) -> Option<String> {
    let url = url::Url::parse(raw).ok()?;
    if !matches!(url.host_str()?, "github.com" | "www.github.com") {
        return None;
    }
    let parts: Vec<_> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    if parts.len() < 2 {
        return None;
    }
    Some(format!(
        "{}/{}",
        parts[0],
        parts[1].trim_end_matches(".git")
    ))
}

fn lookup(query: &Query, deadline: Instant, cancelled: &AtomicBool) -> LookupResult {
    match query {
        Query::GitTags(repository) => {
            let mut command = Command::new("git");
            command
                .args([
                    "-c",
                    "credential.helper=",
                    "ls-remote",
                    "--tags",
                    "--refs",
                    repository,
                ])
                .env("GIT_TERMINAL_PROMPT", "0");
            let output = run(&mut command, deadline, cancelled)?;
            newest(
                output
                    .lines()
                    .filter_map(|line| line.split_once("refs/tags/").map(|(_, tag)| tag)),
            )
        }
        Query::Unsupported => Err("unsupported source".into()),
        Query::Mise(tool) => {
            let mut command = Command::new("mise");
            command
                .args(["latest", tool])
                .current_dir(std::env::temp_dir())
                .env("MISE_FETCH_REMOTE_VERSIONS_CACHE", "0s")
                .env("MISE_FETCH_REMOTE_VERSIONS_TIMEOUT", "8s")
                .env("MISE_HTTP_TIMEOUT", "8s")
                .env("MISE_AUTO_INSTALL", "false")
                .env("MISE_COLOR", "false");
            version(&run(&mut command, deadline, cancelled)?)
        }
        Query::Github(repository) => {
            let release = github(
                &format!("repos/{repository}/releases/latest"),
                deadline,
                cancelled,
            );
            match release {
                Ok(value) => version(value["tag_name"].as_str().ok_or("invalid response")?),
                Err(error) if error == "HTTP 404" => {
                    let tags = github(
                        &format!("repos/{repository}/tags?per_page=100"),
                        deadline,
                        cancelled,
                    )?;
                    newest(
                        tags.as_array()
                            .ok_or("invalid response")?
                            .iter()
                            .filter_map(|tag| tag["name"].as_str()),
                    )
                }
                Err(error) => Err(error),
            }
        }
        Query::Go => {
            let value: Value = json(&fetch("https://go.dev/dl/?mode=json", deadline, cancelled)?)?;
            version(
                value
                    .as_array()
                    .and_then(|items| items.iter().find(|item| item["stable"] == true))
                    .and_then(|v| v["version"].as_str())
                    .ok_or("invalid response")?,
            )
        }
        Query::Rust => {
            let text = fetch(
                "https://static.rust-lang.org/dist/channel-rust-stable.toml",
                deadline,
                cancelled,
            )?;
            let section = text
                .split("[pkg.rust]\n")
                .nth(1)
                .ok_or("invalid response")?;
            let value = section
                .lines()
                .take_while(|line| !line.starts_with('['))
                .find_map(|line| line.strip_prefix("version = \""))
                .ok_or("invalid response")?;
            version(
                value
                    .split_whitespace()
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches('"'),
            )
        }
        Query::Brew { name, cask } => {
            if name.contains('/') {
                let parts: Vec<_> = name.split('/').collect();
                if parts.len() != 3 {
                    return Err("unsupported tap".into());
                }
                let path = if *cask { "Casks" } else { "Formula" };
                let body = fetch(
                    &format!(
                        "https://raw.githubusercontent.com/{}/homebrew-{}/HEAD/{path}/{}.rb",
                        parts[0], parts[1], parts[2]
                    ),
                    deadline,
                    cancelled,
                )?;
                let value = body
                    .lines()
                    .find_map(|line| line.trim().strip_prefix("version \""))
                    .and_then(|line| line.split('"').next())
                    .ok_or("unsupported tap version")?;
                version(value)
            } else {
                let kind = if *cask { "cask" } else { "formula" };
                let value = json(&fetch(
                    &format!("https://formulae.brew.sh/api/{kind}/{name}.json"),
                    deadline,
                    cancelled,
                )?)?;
                version(
                    if *cask {
                        &value["version"]
                    } else {
                        &value["versions"]["stable"]
                    }
                    .as_str()
                    .ok_or("invalid response")?,
                )
            }
        }
    }
}

fn github(path: &str, deadline: Instant, cancelled: &AtomicBool) -> Result<Value, String> {
    let mut command = Command::new("gh");
    command.args(["api", "--hostname", "github.com", path]);
    match run(&mut command, deadline, cancelled) {
        Ok(value) => json(&value),
        Err(error) if error == "command unavailable" || error == "command failed" => json(&fetch(
            &format!("https://api.github.com/{path}"),
            deadline,
            cancelled,
        )?),
        Err(error) => Err(error),
    }
}

fn json(text: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|_| "invalid response".into())
}

fn fetch(url: &str, deadline: Instant, cancelled: &AtomicBool) -> LookupResult {
    let mut command = Command::new("/usr/bin/curl");
    command.args([
        "--disable",
        "--silent",
        "--show-error",
        "--location",
        "--fail-with-body",
        "--connect-timeout",
        "3",
        "--max-time",
        "8",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        url,
    ]);
    run(&mut command, deadline, cancelled)
}

fn version(raw: &str) -> LookupResult {
    let raw = raw.trim();
    let start = raw
        .find(|c: char| c.is_ascii_digit())
        .ok_or("invalid version")?;
    let value = &raw[start..];
    if value.len() > 64
        || !value.contains('.')
        || !value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+_,".contains(c))
    {
        return Err("invalid version".into());
    }
    Ok(value.into())
}

fn newest<'a>(tags: impl Iterator<Item = &'a str>) -> LookupResult {
    let mut versions: Vec<_> = tags
        .filter_map(|tag| version(tag).ok())
        .filter(|v| v.split('.').all(|part| part.parse::<u64>().is_ok()))
        .collect();
    versions.sort_by_key(|v| {
        v.split('.')
            .map(|part| part.parse::<u64>().unwrap())
            .collect::<Vec<_>>()
    });
    versions.pop().ok_or("no stable release".into())
}

fn run(command: &mut Command, deadline: Instant, cancelled: &AtomicBool) -> LookupResult {
    if cancelled.load(Ordering::Relaxed) {
        return Err("cancelled".into());
    }
    if Instant::now() >= deadline {
        return Err("timeout".into());
    }
    let output = tempfile::NamedTempFile::new().map_err(|_| "temporary output unavailable")?;
    let errors = tempfile::NamedTempFile::new().map_err(|_| "temporary output unavailable")?;
    command
        .env("NO_COLOR", "1")
        .env_remove("CLICOLOR_FORCE")
        .env_remove("FORCE_COLOR")
        .env_remove("GH_FORCE_TTY")
        .stdin(Stdio::null())
        .stdout(
            output
                .reopen()
                .map_err(|_| "temporary output unavailable")?,
        )
        .stderr(
            errors
                .reopen()
                .map_err(|_| "temporary output unavailable")?,
        )
        .process_group(0);
    let mut child = command.spawn().map_err(|_| "command unavailable")?;
    loop {
        if cancelled.load(Ordering::Relaxed) || Instant::now() >= deadline {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            return Err(if cancelled.load(Ordering::Relaxed) {
                "cancelled"
            } else {
                "timeout"
            }
            .into());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                if status.success() {
                    if output
                        .as_file()
                        .metadata()
                        .map_err(|_| "output unavailable")?
                        .len()
                        > 4_000_000
                    {
                        return Err("response too large".into());
                    }
                    return fs::read_to_string(output.path())
                        .map_err(|_| "invalid response".into());
                }
                let error = fs::read_to_string(errors.path()).unwrap_or_default();
                if status.code() == Some(28) || error.to_lowercase().contains("timed out") {
                    return Err("timeout".into());
                }
                for code in [401, 403, 404, 429, 500, 502, 503] {
                    if error.contains(&code.to_string()) {
                        return Err(format!("HTTP {code}"));
                    }
                }
                return Err("command failed".into());
            }
            Ok(None) => thread::sleep(Duration::from_millis(20)),
            Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("command failed".into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_versions_without_accepting_terminal_controls_or_empty_results() {
        assert_eq!(version("bun-v1.2.3").unwrap(), "1.2.3");
        assert_eq!(version("go1.24.2\n").unwrap(), "1.24.2");
        assert!(version("1.2.3\u{1b}[2J").is_err());
        assert!(version("").is_err());
        assert_eq!(
            github_repository("https://github.com/a/b/releases/download/v1.2/x"),
            Some("a/b".into())
        );
        assert_eq!(github_repository("https://github.com.evil.test/a/b"), None);
    }

    #[test]
    fn structured_responses_remain_plain_when_the_parent_forces_color() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "if [ -n \"${CLICOLOR_FORCE-}\" ]; then printf '\\033[35m'; fi; printf '{\"version\":\"1.2.3\"}'"])
            .env("CLICOLOR_FORCE", "1");
        let text = run(
            &mut command,
            Instant::now() + Duration::from_secs(2),
            &AtomicBool::new(false),
        )
        .unwrap();
        assert_eq!(json(&text).unwrap()["version"], "1.2.3");
    }

    #[test]
    fn timeout_kills_the_process_group_and_reaps_the_child() {
        let cancelled = AtomicBool::new(false);
        let start = Instant::now();
        let result = run(
            Command::new("/bin/sh").args(["-c", "sleep 30 & wait"]),
            start + Duration::from_millis(100),
            &cancelled,
        );
        assert_eq!(result.unwrap_err(), "timeout");
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}
