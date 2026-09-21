use std::collections::BTreeMap;

use serde_json::Value;
use tabled::builder::Builder;
use tabled::grid::dimension::Estimate;
use tabled::settings::object::{Columns, Rows};
use tabled::settings::peaker::PriorityMax;
use tabled::settings::{Format, Modify, Padding, Style, Width};
use tabled::Table;

use super::diff::ResourceDiff;
use super::{Inventory, Service};

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

fn sections(inventory: &Inventory) -> Vec<Section> {
    let mut sections = vec![Section {
        path: vec!["packages".into()],
        headers: vec!["name", "manager", "current"],
        rows: inventory
            .packages
            .iter()
            .map(|package| {
                Row::new(
                    format!("{}\0{}", package.name, package.manager),
                    vec![
                        package.name.clone(),
                        package.manager.clone(),
                        package.declared.clone(),
                    ],
                )
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
                    setting_description(&setting.key, &setting.name, &setting.value),
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

pub(super) fn report(inventory: &Inventory, width: Option<usize>) -> String {
    render_sections(&sections(inventory), width)
}

pub(super) fn deployment(
    generation: Option<&(String, String)>,
    copies: &[crate::home_copy::CopyChange],
    width: Option<usize>,
) -> String {
    let mut rows = Vec::new();
    let mut add = |name: String, before: Option<String>, after: String| {
        if let Some(before) = before {
            let mut row = Row::new(name.clone(), vec![name.clone(), before]);
            row.change = '-';
            rows.push(row);
        }
        let mut row = Row::new(name.clone(), vec![name, after]);
        row.change = '+';
        rows.push(row);
    };
    if let Some((before, after)) = generation {
        add("system".into(), Some(before.clone()), after.clone());
    }
    for copy in copies {
        add(
            format!("~/{}", copy.path),
            copy.before
                .as_ref()
                .map(|hash| format!("sha256:{}", &hash[..12])),
            format!("sha256:{}", &copy.after[..12]),
        );
    }
    if rows.is_empty() {
        return String::new();
    }
    render_sections(
        &[Section {
            path: vec!["deployment".into()],
            headers: vec!["resource", "revision"],
            rows,
        }],
        width,
    )
}

pub(super) fn diff(changes: &ResourceDiff, width: Option<usize>) -> String {
    let mut groups = BTreeMap::<Vec<String>, Section>::new();
    let sides = changes
        .removed
        .iter()
        .map(|inventory| (inventory, '-'))
        .chain(std::iter::once((&changes.added, '+')));
    for (inventory, sign) in sides {
        for mut section in sections(inventory) {
            if section.path == ["agents", "localllm"] && !changes.localllm_changed {
                continue;
            }
            if section.rows.is_empty() {
                continue;
            }
            for row in &mut section.rows {
                row.change = sign;
            }
            if let Some(existing) = groups.get_mut(&section.path) {
                existing.rows.extend(section.rows);
            } else {
                groups.insert(section.path.clone(), section);
            }
        }
    }
    let mut changed: Vec<_> = groups.into_values().collect();
    for section in &mut changed {
        section
            .rows
            .sort_by_key(|row| (row.key.to_lowercase(), row.key.clone(), row.change != '-'));
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
    render_sections(&changed, width)
}

fn render_sections(sections: &[Section], width: Option<usize>) -> String {
    let mut system_table = build_table(
        &["setting", "value", "description"],
        sections
            .iter()
            .filter(|s| s.path[0] == "system")
            .flat_map(|s| s.rows.iter().map(|r| r.cells.clone())),
    );
    if let Some(width) = width {
        system_table
            .with(Width::wrap(width.saturating_sub(4).max(12)).priority(PriorityMax::default()));
    }
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
        let mut table = build_table(
            &section.headers,
            section.rows.iter().map(|row| row.cells.clone()),
        );
        if section.path[0] == "packages" {
            table.with(Modify::new(Columns::first()).with(Width::increase(32)));
        }
        if width.is_some() {
            table.with(Modify::new(Rows::first()).with(Format::content(|text| {
                console::style(text).magenta().italic().to_string()
            })));
        }
        if let Some(columns) = system_dimensions
            .get_widths()
            .filter(|_| section.path[0] == "system")
        {
            for (column, width) in columns.iter().enumerate() {
                let padding = table.get_config().get_padding((0, column).into());
                let content_width = width.saturating_sub(padding.left.size + padding.right.size);
                table.with(
                    Modify::new(Columns::new(column..=column)).with(Width::wrap(content_width)),
                );
            }
            table.with(Width::list(columns.to_vec()));
        } else if let Some(width) = width {
            table.with(
                Width::wrap(width.saturating_sub(indent).max(section.headers.len() * 4))
                    .priority(PriorityMax::default()),
            );
        }
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

fn build_table(headers: &[&str], rows: impl IntoIterator<Item = Vec<String>>) -> Table {
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

fn setting_description(key: &str, name: &str, value: &Value) -> String {
    let custom_description = match key {
        "dictationShortcut.enabled" => Some("音声入力ショートカットの有効化状態"),
        "dictationShortcut.parameters" => Some("音声入力開始キーの修飾キーコード"),
        "dictationShortcut.type" => Some("音声入力ショートカットの入力方式"),
        "homebrew.global.autoUpdate" => Some("通常のHomebrew実行時に自動更新を行うか"),
        "homebrew.onActivation.autoUpdate" => Some("構成反映時にHomebrewの自動更新を許可するか"),
        "homebrew.onActivation.cleanup" => Some("宣言外のHomebrewパッケージに対する削除方針"),
        "homebrew.onActivation.upgrade" => Some("構成反映時にHomebrewパッケージを更新するか"),
        "nightShift.schedule.end" => Some("Night Shiftの翌朝の終了時刻"),
        "nightShift.schedule.start" => Some("Night Shiftの毎日の開始時刻"),
        "nightShift.temperature" => Some("Night Shiftにおける暖色の強さ"),
        "nix-homebrew.mutableTaps" => Some("宣言外のリポジトリ（tap）の手動追加と更新を許可するか"),
        "nix.gc.automatic" => Some("Nixのガベージコレクションを自動実行するか"),
        "nix.gc.options" => Some("Nixのガベージコレクションに渡す削除条件の引数"),
        _ => None,
    };
    if let Some(description) = custom_description {
        return description.into();
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_omits_unchanged_rows_and_keeps_removed_groups_and_wrapped_columns_aligned() {
        let before = serde_json::json!({
            "source": "/fixture",
            "packages": [
                {"name": "stable", "manager": "Nix", "declared": "1"},
                {"name": "node", "manager": "mise", "declared": "1"}
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
            let result = diff(&ResourceDiff::between(Some(&before), &after), width);
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
