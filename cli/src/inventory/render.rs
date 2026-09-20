use std::collections::BTreeMap;
use std::io::{self, Write};
use std::process::ExitCode;
use std::sync::mpsc::TryRecvError;
use std::time::Duration;

use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyModifiers},
    execute, queue, terminal,
};
use serde_json::Value;
use tabled::builder::Builder;
use tabled::grid::dimension::Estimate;
use tabled::settings::object::{Columns, Rows};
use tabled::settings::peaker::PriorityMax;
use tabled::settings::{Format, Modify, Padding, Style, Width};
use tabled::Table;

use super::{latest::Checks, Inventory, Service};

struct Row {
    key: String,
    cells: Vec<String>,
    change: char,
}

impl Row {
    fn new(key: String, cells: Vec<String>) -> Self {
        Self {
            key,
            cells,
            change: ' ',
        }
    }
}

struct Section {
    path: Vec<String>,
    headers: Vec<&'static str>,
    rows: Vec<Row>,
}

fn sections(inventory: &Inventory, latest: bool) -> Vec<Section> {
    let mut headers = vec!["name", "manager", "current"];
    if latest {
        headers.push("latest");
    }
    let mut sections = vec![Section {
        path: vec!["packages".into()],
        headers,
        rows: inventory
            .packages
            .iter()
            .map(|package| {
                let mut cells = vec![
                    package.name.clone(),
                    package.manager.clone(),
                    package.declared.clone(),
                ];
                if latest {
                    cells.push(package.latest.clone());
                }
                Row::new(format!("{}\0{}", package.name, package.manager), cells)
            })
            .collect(),
    }];
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for setting in &inventory.system {
        groups
            .entry(setting.group.clone())
            .or_default()
            .push(Row::new(
                setting.key.clone(),
                vec![
                    setting.name.clone(),
                    value(&setting.value),
                    setting_description(&setting.name, &setting.value),
                ],
            ));
    }
    sections.extend(groups.into_iter().map(|(group, rows)| Section {
        path: vec!["system".into(), group],
        headers: vec!["setting", "value", "description"],
        rows,
    }));
    sections.push(Section {
        path: vec!["services".into()],
        headers: vec!["name", "mode", "start", "restart", "description"],
        rows: inventory
            .services
            .iter()
            .map(|s| {
                Row::new(
                    format!("{}\0{}", s.name, s.scope),
                    service(s, &inventory.time_zone),
                )
            })
            .collect(),
    });
    sections.push(Section {
        path: vec!["agents".into(), "skills".into()],
        headers: vec!["name", "origin", "description"],
        rows: inventory
            .skills
            .iter()
            .map(|s| {
                Row::new(
                    s.name.clone(),
                    vec![s.name.clone(), s.origin.clone(), s.description.clone()],
                )
            })
            .collect(),
    });
    sections.push(Section {
        path: vec!["agents".into(), "tools".into()],
        headers: vec!["tool", "deploy", "file"],
        rows: inventory
            .tools
            .iter()
            .map(|t| {
                Row::new(
                    t.path.clone(),
                    vec![
                        tool(&t.path).into(),
                        t.deploy.clone(),
                        format!("~/{}", t.path),
                    ],
                )
            })
            .collect(),
    });
    sections.push(Section {
        path: vec!["agents".into(), "localllm".into()],
        headers: vec!["enabled", "default model", "description"],
        rows: vec![Row::new(
            "localllm".into(),
            vec![
                inventory.localllm.enabled.to_string(),
                inventory
                    .localllm
                    .default_model
                    .clone()
                    .unwrap_or_else(|| "—".into()),
                "コマンド実行時のみ推論サーバーを起動し、終了時にプロセスを停止する。".into(),
            ],
        )],
    });
    sections
}

pub(super) fn report(inventory: &Inventory, width: Option<usize>, latest: bool) -> String {
    render_sections(&sections(inventory, latest), width)
}

