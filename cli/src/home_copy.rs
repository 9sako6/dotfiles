use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone)]
pub struct CopyPlan {
    entries: Vec<CopyEntry>,
}

#[derive(Debug, Clone)]
struct CopyEntry {
    relative: PathBuf,
    source: PathBuf,
    destination: PathBuf,
}

impl CopyPlan {
    pub fn preview(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }
        let mut lines = vec!["home copy plan:".to_owned()];
        lines.extend(self.entries.iter().map(|entry| {
            format!(
                "  {} -> {}",
                entry.relative.display(),
                entry.destination.display()
            )
        }));
        lines.join("\n")
    }

    pub fn apply(&self) -> Result<()> {
        for entry in &self.entries {
            sync_entry(&entry.source, &entry.destination).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    entry.relative.display(),
                    entry.destination.display()
                )
            })?;
        }
        Ok(())
    }
}

pub fn plan(repo_root: &Path, home: &Path) -> Result<CopyPlan> {
    let config_path = repo_root.join(".dotfiles.json");
    let raw = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;
    let paths = parse_config(&raw)?;
    let source_root = repo_root.join("home");

    let mut entries = Vec::with_capacity(paths.len());
    for relative in paths {
        let relative_path = PathBuf::from(&relative);
        let source = source_root.join(&relative_path);
        let metadata = fs::symlink_metadata(&source).with_context(|| {
            format!(
                ".dotfiles.json: copy source does not exist: {}",
                relative_path.display()
            )
        })?;
        if metadata.file_type().is_symlink() {
            bail!(
                ".dotfiles.json: copy source must not be a symlink: {}",
                relative_path.display()
            );
        }
        if !metadata.is_file() && !metadata.is_dir() {
            bail!(
                ".dotfiles.json: copy source must be a file or directory: {}",
                relative_path.display()
            );
        }
        entries.push(CopyEntry {
            source,
            destination: home.join(&relative_path),
            relative: relative_path,
        });
    }

    Ok(CopyPlan { entries })
}

fn parse_config(raw: &str) -> Result<Vec<String>> {
    let mut parser = JsonParser::new(raw);
    parser.skip_whitespace();
    parser.expect_byte(b'{')?;
    parser.skip_whitespace();

    let mut copy = None;
    if !parser.consume_byte(b'}') {
        loop {
            parser.skip_whitespace();
            let key = parser.parse_string()?;
            parser.skip_whitespace();
            parser.expect_byte(b':')?;
            parser.skip_whitespace();
            match key.as_str() {
                "copy" => {
                    if copy.is_some() {
                        bail!(".dotfiles.json: duplicate key: copy");
                    }
                    copy = Some(parser.parse_string_array()?);
                }
                _ => bail!(".dotfiles.json: unknown key: {key}; allowed: copy"),
            }
            parser.skip_whitespace();
            if parser.consume_byte(b'}') {
                break;
            }
            parser.expect_byte(b',')?;
        }
    }
    parser.skip_whitespace();
    if !parser.is_eof() {
        bail!(".dotfiles.json: trailing content after root object");
    }

    let copy = copy.unwrap_or_default();
    validate_paths(&copy)?;
    Ok(copy)
}

fn validate_paths(paths: &[String]) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut previous: Option<&str> = None;

    for value in paths {
        validate_relative_path(value)?;
        if !seen.insert(value.as_str()) {
            bail!(".dotfiles.json: copy contains duplicate entry: {value}");
        }
        if let Some(previous) = previous {
            if value.as_str() < previous {
                bail!(
                    ".dotfiles.json: copy entries must be alphabetical; {value} should come before {previous}"
                );
            }
        }
        previous = Some(value);
    }

    for (index, path) in paths.iter().enumerate() {
        let path = Path::new(path);
        for other in &paths[index + 1..] {
            let other = Path::new(other);
            if other.starts_with(path) || path.starts_with(other) {
                bail!(
                    ".dotfiles.json: copy entries must not overlap: {} and {}",
                    path.display(),
                    other.display()
                );
            }
        }
    }
    Ok(())
}

fn validate_relative_path(value: &str) -> Result<()> {
    if value.is_empty() {
        bail!(".dotfiles.json: copy entries must not be empty");
    }
    let path = Path::new(value);
    if path.is_absolute() {
        bail!(".dotfiles.json: copy entry must be relative: {value}");
    }
    for component in path.components() {
        if !matches!(component, Component::Normal(_)) {
            bail!(".dotfiles.json: invalid copy entry: {value}");
        }
    }
    Ok(())
}

