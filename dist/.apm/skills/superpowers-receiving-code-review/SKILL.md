---
name: superpowers-receiving-code-review
description: コードレビューのフィードバックを受けたとき、提案を実装する前に使う。特にフィードバックが不明瞭だったり技術的に疑わしいときに有効で、空疎な同意や盲目的な実装ではなく、技術的な厳密さと検証を求める
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/receiving-code-review/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/receiving-code-review/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# コードレビューの受け方

## 概要

コードレビューに必要なのは、感情的なパフォーマンスではなく技術的な評価である。

**中核原則:** 実装前に検証する。決めつける前に尋ねる。社会的な気まずさより技術的な正しさを優先する。

## 反応パターン

```
WHEN receiving code review feedback:

1. READ: 反応せずにフィードバック全体を読む
2. UNDERSTAND: 要求を自分の言葉で言い直す（または尋ねる）
3. VERIFY: codebase の現実と照らし合わせる
4. EVALUATE: THIS codebase にとって技術的に妥当か？
5. RESPOND: 技術的な acknowledgment または理由ある反論
6. IMPLEMENT: 一度に 1 項目ずつ、各項目を test する
```

## 禁止される反応

**絶対にしてはならない:**
- "You're absolutely right!"（明示的な CLAUDE.md 違反）
- "Great point!" / "Excellent feedback!"（パフォーマンス）
- "Let me implement that now"（検証前）

**代わりにすること:**
- 技術的要件を言い直す
- 確認質問をする
- 間違っているなら技術的理由で反論する
- ただ作業を始める（言葉より行動）

## 不明瞭なフィードバックの扱い

```
IF any item is unclear:
  STOP - まだ何も実装しない
  ASK for clarification on unclear items

WHY: 項目同士が関連しているかもしれない。部分理解 = 間違った実装。
```

**例:**
```
your human partner: "Fix 1-6"
You understand 1,2,3,6. Unclear on 4,5.

❌ WRONG: 1,2,3,6 を今すぐ実装し、4,5 は後で聞く
✅ RIGHT: "I understand items 1,2,3,6. Need clarification on 4 and 5 before proceeding."
```

## 出所ごとの扱い

### human partner からのもの
- **信頼してよい** - 理解した後に実装する
- **それでも聞く** - スコープが不明瞭な場合
- **空疎な同意はしない**
- **行動に進む** か **技術的に acknowledgment する**

### 外部レビュアーからのもの
```
BEFORE implementing:
  1. Check: THIS codebase にとって技術的に正しいか？
  2. Check: 既存機能を壊さないか？
  3. Check: 現行実装に理由はあるか？
  4. Check: すべての platform/version で動くか？
  5. Check: reviewer は全体文脈を理解しているか？

IF suggestion seems wrong:
  Push back with technical reasoning

IF can't easily verify:
  Say so: "I can't verify this without [X]. Should I [investigate/ask/proceed]?"

IF conflicts with your human partner's prior decisions:
  Stop and discuss with your human partner first
```

**your human partner's rule:** "External feedback - be skeptical, but check carefully"

## 「ちゃんと実装する」系の提案に対する YAGNI チェック

```
IF reviewer suggests "implementing properly":
  grep codebase for actual usage

  IF unused: "This endpoint isn't called. Remove it (YAGNI)?"
  IF used: Then implement properly
```

**your human partner's rule:** "You and reviewer both report to me. If we don't need this feature, don't add it."

## 実装順序

```
FOR multi-item feedback:
  1. まず不明点をすべて明確化する
  2. その後、この順で実装する:
     - Blocking issue（破損、security）
     - Simple fix（typo、import）
     - Complex fix（refactoring、logic）
  3. 各 fix を個別に test する
  4. regression がないことを確認する
```

## 反論すべきタイミング

次のときは反論する:
- 提案が既存機能を壊す
- reviewer が全体文脈を欠いている
- YAGNI に反する（未使用機能）
- この stack に対して技術的に誤っている
- legacy / compatibility 上の理由がある
- human partner のアーキテクチャ判断と衝突する

**反論のしかた:**
- 防御的になるのではなく、技術的理由を使う
- 具体的な質問をする
- 動いている test / code を参照する
- アーキテクチャに関わるなら human partner を巻き込む

**声に出して反論するのが気まずいときの合図:** "Strange things are afoot at the Circle K"

## 正しいフィードバックの acknowledgment

フィードバックが**正しい**場合:
```
✅ "Fixed. [Brief description of what changed]"
✅ "Good catch - [specific issue]. Fixed in [location]."
✅ [Just fix it and show in the code]

❌ "You're absolutely right!"
❌ "Great point!"
❌ "Thanks for catching that!"
❌ "Thanks for [anything]"
❌ ANY gratitude expression
```

**なぜ thanks を使わないのか:** 行動がすべてを語る。直せばよい。コードそのものが、フィードバックを受け取ったことを示す。

**"Thanks" と書きそうになったら:** DELETE IT. 代わりに fix を述べる。

## 反論が誤りだったときの訂正

もし反論して、実際には自分が間違っていた場合:
```
✅ "You were right - I checked [X] and it does [Y]. Implementing now."
✅ "Verified this and you're correct. My initial understanding was wrong because [reason]. Fixing."

❌ Long apology
❌ Defending why you pushed back
❌ Over-explaining
```

事実として訂正し、そのまま進む。

## よくある失敗

| Mistake | Fix |
|---------|-----|
| Performative agreement | 要件を述べるか、ただ動く |
| Blind implementation | 先に codebase で検証する |
| Batch without testing | 一つずつ、各 fix を test する |
| Assuming reviewer is right | 壊れないか確認する |
| Avoiding pushback | 心地よさより技術的正しさ |
| Partial implementation | まず全項目を明確化する |
| Can't verify, proceed anyway | 制約を明示し、方針を聞く |

## 実例

**Performative Agreement（Bad）:**
```
Reviewer: "Remove legacy code"
❌ "You're absolutely right! Let me remove that..."
```

**Technical Verification（Good）:**
```
Reviewer: "Remove legacy code"
✅ "Checking... build target is 10.15+, this API needs 13+. Need legacy for backward compat. Current impl has wrong bundle ID - fix it or drop pre-13 support?"
```

**YAGNI（Good）:**
```
Reviewer: "Implement proper metrics tracking with database, date filters, CSV export"
✅ "Grepped codebase - nothing calls this endpoint. Remove it (YAGNI)? Or is there usage I'm missing?"
```

**Unclear Item（Good）:**
```
your human partner: "Fix items 1-6"
You understand 1,2,3,6. Unclear on 4,5.
✅ "Understand 1,2,3,6. Need clarification on 4 and 5 before implementing."
```

## GitHub Thread への返信

GitHub 上の inline review comment に返信するときは、top-level の PR comment ではなく、comment thread に返信する（`gh api repos/{owner}/{repo}/pulls/{pr}/comments/{id}/replies`）。

## 要点

**外部フィードバック = 従う命令ではなく、評価すべき提案である。**

検証する。疑問を持つ。そのあとで実装する。

空疎な同意はしない。常に技術的厳密さを。