pub(super) fn diff(before: Option<&Inventory>, after: &Inventory, width: Option<usize>) -> String {
    let mut previous: BTreeMap<_, _> = before
        .into_iter()
        .flat_map(|inventory| sections(inventory, false))
        .map(|section| (section.path.clone(), section))
        .collect();
    let mut changed = Vec::new();
    for mut section in sections(after, false) {
        let old = previous
            .remove(&section.path)
            .map(|s| s.rows)
            .unwrap_or_default();
        section.rows = changed_rows(old, section.rows);
        if !section.rows.is_empty() {
            changed.push(section);
        }
    }
    for (_, mut section) in previous {
        section.rows = changed_rows(section.rows, Vec::new());
        if !section.rows.is_empty() {
            changed.push(section);
        }
    }
    changed.sort_by_key(|section| {
        let rank = match section.path[0].as_str() {
            "packages" => 0,
            "system" => 1,
            "services" => 2,
            _ => 3,
        };
        let subgroup = match section.path.get(1).map(String::as_str) {
            Some("skills") => 0,
            Some("tools") => 1,
            Some("localllm") => 2,
            _ => 0,
        };
        (rank, subgroup, section.path.clone())
    });
    if changed.is_empty() {
        return "no resource changes".into();
    }
    render_sections(&changed, width)
}

fn changed_rows(before: Vec<Row>, mut after: Vec<Row>) -> Vec<Row> {
    let mut rows = Vec::new();
    for mut old in before {
        if let Some(index) = after
            .iter()
            .position(|new| old.key == new.key && old.cells == new.cells)
        {
            after.remove(index);
        } else {
            old.change = '-';
            rows.push(old);
        }
    }
    rows.extend(after.into_iter().map(|mut row| {
        row.change = '+';
        row
    }));
    rows.sort_by_key(|row| (row.key.to_lowercase(), row.key.clone(), row.change != '-'));
    rows
}

fn render_sections(sections: &[Section], width: Option<usize>) -> String {
    let system_table = build_table(
        &["setting", "value", "description"],
        sections
            .iter()
            .filter(|s| s.path[0] == "system")
            .flat_map(|s| s.rows.iter().map(|r| r.cells.clone())),
        width.map(|w| w.saturating_sub(4)),
        None,
    );
    let mut system_dimensions = system_table.get_dimension().clone();
    system_dimensions.estimate(system_table.get_records(), system_table.get_config());
    let mut output = String::new();
    let mut previous: &[String] = &[];
    for section in sections {
        let shared = previous
            .iter()
            .zip(&section.path)
            .take_while(|(a, b)| a == b)
            .count();
        for (depth, name) in section.path.iter().enumerate().skip(shared) {
            heading(&mut output, name, depth * 2, width.is_some());
        }
        let indent = section.path.len() * 2;
        let table = build_table(
            &section.headers,
            section.rows.iter().map(|row| row.cells.clone()),
            width.map(|w| w.saturating_sub(indent)),
            if section.path[0] == "system" {
                system_dimensions.get_widths()
            } else {
                None
            },
        );
        write_table(&mut output, table, indent, &section.rows, width.is_some());
        previous = &section.path;
    }
    output.trim_end().to_owned()
}

fn tool(path: &str) -> &str {
    if path.starts_with(".claude/") {
        "Claude Code"
    } else if path.starts_with(".codex/") {
        "Codex"
    } else if path.starts_with(".config/opencode/") {
        "OpenCode"
    } else {
        "(shared)"
    }
}

fn clean(text: &str) -> String {
    text.chars()
        .flat_map(|c| {
            if c.is_control() && c != '\n' {
                c.escape_default().collect::<Vec<_>>()
            } else {
                vec![c]
            }
        })
        .collect()
}

fn heading(output: &mut String, text: &str, indent: usize, color: bool) {
    if !output.is_empty() && !output.ends_with("\n\n") {
        output.push('\n');
    }
    output.push_str(&" ".repeat(indent));
    output.push_str(
        &console::style(clean(text))
            .magenta()
            .italic()
            .force_styling(color && console::colors_enabled())
            .to_string(),
    );
    output.push('\n');
}

fn build_table(
    headers: &[&str],
    rows: impl IntoIterator<Item = Vec<String>>,
    width: Option<usize>,
    columns: Option<&[usize]>,
) -> Table {
    let mut builder = Builder::default();
    builder.push_record(headers.iter().copied());
    for row in rows {
        builder.push_record(row.iter().map(|cell| clean(cell)));
    }
    let mut table = builder.build();
    table.with(Style::empty());
    table
        .with(Modify::new(Columns::first()).with(Padding::new(0, 1, 0, 0)))
        .with(Modify::new(Columns::last()).with(Padding::zero()));
    if width.is_some() {
        table.with(Modify::new(Rows::first()).with(Format::content(|text| {
            console::style(text).magenta().italic().to_string()
        })));
    }
    if let Some(columns) = columns {
        for (column, width) in columns.iter().enumerate() {
            let padding = table.get_config().get_padding((0, column).into());
            let content_width = width.saturating_sub(padding.left.size + padding.right.size);
            table.with(Modify::new(Columns::new(column..=column)).with(Width::wrap(content_width)));
        }
        table.with(Width::list(columns.to_vec()));
    } else if let Some(width) = width {
        table.with(Width::wrap(width.max(headers.len() * 4)).priority(PriorityMax::default()));
    }
    table
}

