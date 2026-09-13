use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

use crate::system;

#[derive(Deserialize)]
pub struct Setting {
    key: String,
    value: Value,
    source: Option<String>,
}

pub fn run(root: &Path) -> Result<ExitCode> {
    let settings = system::load_settings(root)?;
    write_settings(&mut io::stdout().lock(), &settings)?;
    Ok(ExitCode::SUCCESS)
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(format_value)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => value.to_string(),
    }
}

fn write_settings(output: &mut impl Write, settings: &[Setting]) -> Result<()> {
    let rows: Vec<_> = settings
        .iter()
        .map(|setting| (setting, format_value(&setting.value)))
        .collect();
    let key_width = rows
        .iter()
        .map(|(setting, _)| setting.key.chars().count())
        .max()
        .unwrap_or(0);
    let value_width = rows
        .iter()
        .map(|(_, value)| value.chars().count())
        .max()
        .unwrap_or(0);
    for (setting, value) in rows {
        let key = &setting.key;
        if let Some(source) = &setting.source {
            writeln!(output, "{key:<key_width$}  {value:<value_width$}  {source}")?;
        } else {
            writeln!(output, "{key:<key_width$}  {value}")?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn displays_all_values_and_sources_without_headers_or_duplicate_rows() {
        let settings: Vec<Setting> = serde_json::from_value(json!([
            {"key": "copy", "value": ["a", "b"], "source": "dotfiles.toml"},
            {"key": "localllm.default_model", "value": null, "source": null},
            {"key": "localllm.enabled", "value": false, "source": "dotfiles.local.toml"},
            {"key": "localllm.models", "value": [], "source": "dotfiles.local.toml"},
            {"key": "private.path", "value": "../private", "source": "dotfiles.local.toml"}
        ]))
        .unwrap();
        let mut output = Vec::new();
        write_settings(&mut output, &settings).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "copy                    [\"a\", \"b\"]    dotfiles.toml\n",
                "localllm.default_model  null\n",
                "localllm.enabled        false         dotfiles.local.toml\n",
                "localllm.models         []            dotfiles.local.toml\n",
                "private.path            \"../private\"  dotfiles.local.toml\n"
            )
        );
    }

    #[test]
    fn quotes_strings_and_escapes_control_characters_on_one_line() {
        assert_eq!(
            format_value(&json!("../a\"b\\c\n\t\u{1b}")),
            "\"../a\\\"b\\\\c\\n\\t\\u001b\""
        );
    }
}
