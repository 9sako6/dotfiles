> [!NOTE]
> このファイルは `obra/superpowers` の `skills/using-superpowers/references/gemini-tools.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Gemini CLI Tool Mapping

skill は Claude Code の tool 名を使う。skill 内でそれらを見かけたら、platform 上の等価なものに読み替える:

| Skill references | Gemini CLI equivalent |
|-----------------|----------------------|
| `Read`（file 読み込み） | `read_file` |
| `Write`（file 作成） | `write_file` |
| `Edit`（file 編集） | `replace` |
| `Bash`（command 実行） | `run_shell_command` |
| `Grep`（file 内容検索） | `grep_search` |
| `Glob`（file 名検索） | `glob` |
| `TodoWrite`（task tracking） | `write_todos` |
| `Skill` tool（skill を起動） | `activate_skill` |
| `WebSearch` | `google_web_search` |
| `WebFetch` | `web_fetch` |
| `Task` tool（subagent を dispatch） | 等価物なし — Gemini CLI は subagent をサポートしない |

## Subagent support なし

Gemini CLI には Claude Code の `Task` tool に相当するものがない。subagent dispatch に依存する skill（`subagent-driven-development`、`dispatching-parallel-agents`）は、`executing-plans` による single-session 実行へ fallback する。

## 追加の Gemini CLI tool

Claude Code に等価物のない Gemini CLI tool は次のとおり:

| Tool | Purpose |
|------|---------|
| `list_directory` | file と subdirectory を一覧する |
| `save_memory` | session をまたいで事実を GEMINI.md に保存する |
| `ask_user` | 構造化された user input を求める |
| `tracker_create_task` | 高機能な task management（作成、更新、一覧、可視化） |
| `enter_plan_mode` / `exit_plan_mode` | 変更前に read-only research mode に切り替える |
