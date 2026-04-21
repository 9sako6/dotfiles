> [!NOTE]
> このファイルは `obra/superpowers` の `skills/writing-skills/anthropic-best-practices.md` を基にした日本語版です。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Skill Authoring Best Practices

Claude が見つけやすく、使いやすい skill を書くための実践ガイド。

## Core Principle

### Concise is key

context window は共有資源である。skill は system prompt、会話履歴、他 skill metadata、ユーザー要求と同じ窓を共有する。

- Claude が既に知っていることは繰り返さない
- 各段落について「本当に必要か？」を問う
- `SKILL.md` が読み込まれた後は、1 token ごとに他の context と競合する

### Degrees of Freedom を調整する

task の壊れやすさと variability に合わせて、指示の自由度を決める。

- **High freedom**: 複数の妥当なやり方がある場合
- **Medium freedom**: 好ましい pattern はあるが variation も許される場合
- **Low freedom**: fragile で、正確な順序が必要な場合

### 利用予定の model で test する

同じ skill でも model によって効き方が違う。

- Haiku 系: guidance は十分か
- Sonnet 系: clear で efficient か
- Opus 系: 過剰説明になっていないか

## Skill Structure

### Naming Convention

一貫した命名を使う。推奨は **gerund form**（動詞 + ing）:

- Processing PDFs
- Analyzing spreadsheets
- Writing documentation

避けるもの:
- Helper
- Utils
- Tools
- あいまいすぎる総称

### Effective Description

`description` は discovery のための field であり、**何をするか**と**いつ使うか**の両方が分かる必要がある。

- 必ず 3 人称で書く
- vague な記述を避ける
- file type、symptom、trigger condition を入れる

悪い例:
- "Helps with documents"
- "Processes data"

良い例:
- PDF file、form、document extraction に言及する
- Excel file、spreadsheet、`.xlsx` に言及する
- commit message、staged change に言及する

## Progressive Disclosure

`SKILL.md` は overview であり、必要に応じて詳細資料に導く table of contents として振る舞うのがよい。

- 軽い overview は `SKILL.md`
- 重い reference は別 file
- reusable script は別 file

## 要点

良い skill は:
- 短い
- 構造化されている
- discovery しやすい
- 実使用で test されている
