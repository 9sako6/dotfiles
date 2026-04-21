> [!NOTE]
> このファイルは `obra/superpowers` の `skills/using-superpowers/references/codex-tools.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Codex Tool Mapping

skill は Claude Code の tool 名を使う。skill 内でそれらを見かけたら、platform 上の等価なものに読み替える:

| Skill references | Codex equivalent |
|-----------------|------------------|
| `Task` tool（subagent を dispatch） | `spawn_agent`（[Named agent dispatch](#named-agent-dispatch) を参照） |
| 複数の `Task` 呼び出し（parallel） | 複数の `spawn_agent` 呼び出し |
| Task が結果を返す | `wait` |
| Task が自動で完了する | slot 解放のため `close_agent` |
| `TodoWrite`（task tracking） | `update_plan` |
| `Skill` tool（skill を起動） | skill は native に読み込まれる — 指示に従うだけでよい |
| `Read`、`Write`、`Edit`（file） | native file tool を使う |
| `Bash`（command 実行） | native shell tool を使う |

## Subagent dispatch には multi-agent support が必要

Codex config（`~/.codex/config.toml`）に次を追加する:

```toml
[features]
multi_agent = true
```

これにより `dispatching-parallel-agents` や `subagent-driven-development` のような skill で `spawn_agent`、`wait`、`close_agent` が使えるようになる。

## Named agent dispatch

Claude Code の skill は `superpowers:code-reviewer` のような named agent type を参照する。  
Codex には named agent registry がなく、`spawn_agent` は built-in role（`default`、`explorer`、`worker`）から generic agent を作る。

skill が named agent type を dispatch せよと言っている場合:

1. agent の prompt file を見つける（例: `agents/code-reviewer.md` や、その skill の local prompt template である `code-quality-reviewer-prompt.md`）
2. prompt 内容を読む
3. template placeholder（`{BASE_SHA}`、`{WHAT_WAS_IMPLEMENTED}` など）を埋める
4. 埋めた内容を `message` として `worker` agent を spawn する

| Skill instruction | Codex equivalent |
|-------------------|------------------|
| `Task tool (superpowers:code-reviewer)` | `code-reviewer.md` の内容を使って `spawn_agent(agent_type="worker", message=...)` |
| inline prompt 付きの `Task tool (general-purpose)` | 同じ prompt を使って `spawn_agent(message=...)` |

### Message framing

`message` parameter は system prompt ではなく user-level input である。指示追従性を最大化するように構造化する:

```
Your task is to perform the following. Follow the instructions below exactly.

<agent-instructions>
[filled prompt content from the agent's .md file]
</agent-instructions>

Execute this now. Output ONLY the structured response following the format
specified in the instructions above.
```

- persona framing（"You are..."）ではなく task-delegation framing（"Your task is..."）を使う
- XML tag で instructions を包む — model は tagged block を強い指示として扱う
- 最後に明示的な execution directive を置き、指示の要約で終わらないようにする

### この workaround を外せるタイミング

この方法は、Codex の plugin system がまだ `plugin.json` の `agents` field をサポートしていないことへの補償である。`RawPluginManifest` に `agents` field が追加されたら、plugin は `agents/` への symlink を張れるようになり（既存の `skills/` symlink と同様）、skill から named agent type を直接 dispatch できるようになる。

## Environment Detection

worktree を作ったり branch を完了させたりする skill は、処理前に read-only な git command で environment を判定すべきである:

```bash
GIT_DIR=$(cd "$(git rev-parse --git-dir)" 2>/dev/null && pwd -P)
GIT_COMMON=$(cd "$(git rev-parse --git-common-dir)" 2>/dev/null && pwd -P)
BRANCH=$(git branch --show-current)
```

- `GIT_DIR != GIT_COMMON` → すでに linked worktree 内にいる（作成を飛ばす）
- `BRANCH` が空 → detached HEAD（sandbox から branch/push/PR はできない）

各 skill がこの signal をどう使うかは `using-git-worktrees` Step 0 と `finishing-a-development-branch` Step 1 を参照する。

## Codex App Finishing

sandbox により branch/push operation がブロックされる場合（外部管理された worktree 上の detached HEAD）、agent はすべての作業を commit し、App の native control を使うようユーザーに案内する:

- **"Create branch"** — branch 名を付け、その後の commit/push/PR は App UI で行う
- **"Hand off to local"** — 作業をユーザーの local checkout に引き渡す

agent は test 実行、file staging、branch 名・commit message・PR description の提案までは行えるので、ユーザーはそれを copy して使える。
