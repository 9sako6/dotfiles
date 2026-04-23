---
name: evolving
description: Use when logging-agents の event と recording の提案を読み、再発防止に効く skill 差分を局所的に作るか、0 件で止めるかを判定するときに使う。
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
---

# Skill Evolution

この skill の責務は、既存 skill の局所差分案を作ることだけである。新しい責務を足して問題をごまかしてはならない。

## 入力

- recent な logging-agents event
- recording が出した handoff packet
- 会話中の accepted / rejected / hold

入力 packet には最低限これを含める:

- `owner skill`
- `target module`
- `root cause`
- `minimal diff`
- `status`
- `evidence`
- `score`
- `evaluation_trace`
- `feedback_text`

## ワークフロー

```text
- [ ] Step 1: 再発パターンを 1 件選ぶ
- [ ] Step 2: owner skill を 1 つに絞る
- [ ] Step 3: policy / docs / 既存 skill と衝突しないか確認する
- [ ] Step 4: target module にだけ効く最小 diff を書く
- [ ] Step 5: accepted / rejected / hold の判断材料を残す
- [ ] Step 6: 0 件なら 0 件で終了する
```

## ルール

- 1 つの再発パターンは 1 つの owner skill にだけ帰属させる
- 1 回の更新で 1 module だけを重点的に改稿する
- policy as code に落とせるものを skill 差分にしない
- accepted / rejected / hold の根拠がない案は採用候補にしない
- judge 依存の失敗で `feedback_text` または `evaluation_trace` が欠ける案は `hold` にする
- 新しい保存層や新しい skill を増やして問題をごまかさない
- 0 件は正常な結果である

## 出力

各提案に必ず次を残す:

- `owner skill`
- `target module`
- `root cause`
- `minimal diff`
- `expected effect`
- `acceptance evidence`
- `status: accepted / rejected / hold`

`status=hold` の典型例:

- score はあるが failure reason がない
- target module を評価 trace から特定できない

## アンチパターン

- 複数 skill にまたがる大きな全面書き換え
- logging の量を増やすだけで改善したつもりになる
- recording の分類を飛ばして思いつきで diff を書く
- accepted / rejected を残さずに次へ進む

## 要点

この skill は mutation と採否判定の担当である。記録の担当でも、永続化先の選定担当でもない。
