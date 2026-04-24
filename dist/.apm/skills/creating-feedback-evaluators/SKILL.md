---
name: creating-feedback-evaluators
description: Use when task-specific evaluators that return score plus diagnostic feedback are needed for prompt or skill evolution, especially when pass/fail alone is too weak to drive localized improvements.
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
---

# Feedback Evaluator を作る

この skill は、task 専用の feedback evaluator を設計するときに使う。  
目的は、pass/fail だけでは足りない改善ループに対して、`score` と診断テキストを返す `μf` を作ること。

## いつ使うか

- `recording` が `hold` を増やし、failure reason が薄いとき
- `evolving` が `target module` を特定できないとき
- テストはあるが、局所改稿に使える診断が返ってこないとき
- task ごとに judge や rubric を作りたいとき

## この skill の責務

- evaluator の入力を決める
- `score / evaluation_trace / feedback_text / module_feedback` の出力契約を決める
- hard checks と soft checks を分ける
- `hold` 条件を決める
- evaluator の最小テストを決める

この skill は evaluator を実行しない。  
記録は `logging-agents`、抽出は `recording`、改稿は `evolving` が担当する。

## リソースの置き場

task ごとの evaluator resources に公開できない情報が含まれるなら、repo には置かない。

- git 管理するもの:
  - evaluator の共通 code
  - 汎用 schema
  - 再利用できる test helper
- local-only に置くもの:
  - task 定義
  - rubric
  - private fixtures
  - expected outputs
  - run artifacts

推奨置き場:

```text
~/.agents/local/feedback-evaluators/<task-slug>/
  task.md
  rubric.md
  fixtures/
  expected/
  runs/
  tmp/
```

`~/.agents/skills` の managed skill 配下には置かない。  
配布物と local-only data を混ぜない。

## コアパターン

1. まず hard checks を作る  
2. 次に soft checks を作る  
3. 最後に output schema と `hold` 条件を固定する

### Hard Checks

機械的に判定できるものだけを入れる。

例:

- test command の exit code
- 期待ファイルの有無
- forbidden path の不使用
- 必須 field の有無
- schema の妥当性

### Soft Checks

LLM judge に任せるが、観点は固定する。

例:

- 責務境界が明確か
- 最小 diff になっているか
- 再発防止に効くか
- `target module` が妥当か
- feedback が具体的で局所改稿に使えるか

## 推奨 output schema

```yaml
score: 0.0-1.0
evaluation_trace:
  hard_checks:
    <name>: pass|fail
  soft_checks:
    <name>: pass|partial|fail
feedback_text: |
  なぜこの score なのかを書く。
module_feedback:
  <module-name>: |
    module ごとの診断を書く。
```

## Hold 条件

次のどれかに当てはまるなら `hold` にする。

- `score` はあるが failure reason がない
- `target module` を特定できない
- hard check が fail していて soft check を信じる意味がない
- feedback が抽象的すぎて diff に落ちない

## 実装手順

```text
- [ ] Step 1: 評価対象 task を 1 つに絞る
- [ ] Step 2: hard checks を列挙する
- [ ] Step 3: soft checks を 3-6 個に絞る
- [ ] Step 4: output schema を固定する
- [ ] Step 5: hold 条件を明文化する
- [ ] Step 6: local-only resources の置き場を決める
- [ ] Step 7: 最小テストを作る
```

## よくある失敗

- 汎用 evaluator を最初から作ろうとする
- score だけ返して `feedback_text` を省く
- soft checks だけで採択しようとする
- `module_feedback` を task 全体の感想にしてしまう
- `hold` を用意せず、薄い feedback を無理に proposal 化する
- private task data を repo や managed skill 配下に置く

## 要点

task ごとに、小さく、診断可能で、局所改稿に使える evaluator を作る。  
まずは汎用化せず、1 task 専用で始める。
