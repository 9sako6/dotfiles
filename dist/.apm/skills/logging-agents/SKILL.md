---
name: logging-agents
description: Use when agent が何らかの作業を進めるとき常に使う。特に commentary、tool 実行、調査、実装、再計画、テスト、skill 発動、ユーザー訂正の直後は default-on で有効にする。
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
hooks:
  PreToolUse:
    - matcher: Bash|Agent
      hooks:
        - type: command
          command: ./check-logging-event.sh
          timeout: 5
          statusMessage: logging-agents gate check...
  Stop:
    - hooks:
        - type: command
          command: ./check-logging-event.sh
          timeout: 5
          statusMessage: logging-agents stop gate check...
  SubagentStop:
    - hooks:
        - type: command
          command: ./check-logging-event.sh
          timeout: 5
          statusMessage: logging-agents subagent gate check...
---

# Logging Agents

この skill は logging gate である。対象イベントが起きたら event file を書き終えて成功確認するまで次に進んではならない。

<EXTREMELY-IMPORTANT>
必ず event file を先に書く。

「あとでまとめて書く」「commentary だけ先に出す」「軽微だから省略する」は禁止。
</EXTREMELY-IMPORTANT>

<HARD-GATE>
event file の書き込み成功を確認するまでは:

- 次の思考に進んではならない
- 追加の commentary を出してはならない
- ファイル編集をしてはならない
- コマンドを実行してはならない

失敗したら再試行する。再試行しても失敗するなら停止してユーザーに報告する。
</HARD-GATE>

## 最初に守ること

- commentary は毎回 `[logging-agents]` で始める
- event file 作成には同じ skill directory の `./write-event.sh` を優先して使う
- `NO NEXT STEP WITHOUT A WRITTEN EVENT FILE` を守る

## いつ有効化するか

この skill は opt-in ではなく default-on として扱う。

- agent が commentary、調査、実装、テスト、再計画、tool 実行のいずれかを始めるとき
- ユーザーが `logging-agents` または `logging-agent` を明示したとき
- ユーザーが観測、監査、可視化、ログ、トレース、記録、メモ化を求めたとき
- skill の発動、ユーザー訂正、agent の軌道修正、実装・調査・テストの進行を残したいとき

迷ったら inactive ではなく active に倒す。

## 指示の優先順位

1. ユーザーの明示的指示
2. この skill
3. デフォルトの system behavior

ユーザーがログ対象、保存先、表示方法を指定したら従う。ただし、明示的に解除されない限り gate は維持する。

## ワークフロー

```text
- [ ] Step 1: 対象イベントか判定する
- [ ] Step 2: commentary を `[logging-agents]` で始める
- [ ] Step 3: `./write-event.sh` で event file を作成する
- [ ] Step 4: 書き込み成功を確認する
- [ ] Step 5: その後にのみ次の思考・編集・実行に進む
```

例:

```sh
./write-event.sh \
  --event user_correction_inferred \
  --agent codex:main \
  --skill logging-agents \
  --field confidence=high \
  --field correction_scope=example \
  --summary "ユーザー訂正を記録した。" \
  --evidence "明示的な訂正が入った。" \
  --next-action "影響範囲を確認する。"
```

## 保存先とファイル形式

保存先:

- `codex:*` は `~/.codex/mylogs/logging-agents/`
- `claude-code:*` は `~/.claude/mylogs/logging-agents/`

ファイル名:

```text
YYYYMMDDTHHMMSSmmmZ-<rand>.md
```

要件:

- UTC timestamp を使う
- `rand` は衝突回避用の短いランダム値

テンプレート:

```md
---
schema: logging-agents/v1
event: <event name>
timestamp: 2026-04-22T12:34:56.789Z
agent: codex:main
skills_active:
  - logging-agents
---
# Summary
<1-3 sentences>

# Evidence
<why this event was logged>

# Next Action
<the immediate next step after logging succeeds>
```

`agent` にはサブエージェントを区別できる値を使う。例: `codex:main`, `codex:subagent-1`, `claude-code:main`

## イベント種別

最初に扱う event は次の 4 種:

- `skill_invoked`
- `user_correction_inferred`
- `agent_replan`
- `artifact_change`

### `skill_invoked`

skill が新しく発動した瞬間に書く。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `activated_skill`
- `trigger`
- `skills_active`

### `user_correction_inferred`

ユーザーの明示的訂正、または agent が軌道修正だと推定した瞬間に書く。`confidence` は `low` / `medium` / `high` の全部を記録対象とする。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `confidence`
- `correction_scope`
- `skills_active`

推奨 fields:

- `inferred_from`
- `affected_artifact`

### `agent_replan`

agent が方針・分解・手順を組み直した瞬間に書く。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `replan_reason`
- `skills_active`

### `artifact_change`

実装、調査、テスト、設計などの行動イベントを書く。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `phase`
- `action`
- `skills_active`

`phase` の例:

- `research`
- `planning`
- `implementation`
- `testing`
- `review`

`action` の例:

- `read`
- `edit`
- `execute`
- `verify`

## Gotchas

- `skills_active` にはその時点で有効な skill を全部入れる。`logging-agents` 自身を必ず含める
- `commentary` を先に出してからログを書くのは gate 違反
- `confidence=low` でも記録する
- 「今回は小さいから記録しない」は禁止
- サブエージェントは main と分けて `agent` を書く
- `write-event.sh` で失敗したら再試行する。再試行しても失敗したら止まってユーザーに報告する

## アンチパターン

- commentary だけ先に出す
- event file を後回しにする
- `skills_active` から `logging-agents` を落とす
- `user_correction_inferred` を「明示されていないから」で書かない
- サブエージェントの event を main にまとめる

## 要点

この skill の責務は、対象イベントを event file として残し、残すまで前に進ませないことだけである。