fn sync_entry(source: &Path, destination: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(source)
        .with_context(|| format!("failed to inspect {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        bail!("copy source contains a symlink: {}", source.display());
    }
    if metadata.is_file() {
        sync_file(source, destination)
    } else if metadata.is_dir() {
        sync_directory(source, destination)
    } else {
        bail!("unsupported copy source: {}", source.display())
    }
}

fn sync_file(source: &Path, destination: &Path) -> Result<()> {
    if let Some(parent) = destination.parent() {
        ensure_directory(parent)?;
    }
    if fs::symlink_metadata(destination).is_ok() {
        remove_entry(destination)?;
    }
    fs::copy(source, destination).with_context(|| {
        format!(
            "failed to copy {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

fn sync_directory(source: &Path, destination: &Path) -> Result<()> {
    ensure_directory(destination)?;

    let mut source_names = BTreeSet::new();
    let mut source_entries = read_entries(source)?;
    source_entries.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
    for entry in source_entries {
        let name = entry.file_name();
        source_names.insert(name.clone());
        sync_entry(&entry.path(), &destination.join(&name))?;
    }

    for entry in read_entries(destination)? {
        if !source_names.contains(&entry.file_name()) {
            remove_entry(&entry.path())?;
        }
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => {
            remove_entry(path)?;
            fs::create_dir_all(path)
                .with_context(|| format!("failed to create {}", path.display()))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fs::create_dir_all(path)
            .with_context(|| format!("failed to create {}", path.display())),
        Err(error) => Err(error).with_context(|| format!("failed to inspect {}", path.display())),
    }
}

fn read_entries(path: &Path) -> Result<Vec<fs::DirEntry>> {
    fs::read_dir(path)
        .with_context(|| format!("failed to read {}", path.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read {}", path.display()))
}

fn remove_entry(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to inspect {}", path.display()))?;
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove {}", path.display()))
    } else {
        fs::remove_file(path).with_context(|| format!("failed to remove {}", path.display()))
    }
}

struct JsonParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> JsonParser<'a> {
    fn new(raw: &'a str) -> Self {
        Self {
            bytes: raw.as_bytes(),
            position: 0,
        }
    }

    fn is_eof(&self) -> bool {
        self.position == self.bytes.len()
    }

    fn skip_whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn expect_byte(&mut self, expected: u8) -> Result<()> {
        if self.consume_byte(expected) {
            Ok(())
        } else {
            bail!(
                ".dotfiles.json: expected '{}' at byte {}",
                expected as char,
                self.position
            )
        }
    }

    fn parse_string_array(&mut self) -> Result<Vec<String>> {
        self.expect_byte(b'[')?;
        self.skip_whitespace();
        let mut values = Vec::new();
        if self.consume_byte(b']') {
            return Ok(values);
        }
        loop {
            self.skip_whitespace();
            values.push(self.parse_string()?);
            self.skip_whitespace();
            if self.consume_byte(b']') {
                break;
            }
            self.expect_byte(b',')?;
        }
        Ok(values)
    }

    fn parse_string(&mut self) -> Result<String> {
        self.expect_byte(b'"')?;
        let mut result = String::new();
        while let Some(byte) = self.peek() {
            self.position += 1;
            match byte {
                b'"' => return Ok(result),
                b'\\' => {
                    let escaped = self.peek().context(".dotfiles.json: incomplete escape")?;
                    self.position += 1;
                    match escaped {
                        b'"' => result.push('"'),
                        b'\\' => result.push('\\'),
                        b'/' => result.push('/'),
                        b'b' => result.push('\u{0008}'),
                        b'f' => result.push('\u{000c}'),
                        b'n' => result.push('\n'),
                        b'r' => result.push('\r'),
                        b't' => result.push('\t'),
                        b'u' => result.push(self.parse_unicode_escape()?),
                        _ => bail!(
                            ".dotfiles.json: invalid escape at byte {}",
                            self.position - 1
                        ),
                    }
                }
                0x00..=0x1f => bail!(
                    ".dotfiles.json: control character in string at byte {}",
                    self.position - 1
                ),
                0x20..=0x7f => result.push(byte as char),
                _ => {
                    let start = self.position - 1;
                    let remaining = std::str::from_utf8(&self.bytes[start..])
                        .context(".dotfiles.json: invalid UTF-8")?;
                    let character = remaining
                        .chars()
                        .next()
                        .context(".dotfiles.json: incomplete UTF-8")?;
                    self.position = start + character.len_utf8();
                    result.push(character);
                }
            }
        }
        bail!(".dotfiles.json: unterminated string")
    }

    fn parse_unicode_escape(&mut self) -> Result<char> {
        let first = self.parse_hex_quad()?;
        if (0xd800..=0xdbff).contains(&first) {
            self.expect_byte(b'\\')?;
            self.expect_byte(b'u')?;
            let second = self.parse_hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                bail!(".dotfiles.json: invalid Unicode surrogate pair");
            }
            let codepoint =
                0x10000 + (((first as u32 - 0xd800) << 10) | (second as u32 - 0xdc00));
            char::from_u32(codepoint).context(".dotfiles.json: invalid Unicode escape")
        } else if (0xdc00..=0xdfff).contains(&first) {
            bail!(".dotfiles.json: unexpected low Unicode surrogate")
        } else {
            char::from_u32(first as u32).context(".dotfiles.json: invalid Unicode escape")
        }
    }

    fn parse_hex_quad(&mut self) -> Result<u16> {
        let end = self.position + 4;
        if end > self.bytes.len() {
            bail!(".dotfiles.json: incomplete Unicode escape");
        }
        let mut value = 0u16;
        for byte in &self.bytes[self.position..end] {
            value = (value << 4)
                | match byte {
                    b'0'..=b'9' => (byte - b'0') as u16,
                    b'a'..=b'f' => (byte - b'a' + 10) as u16,
                    b'A'..=b'F' => (byte - b'A' + 10) as u16,
                    _ => bail!(".dotfiles.json: invalid Unicode escape"),
                };
        }
        self.position = end;
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::os::unix::fs::symlink;

    use super::{parse_config, plan};

    #[test]
    fn parses_copy_paths() {
        let raw = r#"{
          "copy": [
            ".agents/skills",
            ".claude/settings.json"
          ]
        }"#;
        assert_eq!(
            parse_config(raw).unwrap(),
            vec![".agents/skills", ".claude/settings.json"]
        );
    }

    #[test]
    fn rejects_invalid_config() {
        for raw in [
            r#"{"copi": []}"#,
            r#"{"copy": ["b", "a"]}"#,
            r#"{"copy": ["a", "a"]}"#,
            r#"{"copy": ["../secret"]}"#,
            r#"{"copy": [".claude", ".claude/settings.json"]}"#,
        ] {
            assert!(parse_config(raw).is_err(), "should reject {raw}");
        }
    }

    #[test]
    fn apply_replaces_store_links_and_prunes_owned_directory_only() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        let home = temp.path().join("home-target");
        fs::create_dir_all(repo.join("home/.claude/skills/design-it")).unwrap();
        fs::write(
            repo.join("home/.claude/skills/design-it/SKILL.md"),
            "new\n",
        )
        .unwrap();
        fs::write(
            repo.join(".dotfiles.json"),
            "{\n  \"copy\": [\n    \".claude/skills\"\n  ]\n}\n",
        )
        .unwrap();

        fs::create_dir_all(home.join(".claude/skills/old")).unwrap();
        fs::write(home.join(".claude/skills/old/SKILL.md"), "old\n").unwrap();
        fs::write(home.join(".claude/runtime.json"), "keep\n").unwrap();
        let store_file = temp.path().join("store-skill");
        fs::write(&store_file, "store\n").unwrap();
        fs::create_dir_all(home.join(".claude/skills/design-it")).unwrap();
        symlink(
            &store_file,
            home.join(".claude/skills/design-it/SKILL.md"),
        )
        .unwrap();

        let plan = plan(&repo, &home).unwrap();
        plan.apply().unwrap();

        let deployed = home.join(".claude/skills/design-it/SKILL.md");
        assert_eq!(fs::read_to_string(&deployed).unwrap(), "new\n");
        assert!(
            !fs::symlink_metadata(&deployed)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!home.join(".claude/skills/old").exists());
        assert_eq!(
            fs::read_to_string(home.join(".claude/runtime.json")).unwrap(),
            "keep\n"
        );
    }
}
