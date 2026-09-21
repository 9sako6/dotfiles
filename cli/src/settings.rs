use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;
use tabled::builder::Builder;
use tabled::settings::object::{Columns, Rows};
use tabled::settings::peaker::PriorityMax;
use tabled::settings::{Format, Modify, Padding, Remove, Style, Width};

use crate::system;

#[derive(Deserialize)]
pub struct Setting {
    key: String,
    value: Value,
    source: Option<String>,
}

pub fn run(root: &Path) -> Result<ExitCode> {
    let (settings, inventory) = system::load_settings(root)?;
    let width = crate::terminal_width();
    let report = format!(
        "{}\n{}\n",
        render(&settings, width),
        crate::inventory::report(inventory, width)?
    );
    let mut output = io::stdout().lock();
    output.write_all(report.as_bytes())?;
    output.flush()?;
    Ok(ExitCode::SUCCESS)
}

fn format_value(value: &Value) -> String {
    match value {
        Value::Array(values) => format!(
            "[{}\n]",
            values
                .iter()
                .map(|value| format!("\n  {}", format_value(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
        _ => value.to_string(),
    }
}

pub(crate) fn render(settings: &[Setting], terminal_width: Option<usize>) -> String {
    let mut output = format!(
        "{}\n",
        console::style("settings")
            .magenta()
            .italic()
            .force_styling(terminal_width.is_some() && console::colors_enabled())
    );
    let mut builder = Builder::default();
    builder.push_record(["key", "value", "source"]);
    for setting in settings {
        builder.push_record([
            setting.key.clone(),
            format_value(&setting.value),
            setting.source.clone().unwrap_or_default(),
        ]);
    }
    let mut table = builder.build();
    table.with(Style::empty());
    if let Some(width) = terminal_width {
        table.with(Modify::new(Rows::first()).with(Format::content(|header| {
            console::style(header).italic().magenta().to_string()
        })));
        table
            .with(Width::wrap(width).priority(PriorityMax::default()))
            .with(Width::increase(width));
    } else {
        table.with(Remove::row(Rows::first()));
    }
    table
        .with(Modify::new(Columns::first()).with(Padding::new(0, 1, 0, 0)))
        .with(Modify::new(Columns::last()).with(Padding::zero()));
    for line in table.to_string().lines() {
        output.push_str(line.trim_end());
        output.push('\n');
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn piped_output_displays_all_values_without_column_headers_or_duplicate_rows() {
        let settings: Vec<Setting> = serde_json::from_value(json!([
            {"key": "copy", "value": ["a", "b"], "source": "dotfiles.toml"},
            {"key": "localllm.default_model", "value": null, "source": null},
            {"key": "localllm.enabled", "value": false, "source": "dotfiles.local.toml"},
            {"key": "localllm.models", "value": [], "source": "dotfiles.local.toml"},
            {"key": "private.path", "value": "../private", "source": "dotfiles.local.toml"}
        ]))
        .unwrap();
        assert_eq!(
            render(&settings, None),
            concat!(
                "settings\n",
                "copy                    [            dotfiles.toml\n",
                "                          \"a\",\n",
                "                          \"b\"\n",
                "                        ]\n",
                "localllm.default_model  null\n",
                "localllm.enabled        false        dotfiles.local.toml\n",
                "localllm.models         [            dotfiles.local.toml\n",
                "                        ]\n",
                "private.path            \"../private\" dotfiles.local.toml\n"
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
