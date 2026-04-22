---
name: logging-agent
description: Use when skills の発動、推定された軌道修正、再計画、実装・調査・テストなどの agent 行動イベントを会話中に逐次ログしたいとき、または観測・可視化・監査のためにログ書き込み完了を強いゲートにしたいときに使う。
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
---

# Logging Agent

会話中の event を、あとから分析できる形で残すための rigid skill。

これは「丁寧に記録するための補助」ではない。**記録が終わるまで次に進ませない gate** である。

<EXTREMELY-IMPORTANT>
この skill が有効な間は、対象イベントが起きたら必ずログを書く。

ログを書き終えて成功を確認するまで、次に進んではならない。

「あとでまとめて書く」「今回は軽微だから省略する」「急いでいるので後回しにする」といった合理化は禁止。
</EXTREMELY-IMPORTANT>

<HARD-GATE>
イベントファイルの書き込み成功を確認するまでは:

- 次の思考に進んではならない
- 追加の commentary を出してはならない
- ファイル編集をしてはならない
- コマンドを実行してはならない

書き込みに失敗したら再試行する。再試行しても失敗するなら、その場で停止し、失敗をユーザーに報告する。
</HARD-GATE>

## いつ使うか

- ユーザーが観測、可視化、監査、ログ、トレース、記録、メモ化を求めたとき
- skill の発動や組み合わせを記録したいとき
- ユーザーの明示的訂正だけでなく、agent が「これは軌道修正だ」と推定した瞬間も残したいとき
- 再計画、実装、調査、テストなどの行動イベントを後から分析可能にしたいとき
- 観測のために、ログ書き込み完了を progress gate にしたいとき

## 指示の優先順位

1. **ユーザーの明示的指示**（AGENTS.md、直接の依頼）
2. **この skill**
3. **デフォルトの system behavior**

ユーザーがログ対象、保存先、表示方法を指定したら、その指示を優先する。ただし、明示的に解除されない限り、`書き込み成功まで進まない` gate は維持する。

## 可視化ルール

この skill が有効な間の progress update / commentary は、毎回の冒頭を必ず次の固定プレフィックスで始める:

```text
[logging-agent]
```

final answer にはこのプレフィックスを付けなくてよい。

この表示は、今この会話で logging gate が有効であることをユーザーに見える形で知らせるためのもの。省略してはならない。

## 鉄則

```text
NO NEXT STEP WITHOUT A WRITTEN EVENT FILE
```

この文言の精神は、`記録の完了より先に実務を進めない` ことである。言い換えで逃げてはならない。

## ワークフロー

```
- [ ] Step 1: 対象イベントか判定する
- [ ] Step 2: 毎回の commentary に `[logging-agent]` を付ける
- [ ] Step 3: event file を作成する
- [ ] Step 4: 書き込み成功を確認する
- [ ] Step 5: その後にのみ作業を続ける
```

対象イベントかどうか迷う場合は、**記録する側に倒す**。誤記録より未記録のほうが分析上の損失が大きい。

## 保存先

- Codex 系: `~/.codex/mylogs/logging-agent/`
- Claude Code 系: `~/.claude/mylogs/logging-agent/`

`agent` 値に応じて保存先を選ぶ:

| `agent` prefix | 保存先 |
|---|---|
| `codex:` | `~/.codex/mylogs/logging-agent/` |
| `claude-code:` | `~/.claude/mylogs/logging-agent/` |

## ファイル名

1 イベント 1 ファイルで保存する。ファイル名は次の形式:

```text
YYYYMMDDTHHMMSSmmmZ-<rand>.md
```

要件:

- UTC timestamp を使う
- `rand` は衝突回避用の短いランダム値
- event 名はファイル名に入れず、frontmatter の `event` で判別する

## フォーマット

各イベントファイルは Markdown とし、**厳格な YAML frontmatter + 固定見出し**で構成する。

基本テンプレート:

