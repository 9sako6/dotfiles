> [!NOTE]
> このファイルは `obra/superpowers` の `skills/using-superpowers/references/copilot-tools.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Copilot CLI Tool Mapping

skill は Claude Code の tool 名を使う。skill 内でそれらを見かけたら、platform 上の等価なものに読み替える:

| Skill references | Copilot CLI equivalent |
|-----------------|----------------------|
| `Read`（file 読み込み） | `view` |
| `Write`（file 作成） | `create` |
| `Edit`（file 編集） | `edit` |
| `Bash`（command 実行） | `bash` |
| `Grep`（file 内容検索） | `grep` |
| `Glob`（file 名検索） | `glob` |
| `Skill` tool（skill を起動） | `skill` |
| `WebFetch` | `web_fetch` |
| `Task` tool（subagent を dispatch） | `task`（[Agent types](#agent-types) を参照） |
| 複数の `Task` 呼び出し（parallel） | 複数の `task` 呼び出し |
| Task の status / output | `read_agent`、`list_agents` |
| `TodoWrite`（task tracking） | built-in `todos` table に対する `sql` |
| `WebSearch` | 等価物なし — search engine URL と `web_fetch` を使う |
| `EnterPlanMode` / `ExitPlanMode` | 等価物なし — main session に留まる |

## Agent types

Copilot CLI の `task` tool は `agent_type` parameter を受け取る:

| Claude Code agent | Copilot CLI equivalent |
|-------------------|----------------------|
| `general-purpose` | `"general-purpose"` |
| `Explore` | `"explore"` |
| named plugin agent（例: `superpowers:code-reviewer`） | installed plugin から自動 discovery される |

## Async shell session

Copilot CLI は永続的な async shell session をサポートしており、これに Claude Code での直接の等価物はない:

| Tool | Purpose |
|------|---------|
| `bash` with `async: true` | 長時間動く command をバックグラウンドで始める |
| `write_bash` | 実行中の async session に入力を送る |
| `read_bash` | 実行中 session の出力を読む |
| `stop_bash` | async session を終了する |
| `list_bash` | すべての active shell session を一覧する |

## 追加の Copilot CLI tool

| Tool | Purpose |
|------|---------|
| `store_memory` | codebase に関する事実を将来 session 用に永続化する |
| `report_intent` | 現在の intent で UI status line を更新する |
| `sql` | session の SQLite database（todos、metadata）を query する |
| `fetch_copilot_cli_documentation` | Copilot CLI の documentation を調べる |
| GitHub MCP tool（`github-mcp-server-*`） | issue、PR、code search などへの native GitHub API access |
