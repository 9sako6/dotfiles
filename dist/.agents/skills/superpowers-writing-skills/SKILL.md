---
name: superpowers-writing-skills
description: 新しい skill を作るとき、既存 skill を編集するとき、または配布前に skill の効き方を検証するときに使う
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/writing-skills/SKILL.md
  translation: Japanese version based on upstream
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/writing-skills/SKILL.md` を基にした日本語版です。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Writing Skills

## 概要

**Writing skills は、process documentation に対する Test-Driven Development である。**

あなたは test case（subagent を使った pressure scenario）を書き、skill なしで fail する様子を見る（baseline behavior）。次に skill（documentation）を書き、agent が従うことを確認し、最後に loophole を塞ぐ。

**中核原則:** skill なしで agent が fail するのを見ていないなら、その skill が何を教えるべきか分かっていない。

**前提:** `superpowers:test-driven-development` を理解していること。そこにある RED-GREEN-REFACTOR cycle を、skill 文書化へ適用する。

## Skill とは何か

**skill** は、実証済みの technique、pattern、tool に関する reference guide である。将来の Claude が有効なアプローチを見つけて適用する助けになる。

**skill であるもの:**
- 再利用可能な technique
- pattern
- tool の使い方
- reference guide

**skill ではないもの:**
- 一度きりの問題をどう解いたかの narrative

## Skill 作成における TDD 対応

| TDD Concept | Skill Creation |
|-------------|----------------|
| Test case | subagent を使った pressure scenario |
| Production code | `SKILL.md` |
| RED | skill なしで rule を破る baseline |
| GREEN | skill があると agent が従う |
| Refactor | loophole を塞ぎつつ準拠状態を保つ |

## いつ skill を作るべきか

**作るべきなのは次のとき:**
- その technique が直感的ではなかった
- 今後も何度も参照したい
- pattern が project 固有ではなく広く適用できる
- 他の人にも利益がある

**作らない方がよいのは次のとき:**
- 一回限りの solution
- 既に他所で十分文書化された standard practice
- project 固有の convention（それは CLAUDE.md / AGENTS.md に書く）
- regex や validation で機械的に強制できる constraint

## Skill Type

### Technique
手順を伴う具体的方法（例: condition-based-waiting）

### Pattern
問題の捉え方（例: test-invariants）

### Reference
API docs、syntax guide、tool reference

## Directory Structure

```text
skills/
  skill-name/
    SKILL.md
    supporting-file.*
```

- namespace は flat に保つ
- 100 行を超える重い reference や reusable tool は別 file に出す
- 原則や短い code pattern は `SKILL.md` に inline で置く

## `SKILL.md` の構造

frontmatter には少なくとも `name` と `description` が必要:

- `name`: 文字、数字、hyphen のみ
- `description`: 3 人称で、**いつ使うか**だけを書く
- `description` は process ではなく trigger condition を表す

推奨構造:

1. Overview
2. When to Use
3. Core Pattern
4. Quick Reference
5. Implementation
6. Common Mistakes
7. Real-World Impact（必要なら）

## Claude Search Optimization (CSO)

将来の Claude が skill を見つけられるように、description は discovery の要である。

### 1. Rich Description Field

description は「今この skill を読むべきか？」に答えなければならない。

- "Use when..." で始める
- 何をする skill かではなく、**いつ使うべきか**を書く
- workflow を要約しない
- 具体的な symptom、situation、context を入れる

### 2. Keyword Coverage

Claude が検索しそうな語を含める:
- error message
- symptom
- synonym
- actual command / library / file type

### 3. Descriptive Naming

active voice、verb-first を優先する:
- `creating-skills`
- `condition-based-waiting`

### 4. Token Efficiency

token は共有資源である。常時読まれやすい skill ほど短く保つ。

- getting-started workflow: 150 words 未満
- 頻繁に読み込まれる skill: 200 words 未満
- その他: 500 words 未満を目安

冗長な flag 説明や workflow の重複は避け、必要なら `--help` や他 skill に参照させる。

## 要点

skill は「上手く書く」のではなく、「fail する baseline を見てから、そこを塞ぐために書く」。  
TDD と同じく、RED → GREEN → REFACTOR を documentation に適用する。