```md
---
schema: logging-agent/v1
event: <event name>
timestamp: 2026-04-22T12:34:56.789Z
agent: codex:main
skills_active:
  - logging-agent
---
# Summary
<1-3 sentences>

# Evidence
<why this event was logged>

# Next Action
<the immediate next step after logging succeeds>
```

## `agent` の命名

サブエージェントを識別できる値を使う。

例:

- `codex:main`
- `codex:subagent-1`
- `claude-code:main`
- `claude-code:subagent-2`

## イベント種別

最初に扱う event は次の 4 種:

- `skill_invoked`
- `user_correction_inferred`
- `agent_replan`
- `artifact_change`

### `skill_invoked`

skill が新しく発動した瞬間に記録する。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `activated_skill`
- `trigger`
- `skills_active`

例:

```md
---
schema: logging-agent/v1
event: skill_invoked
timestamp: 2026-04-22T12:34:56.789Z
agent: codex:main
activated_skill: superpowers-brainstorming
trigger: user_request
skills_active:
  - logging-agent
  - superpowers-brainstorming
---
# Summary
superpowers-brainstorming が発動したため記録。

# Evidence
設計検討と要件整理の依頼として解釈した。

# Next Action
brainstorming workflow に従って確認を続ける。
```

### `user_correction_inferred`

ユーザーの明示的な訂正、または agent が軌道修正だと推定した瞬間に記録する。`confidence` は **low / medium / high を全部記録対象** とする。

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

agent が方針・分解・手順を組み直した瞬間に記録する。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `replan_reason`
- `skills_active`

### `artifact_change`

実装、調査、テスト、設計などの行動イベントを記録する。

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

## `skills_active` の扱い

`skills_active` には、その時点で有効な skill を **全部** 入れる。

- `logging-agent` 自身を必ず含める
- 新しく発動した skill があるなら含める
- 配列はその時点の active set を表す

## 書き込み手順

イベント発生時は必ず次の順で進める:

1. event 種別を決める
2. frontmatter と本文を埋める
3. 保存先ディレクトリを作成・確認する
4. イベントファイルを書き込む
5. 書き込み成功を確認する
6. そのあとで次の思考・編集・実行に進む

途中で「まず作業を進めて後で書く」は禁止。

## 失敗時の扱い

- ディレクトリがなければ作る
- 権限やパスの問題で失敗したら再試行する
- 再試行しても失敗したら、それ以上進まずユーザーに報告する

## Red Flags

こんな考えが浮かんだら STOP。合理化している:

| Thought | Reality |
|---------|---------|
| "これは小さい correction だから記録しなくてよい" | 小さいかどうかは後段分析で決める。今は記録する |
| "commentary を先に出してから書けばよい" | 先に出した時点で gate を破っている |
| "confidence が low だから不要" | `low` も観測対象である |
| "skill はもう active だから invocation は書かなくてよい" | 発動の瞬間は別イベントとして残す |
| "サブエージェントは main とまとめてよい" | 後で分析不能になる。分けて記録する |
| "今は急いでいる" | 急いでいるときほど記録漏れが増える。止まって書く |
| "後でまとめて補完できる" | その運用はこの skill で明示的に禁止されている |

## 分析上の意図

この skill は task start / end を定義しない。会話全体を event stream として残し、後段の分析でまとまりを推定する。

この設計で、あとから次を観測できる:

- どの skill のあとに軌道修正が増減したか
- どの skill 組み合わせで correction が減るか
- replan のあとに correction が止まりやすいか
- どの phase でズレやすいか

## アンチパターン

- correction が明示されていないから記録しない
- confidence が low だから記録しない
- commentary だけ先に出して、ログを後回しにする
- event file は不要と自己判断して進む
- `skills_active` から `logging-agent` 自身を落とす
- サブエージェントを main と同一視して `agent` を潰す

## 要点

この skill の責務は「イベントをあとで分析できる形で残し、残すまで前に進ませない」ことに尽きる。
