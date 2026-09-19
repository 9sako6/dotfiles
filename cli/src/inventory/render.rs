use std::collections::BTreeMap;
use std::io::{self, Write};
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

pub(super) fn report(inventory: &Inventory, width: Option<usize>) -> String {
    let mut output = String::new();
    heading(&mut output, "packages", 0, width.is_some());
    table(
        &mut output,
        &["name", "manager", "declared", "latest"],
        inventory.packages.iter().map(|p| {
            vec![
                p.name.clone(),
                p.manager.replace(" (", "\n("),
                p.declared.clone(),
                p.latest.clone(),
            ]
        }),
        2,
        width,
    );
    heading(&mut output, "system", 0, width.is_some());
    let mut groups = BTreeMap::<_, Vec<_>>::new();
    for setting in &inventory.system {
        groups.entry(&setting.group).or_default().push(vec![
            setting.name.clone(),
            value(&setting.value),
            setting_description(&setting.name, &setting.value),
        ]);
    }
    let headers = &["setting", "value", "description"];
    let system_width = width.map(|w| w.saturating_sub(4));
    let combined = build_table(
        headers,
        groups.values().flatten().cloned(),
        system_width,
        None,
    );
    let mut dimensions = combined.get_dimension().clone();
    dimensions.estimate(combined.get_records(), combined.get_config());
    for (group, settings) in groups {
        heading(&mut output, group, 2, width.is_some());
        write_table(
            &mut output,
            build_table(headers, settings, system_width, dimensions.get_widths()),
            4,
        );
    }
    heading(&mut output, "services", 0, width.is_some());
    table(
        &mut output,
        &["name", "mode", "start", "restart", "description"],
        inventory
            .services
            .iter()
            .map(|s| service(s, &inventory.time_zone)),
        2,
        width,
    );
    heading(&mut output, "agents", 0, width.is_some());
    heading(&mut output, "skills", 2, width.is_some());
    table(
        &mut output,
        &["name", "origin", "description"],
        inventory
            .skills
            .iter()
            .map(|s| vec![s.name.clone(), s.origin.clone(), s.description.clone()]),
        4,
        width,
    );
    heading(&mut output, "tools", 2, width.is_some());
    table(
        &mut output,
        &["tool", "deploy", "file"],
        inventory.tools.iter().map(|t| {
            vec![
                tool(&t.path).into(),
                t.deploy.clone(),
                format!("~/{}", t.path),
            ]
        }),
        4,
        width,
    );
    heading(&mut output, "localllm", 2, width.is_some());
    table(
        &mut output,
        &["enabled", "default model", "description"],
        [vec![
            inventory.localllm.enabled.to_string(),
            inventory
                .localllm
                .default_model
                .clone()
                .unwrap_or_else(|| "—".into()),
            "コマンド実行時のみ推論サーバーを起動し、終了時にプロセスを停止する。".into(),
        ]],
        4,
        width,
    );
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

fn table(
    output: &mut String,
    headers: &[&str],
    rows: impl IntoIterator<Item = Vec<String>>,
    indent: usize,
    width: Option<usize>,
) {
    write_table(
        output,
        build_table(headers, rows, width.map(|w| w.saturating_sub(indent)), None),
        indent,
    );
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

fn write_table(output: &mut String, table: Table, indent: usize) {
    for line in table.to_string().lines() {
        output.push_str(&" ".repeat(indent));
        output.push_str(line.trim_end());
        output.push('\n');
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

pub(super) fn live(inventory: &mut Inventory, checks: &mut Checks) -> Result<bool> {
    let _screen = Screen::enter()?;
    let mut offset = 0usize;
    let mut dirty = true;
    let mut rendered = String::new();
    loop {
        let (width, height) = terminal::size()?;
        let rows = usize::from(height.saturating_sub(1)).max(1);
        if dirty {
            rendered = report(inventory, Some(usize::from(width)));
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
                    "checking latest · ↑↓ scroll · q cancel",
                    usize::from(width),
                    ""
                )
            )?;
            output.flush()?;
            dirty = false;
        }
        loop {
            match checks.poll() {
                Ok((indices, result)) => {
                    for index in indices {
                        inventory.packages[index].latest = result.clone();
                    }
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return Ok(false),
            }
        }
        if event::poll(Duration::from_millis(50))? {
            match event::read()? {
                Event::Key(key) => {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            checks.cancel();
                            mark_cancelled(inventory);
                            return Ok(true);
                        }
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            checks.cancel();
                            mark_cancelled(inventory);
                            return Ok(true);
                        }
                        KeyCode::Down | KeyCode::Char('j') => offset = offset.saturating_add(1),
                        KeyCode::Up | KeyCode::Char('k') => offset = offset.saturating_sub(1),
                        KeyCode::PageDown | KeyCode::Char(' ') => {
                            offset = offset.saturating_add(rows)
                        }
                        KeyCode::PageUp => offset = offset.saturating_sub(rows),
                        KeyCode::Home => offset = 0,
                        KeyCode::End => offset = lines.len(),
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

fn mark_cancelled(inventory: &mut Inventory) {
    for package in &mut inventory.packages {
        if package.latest == "checking" {
            package.latest = "error: cancelled".into();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