fn write_table(output: &mut String, table: Table, indent: usize, rows: &[Row], color: bool) {
    let mut dimensions = table.get_dimension().clone();
    dimensions.estimate(table.get_records(), table.get_config());
    let rendered = table.to_string();
    let mut lines = rendered.lines();
    for (index, height) in dimensions
        .get_heights()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let change = index.checked_sub(1).map(|i| rows[i].change).unwrap_or(' ');
        for line in lines.by_ref().take(*height) {
            if change == ' ' {
                output.push_str(&" ".repeat(indent));
                output.push_str(line.trim_end());
            } else {
                let line = format!("{change}{}{line}", " ".repeat(indent.saturating_sub(1)));
                let style = console::style(line.trim_end());
                let style = if change == '-' {
                    style.red()
                } else {
                    style.green()
                };
                output.push_str(
                    &style
                        .force_styling(color && console::colors_enabled())
                        .to_string(),
                );
            }
            output.push('\n');
        }
    }
}

fn value(value: &Value) -> String {
    value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn setting_description(name: &str, value: &Value) -> String {
    match (name, value.as_i64()) {
        ("wvous-br-corner", Some(1)) => "右下ホットコーナー：何もしない".into(),
        ("ShowDate", Some(0)) => "メニューバーの日付：空きがある場合に表示".into(),
        _ => description(name).into(),
    }
}

fn description(name: &str) -> &str {
    match name {
        "AppleInterfaceStyle" => "外観モードのスタイル",
        "NSAutomaticCapitalizationEnabled" => "英字入力時の自動大文字変換",
        "com.apple.swipescrolldirection" => "スクロール方向の自然な動作",
        "Sound" => "コントロールセンターのサウンド表示",
        "show-recents" => "Dockにおける最近使ったアプリケーションの表示",
        "wvous-br-corner" => "右下ホットコーナー動作",
        "AppleFnUsageType" => "Fnキー動作",
        "Dictation Enabled" => "音声入力機能の利用",
        "AppleShowAllExtensions" => "Finderにおけるすべてのファイル拡張子の表示",
        "AppleShowAllFiles" => "Finderにおける不可視ファイルの表示",
        "QuitMenuItem" => "Finderの終了メニュー項目の表示",
        "ShowPathbar" => "Finderのパスバー表示",
        "ShowStatusBar" => "Finderのステータスバー表示",
        "FXRemoveOldTrashItems" => "30日経過したゴミ箱内項目の自動削除",
        "NewWindowTarget" => "新規Finderウインドウで開く対象フォルダ",
        "ShowAMPM" => "メニューバー時計における午前・午後の表示",
        "ShowDate" => "メニューバー日付表示条件",
        "ShowDayOfWeek" => "メニューバー時計における曜日の表示",
        "location" => "スクリーンショット保存先",
        "enableKeyMapping" => "キーマッピング機能の有効化状態",
        "remapCapsLockToControl" => "CapsLockキーからControlキーへの割り当て変更",
        "timeZone" => "システムのタイムゾーン設定",
        _ => "—",
    }
}

fn service(service: &Service, timezone: &str) -> Vec<String> {
    let config = &service.config;
    let keep_alive = &config["KeepAlive"];
    let persistent = keep_alive == true;
    let mut starts = Vec::new();
    if config["RunAtLoad"] == true
        || persistent
        || (keep_alive.is_object() && keep_alive["AfterInitialDemand"] != true)
    {
        starts.push(
            match service.scope.as_str() {
                "system" => "at Mac startup / when enabled",
                "all users" => "at any user login / when enabled",
                _ => "at login / when enabled",
            }
            .into(),
        );
    }
    let schedule = &config["StartCalendarInterval"];
    if !schedule.is_null() {
        for schedule in schedule
            .as_array()
            .cloned()
            .unwrap_or_else(|| vec![schedule.clone()])
        {
            starts.push(calendar(&schedule, timezone));
        }
        starts.push("after sleep: once if missed".into());
    }
    if let Some(seconds) = config["StartInterval"].as_u64() {
        starts.push(format!("every {seconds}s (missed while asleep: skipped)"));
    }
    for (key, trigger) in [
        ("QueueDirectories", "nonempty directory"),
        ("Sockets", "socket activity"),
        ("WatchPaths", "path change"),
    ] {
        if let Some(value) = config.get(key).filter(|v| {
            !v.is_null() && **v != serde_json::json!({}) && **v != serde_json::json!([])
        }) {
            starts.push(format!("{trigger}: {}", self::value(value)));
        }
    }
    if config["StartOnMount"] == true {
        starts.push("volume mount".into());
    }
    if starts.is_empty() {
        starts.push("on demand".into());
    }
    let description = match service.name.as_str() {
        "nix-daemon" => "Nixのビルドやストア操作を受け付けます。",
        "nix-gc" => "設定された保存期間に従い、参照されなくなったNixデータを回収します。",
        "zundamonotify" => {
            "対応するAIエージェントの作業完了や入力待ちをずんだもんの音声で知らせます。"
        }
        _ => "—",
    };
    vec![
        service.name.clone(),
        if persistent {
            "persistent"
        } else if keep_alive.is_object() {
            "conditional"
        } else {
            "one-shot"
        }
        .into(),
        starts.join("\n"),
        if persistent {
            "yes".into()
        } else if keep_alive.is_object() {
            restart(keep_alive)
        } else {
            "no".into()
        },
        description.into(),
    ]
}

fn restart(conditions: &Value) -> String {
    conditions
        .as_object()
        .into_iter()
        .flatten()
        .filter(|(_, value)| !value.is_null())
        .map(
            |(key, condition)| match (key.as_str(), condition.as_bool()) {
                ("SuccessfulExit", Some(false)) => "on failure".into(),
                ("SuccessfulExit", Some(true)) => "on success".into(),
                ("Crashed", Some(true)) => "on crash".into(),
                ("Crashed", Some(false)) => "unless crashed".into(),
                _ => format!("{key}={}", value(condition)),
            },
        )
        .collect::<Vec<_>>()
        .join(" OR ")
}

fn calendar(schedule: &Value, timezone: &str) -> String {
    let number = |key: &str| {
        schedule[key]
            .as_u64()
            .map(|v| format!("{v:02}"))
            .unwrap_or_else(|| "*".into())
    };
    let present = |key: &str| schedule.get(key).is_some_and(|v| !v.is_null());
    let date = if present("Day") || present("Month") {
        format!("month={} day={}", number("Month"), number("Day"))
    } else if present("Weekday") {
        format!("weekday={}", number("Weekday"))
    } else {
        "daily".into()
    };
    let weekday = if present("Day") && present("Weekday") {
        format!(" OR weekday={}", number("Weekday"))
    } else {
        String::new()
    };
    format!(
        "{date}{weekday} {}:{} {timezone}",
        number("Hour"),
        number("Minute")
    )
}

struct Screen;

impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        let guard = Self;
        execute!(io::stdout(), terminal::EnterAlternateScreen, cursor::Hide)?;
        Ok(guard)
    }
}

impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(io::stdout(), cursor::Show, terminal::LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub(super) fn live(
    settings: &[crate::settings::Setting],
    inventory: &mut Inventory,
    checks: &mut Checks,
) -> Result<ExitCode> {
    let _screen = Screen::enter()?;
    let mut offset = 0usize;
    let mut dirty = true;
    let mut checking = true;
    let mut rendered = String::new();
    loop {
        let (width, height) = terminal::size()?;
        let rows = usize::from(height.saturating_sub(1)).max(1);
        if dirty {
            let width = Some(usize::from(width));
            rendered = format!(
                "{}\n{}",
                crate::settings::render(settings, width),
                report(inventory, width, true)
            );
        }
        let lines: Vec<_> = rendered.lines().collect();
        offset = offset.min(lines.len().saturating_sub(rows));
        if dirty {
            let mut output = io::stdout().lock();
            queue!(
                output,
                cursor::MoveTo(0, 0),
                terminal::Clear(terminal::ClearType::All)
            )?;
            for (row, line) in lines.iter().skip(offset).take(rows).enumerate() {
                queue!(output, cursor::MoveTo(0, row as u16))?;
                write!(output, "{line}")?;
            }
            queue!(output, cursor::MoveTo(0, height.saturating_sub(1)))?;
            write!(
                output,
                "{}",
                console::truncate_str(
                    if checking {
                        "checking latest · j/k line · f/b page · d/u half · g/G ends · q quit"
                    } else {
                        "j/k line · f/b page · d/u half · g/G ends · q quit"
                    },
                    usize::from(width),
                    ""
                )
            )?;
            output.flush()?;
            dirty = false;
        }
        while checking {
            match checks.poll() {
                Ok((indices, result)) => {
                    for index in indices {
                        inventory.packages[index].latest = result.clone();
                    }
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    checking = false;
                    dirty = true;
                }
            }
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            return Ok(ExitCode::SUCCESS);
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            return Ok(ExitCode::from(130));
                        }
                        KeyCode::Down | KeyCode::Char('j') => offset = offset.saturating_add(1),
                        KeyCode::Up | KeyCode::Char('k') => offset = offset.saturating_sub(1),
                        KeyCode::PageDown | KeyCode::Char(' ' | 'f') => {
                            offset = offset.saturating_add(rows)
                        }
                        KeyCode::PageUp | KeyCode::Char('b') => {
                            offset = offset.saturating_sub(rows)
                        }
                        KeyCode::Char('d') => offset = offset.saturating_add(rows.div_ceil(2)),
                        KeyCode::Char('u') => offset = offset.saturating_sub(rows.div_ceil(2)),
                        KeyCode::Home | KeyCode::Char('g') => offset = 0,
                        KeyCode::End | KeyCode::Char('G') => offset = lines.len(),
                        _ => (),
                    }
                    dirty = true;
                }
                Event::Resize(_, _) => dirty = true,
                _ => (),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_omits_unchanged_rows_and_keeps_removed_groups_and_wrapped_columns_aligned() {
        let before = serde_json::json!({
            "source": "/fixture",
            "packages": [
                {"name": "stable", "manager": "Nix", "declared": "1", "lookup": null},
                {"name": "node", "manager": "mise", "declared": "1", "lookup": null}
            ],
            "system": [
                {"key": "system.defaults.dock.show-recents", "group": "system.defaults.dock", "name": "show-recents", "value": true},
                {"key": "system.defaults.finder.AppleShowAllExtensions", "group": "system.defaults.finder", "name": "AppleShowAllExtensions", "value": true}
            ],
            "services": [], "tools": [], "timeZone": "UTC",
            "localllm": {"enabled": false, "default_model": null}
        });
        let mut after = before.clone();
        after["packages"][1]["declared"] = "2".into();
        after["packages"].as_array_mut().unwrap().reverse();
        after["system"][0]["value"] = false.into();
        after["system"].as_array_mut().unwrap().pop();
        let before: Inventory = serde_json::from_value(before).unwrap();
        let after: Inventory = serde_json::from_value(after).unwrap();
        for width in [None, Some(60), Some(80), Some(120)] {
            let result = diff(Some(&before), &after, width);
            let result = console::strip_ansi_codes(&result);
            assert!(!result.contains("stable"));
            assert!(!result.contains("latest"));
            assert!(!result.contains("agents"));
            assert!(!result.contains("services"));
            assert!(result.contains("system.defaults.finder"));
            let headers: Vec<_> = result
                .lines()
                .filter(|line| line.trim_start().starts_with("setting "))
                .collect();
            assert_eq!(headers.len(), 2);
            assert_eq!(headers[0], headers[1]);
            let descriptions = result
                .split("system.defaults.dock\n")
                .nth(1)
                .unwrap()
                .split("system.defaults.finder\n")
                .next()
                .unwrap();
            assert!(
                descriptions
                    .lines()
                    .filter(|line| line.starts_with('-'))
                    .count()
                    >= 1
            );
            assert!(
                descriptions
                    .lines()
                    .filter(|line| line.starts_with('+'))
                    .count()
                    >= 1
            );
            if let Some(width) = width {
                assert!(result
                    .lines()
                    .all(|line| console::measure_text_width(line) <= width));
            }
        }
    }

    #[test]
    fn scheduling_distinguishes_interval_calendar_and_login() {
        let scheduled = Service {
            name: "example".into(),
            scope: "system".into(),
            config: serde_json::json!({"RunAtLoad": false, "KeepAlive": false, "StartCalendarInterval": {"Hour": 0, "Minute": 0}}),
        };
        let row = service(&scheduled, "Asia/Tokyo");
        assert_eq!(row[1], "one-shot");
        assert_eq!(
            row[2],
            "daily 00:00 Asia/Tokyo\nafter sleep: once if missed"
        );
        let login = Service {
            name: "example".into(),
            scope: "user".into(),
            config: serde_json::json!({"KeepAlive": true, "RunAtLoad": true}),
        };
        assert_eq!(service(&login, "UTC")[2], "at login / when enabled");
        assert_eq!(clean("unsafe\u{1b}[2J"), "unsafe\\u{1b}[2J");
    }
}
