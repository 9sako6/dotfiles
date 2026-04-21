---
name: superpowers-requesting-code-review
description: タスク完了時、大きな機能の実装後、または merge 前に、作業が要件を満たしているか確認するために使う
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/requesting-code-review/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/requesting-code-review/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# コードレビューを依頼する

問題が連鎖する前に `superpowers:code-reviewer` subagent を dispatch して問題を拾う。reviewer には評価に必要な文脈だけを精密に与え、あなたのセッション履歴は決して渡さない。これにより、reviewer はあなたの思考過程ではなく成果物に集中でき、自分の文脈もその後の作業のために温存できる。

**中核原則:** 早くレビューし、頻繁にレビューする。

## レビューを依頼するタイミング

**必須:**
- subagent-driven development の各 task 後
- 大きな機能を完了した後
- main に merge する前

**任意だが価値がある:**
- 行き詰まったとき（新しい視点）
- refactoring の前（baseline の確認）
- 複雑な bug 修正の後

## 依頼方法

**1. git SHA を取得する:**
```bash
BASE_SHA=$(git rev-parse HEAD~1)  # or origin/main
HEAD_SHA=$(git rev-parse HEAD)
```

**2. `code-reviewer` subagent を dispatch する:**

Task tool を `superpowers:code-reviewer` type で使い、`code-reviewer.md` の template を埋める。

**placeholder:**
- `{WHAT_WAS_IMPLEMENTED}` - 今まさに作ったもの
- `{PLAN_OR_REQUIREMENTS}` - それが本来何をすべきか
- `{BASE_SHA}` - 開始 commit
- `{HEAD_SHA}` - 終了 commit
- `{DESCRIPTION}` - 短い要約

**3. フィードバックに対応する:**
- Critical issue は即座に直す
- Important issue は次へ進む前に直す
- Minor issue は後回し候補として記録する
- reviewer が誤っているなら理由を添えて反論する

## 例

```
[Just completed Task 2: Add verification function]

You: Let me request code review before proceeding.

BASE_SHA=$(git log --oneline | grep "Task 1" | head -1 | awk '{print $1}')
HEAD_SHA=$(git rev-parse HEAD)

[Dispatch superpowers:code-reviewer subagent]
  WHAT_WAS_IMPLEMENTED: Verification and repair functions for conversation index
  PLAN_OR_REQUIREMENTS: Task 2 from docs/superpowers/plans/deployment-plan.md
  BASE_SHA: a7981ec
  HEAD_SHA: 3df7661
  DESCRIPTION: Added verifyIndex() and repairIndex() with 4 issue types

[Subagent returns]:
  Strengths: Clean architecture, real tests
  Issues:
    Important: Missing progress indicators
    Minor: Magic number (100) for reporting interval
  Assessment: Ready to proceed

You: [Fix progress indicators]
[Continue to Task 3]
```

## Workflow との統合

**Subagent-Driven Development:**
- 各 task の後にレビューする
- 問題が積み上がる前に見つける
- 次の task へ進む前に修正する

**Executing Plans:**
- 各 batch（3 task）後にレビューする
- フィードバックを得て、反映して、続行する

**Ad-Hoc Development:**
- merge 前にレビューする
- 行き詰まったときにレビューする

## Red Flags

**絶対にしてはならない:**
- 「簡単だから」とレビューを飛ばす
- Critical issue を無視する
- Important issue を未修正のまま進む
- 妥当な技術フィードバックに言い争いで返す

**reviewer が間違っている場合:**
- 技術的理由で反論する
- 動作を証明する code/test を示す
- clarification を求める

template はここを参照: `requesting-code-review/code-reviewer.md`
