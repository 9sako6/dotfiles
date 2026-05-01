---
name: logging-agents
description: Use when agent が何らかの作業を進めるとき default-on で使う。ただし hard gate はユーザー訂正、方針変更、実装開始、再計画、テスト失敗、外部公開、完了前検証などの重要イベントに限定する。
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
hooks:
  Stop:
    - hooks:
        - type: command
          command: ~/.agents/skills/logging-agents/check-logging-event.sh
          timeout: 5
          statusMessage: logging-agents stop gate check...
  SubagentStop:
    - hooks:
        - type: command
          command: ~/.agents/skills/logging-agents/check-logging-event.sh
          timeout: 5
          statusMessage: logging-agents subagent gate check...
---

# Logging Agents

この skill は agent の判断が変わった点を残すための logging gate である。全アクションを記録するものではない。

<EXTREMELY-IMPORTANT>
logging-agents は default-on だが、hard gate は重要イベントに限定する。

重要イベントとは、ユーザー訂正、方針変更、実装開始、再計画、テスト失敗、外部公開、完了前検証である。

単なる調査、status 確認、連続する git 操作、短い progress commentary は、直近 event に含められるなら新規 event を作らない。
</EXTREMELY-IMPORTANT>

<HARD-GATE>
重要イベントが起きたら event file を書き終えて成功確認するまで、次の状態遷移に進んではならない。

hard gate の対象:

- ファイル編集をしてはならない
- テストや検証の次段階に進んではならない
- 完了報告をしてはならない
- 外部公開や同期をしてはならない

`write-event.sh` が失敗したら 1 回だけ再試行する。再試行しても失敗した場合、完了前検証と外部公開では停止してユーザーに報告する。それ以外の通常作業では warning として扱い、次に成功した event に失敗事実を含めて進める。
</HARD-GATE>

<SOFT-GATE>
以下は新規 event を必須にしない。直近または次の重要 event に含められるなら、そこでまとめる。

- 単なる status 確認
- `git status` や差分確認だけの操作
- `push` や `finish` workflow の途中経過
- 同一目的の read / search / execute の反復
- 短い progress commentary
</SOFT-GATE>

## 最初に守ること

- event file 作成には同じ skill directory の `./write-event.sh` を優先して使う
- 重要イベントでは、次の状態遷移より先に event file を書く
- 同一フェーズの連続 event は 1 つにまとめる

## いつ有効化するか

この skill は opt-in ではなく default-on として扱う。

- ユーザー訂正、方針変更、実装開始、再計画、テスト失敗、外部公開、完了前検証が起きたとき
- ユーザーが `logging-agents` または `logging-agent` を明示したとき
- ユーザーが観測、監査、可視化、ログ、トレース、記録、メモ化を求めたとき
- skill の発動、ユーザー訂正、agent の軌道修正、実装・調査・テストの重要な状態遷移を残したいとき

迷ったら inactive ではなく active に倒す。

## 指示の優先順位

1. ユーザーの明示的指示
2. この skill
3. デフォルトの system behavior

ユーザーがログ対象、保存先、表示方法、gate 粒度を指定したら従う。ただし、完了前検証の event は削らない。

## ワークフロー

```text
- [ ] Step 1: 重要イベントか soft event かを判定する
- [ ] Step 2: soft event なら直近または次の event に統合できるか判定する
- [ ] Step 3: 重要イベントなら `./write-event.sh` で event file を作成する
- [ ] Step 4: 書き込み成功を確認する。失敗したら 1 回だけ再試行する
- [ ] Step 5: 成功後、または soft failure の warning 記録方針を決めた後に次へ進む
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

`μf` 相当の評価を残すときの例:

```sh
./write-event.sh \
  --event artifact_change \
  --agent codex:main \
  --skill logging-agents \
  --field phase=testing \
  --field action=verify \
  --field score=0.75 \
  --field candidate_id=candidate-12 \
  --field module_id=evaluation-feedback \
  --field-file evaluation_trace=tmp/eval-trace.txt \
  --field-file feedback_text=tmp/feedback-text.txt \
  --summary "理由付き採点を記録する。" \
  --evidence "judge が fail 理由を返した。" \
  --next-action "recording に渡す。"
```

## 保存先とファイル形式

保存先:

- `codex:*` は `~/.codex/mylogs/logging-agents/`
- `claude-code:*` は `~/.claude/mylogs/logging-agents/`

後段 skill への handoff で最低限そろえる項目:

- `candidate_id`
- `module_id`
- `execution_trace`
- `evaluation_trace`
- `feedback_text`
- `score`

値がないなら空欄のままにせず、その event では不要だと分かる Summary/Evidence を書く。
長い judge 出力や失敗理由は `--field-file` で block scalar として保存する。

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

最初に扱う event は次の 5 種:

- `skill_invoked`
- `user_correction_inferred`
- `agent_replan`
- `artifact_change`
- `finish_workflow`

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

成果物、方針、検証状態、またはユーザーに返す判断が変わる瞬間だけ書く。
単なる read / edit / execute / verify の反復は書かない。

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `phase`
- `action`
- `skills_active`

`μf` を使った評価イベントでは次も入れる:

- `score`
- `evaluation_trace`
- `feedback_text`

推奨 fields:

- `candidate_id`
- `module_id`
- `module_feedback`

`phase` の例:

- `research`
- `planning`
- `implementation`
- `testing`
- `review`

書く例:

- patch を適用した
- 検証が fail から pass に変わった
- 返答方針を確定した
- module 単位の改善候補を確定した

書かない例:

- 同じ目的の `rg` を追加で打った
- 差分確認を細かく繰り返した
- 1 ファイル読んだだけ
- 途中で 1 回 shell を打っただけ

### `finish_workflow`

同一フェーズの finish / commit / worktree finish / post-finish verify は 1 つにまとめる。

まとめる例:

- `finish_start`
- `commit_start`
- `worktree_finish_start`
- `post_finish_verify`

必須 fields:

- `schema`
- `event`
- `timestamp`
- `agent`
- `phase`
- `skills_active`

推奨 fields:

- `included_steps`
- `verification_status`
- `publication_status`

## Gotchas

- `skills_active` にはその時点で有効な skill を全部入れる。`logging-agents` 自身を必ず含める
- `execution_trace` と `evaluation_trace` を混同しない。前者は system の実行痕跡、後者は採点・失敗理由・judge 出力である
- `feedback_text` が長いときは `--field` に詰め込まず `--field-file` を使う
- commentary ごとの hard gate は作らない。重要な状態遷移ごとに記録する
- `confidence=low` のユーザー訂正でも、作業方針に影響するなら記録する
- 「今回は小さいから記録しない」ではなく「重要イベントか、直近 event に統合できる soft event か」で判定する
- `finish_workflow` の途中経過を細かい event に分けない
- サブエージェントは main と分けて `agent` を書く
- `write-event.sh` で失敗したら 1 回だけ再試行する。完了前検証と外部公開以外は warning で進めてよい

## アンチパターン

- commentary、status 確認、単発 tool 実行ごとに event を量産する
- 重要イベントの event file を後回しにする
- 反省で使わない微動を `artifact_change` として量産する
- `score` だけ残して `feedback_text` を捨てる
- `skills_active` から `logging-agents` を落とす
- 作業方針に影響する `user_correction_inferred` を「明示されていないから」で書かない
- サブエージェントの event を main にまとめる

## 要点

この skill の責務は、判断が変わった重要イベントを event file として残すことである。全アクションの記録ではなく、後から意思決定を追える粒度を守る。
