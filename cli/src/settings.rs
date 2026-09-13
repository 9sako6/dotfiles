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
    let settings = system::load_settings(root)?;
    let width = console::user_attended().then(|| usize::from(console::Term::stdout().size().1));
    write_settings(&mut io::stdout().lock(), &settings, width)?;
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

fn format_cell(value: &Value, width: Option<usize>) -> String {
    let inline = format_value(value);
    if let Value::Array(values) = value {
        if !values.is_empty()
            && width.is_some_and(|width| console::measure_text_width(&inline) > width)
        {
            let items = values
                .iter()
                .map(|value| format!("  {}", format_value(value)))
                .collect::<Vec<_>>()
                .join(",\n");
            return format!("[\n{items}\n]");
        }
    }
    inline
}

fn write_settings(
    output: &mut impl Write,
    settings: &[Setting],
    terminal_width: Option<usize>,
) -> Result<()> {
    let key_width = settings
        .iter()
        .map(|setting| console::measure_text_width(&setting.key))
        .max()
        .unwrap_or(0)
        .max("Key".len());
    let source_width = settings
        .iter()
        .map(|setting| console::measure_text_width(setting.source.as_deref().unwrap_or_default()))
        .max()
        .unwrap_or(0)
        .max("Source".len());
    let value_width =
        terminal_width.map(|width| width.saturating_sub(key_width + source_width + 3));
    let mut builder = Builder::default();
    builder.push_record(["Key", "Value", "Source"]);
    for setting in settings {
        builder.push_record([
            setting.key.clone(),
            format_cell(&setting.value, value_width),
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
        writeln!(output, "{}", line.trim_end())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn piped_output_displays_all_values_without_headers_or_duplicate_rows() {
        let settings: Vec<Setting> = serde_json::from_value(json!([
            {"key": "copy", "value": ["a", "b"], "source": "dotfiles.toml"},
            {"key": "localllm.default_model", "value": null, "source": null},
            {"key": "localllm.enabled", "value": false, "source": "dotfiles.local.toml"},
            {"key": "localllm.models", "value": [], "source": "dotfiles.local.toml"},
            {"key": "private.path", "value": "../private", "source": "dotfiles.local.toml"}
        ]))
        .unwrap();
        let mut output = Vec::new();
        write_settings(&mut output, &settings, None).unwrap();
        assert_eq!(
            String::from_utf8(output).unwrap(),
            concat!(
                "copy                    [\"a\", \"b\"]   dotfiles.toml\n",
                "localllm.default_model  null\n",
                "localllm.enabled        false        dotfiles.local.toml\n",
                "localllm.models         []           dotfiles.local.toml\n",
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
