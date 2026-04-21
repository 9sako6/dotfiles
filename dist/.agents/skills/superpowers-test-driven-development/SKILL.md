---
name: superpowers-test-driven-development
description: 機能追加や bugfix を実装するとき、実装 code を書く前に使う
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/test-driven-development/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/test-driven-development/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Test-Driven Development (TDD)

## 概要

先に test を書く。失敗するのを確認する。通すための最小 code を書く。

**中核原則:** test が失敗するのを見ていないなら、その test が正しいことを確かめていない。

**このルールの字面に違反することは、その精神に違反することである。**

## 使うタイミング

**常に:**
- 新機能
- bug fix
- refactoring
- 挙動変更

**例外（human partner に確認する）:**
- 使い捨て prototype
- 生成 code
- 設定 file

"skip TDD just this once" と思ったら止まる。それは合理化である。

## 鉄則

```
NO PRODUCTION CODE WITHOUT A FAILING TEST FIRST
```

test より先に code を書いたか？ 削除してやり直す。

**例外はない:**
- "reference" として残さない
- test を書きながら「適応」しない
- 見返さない
- delete とは本当に delete すること

test から fresh に実装する。それだけ。

## Red-Green-Refactor

### RED - 失敗する Test を書く

起きるべきことを示す、最小の test を 1 つ書く。

**要件:**
- 1 つの挙動だけ
- 明確な名前
- 実 code を使う（避けられない場合を除き mock しない）

### Verify RED - 失敗することを確認する

**必須。飛ばしてはならない。**

確認すること:
- test は fail しているか（error ではなく）
- failure message は想定どおりか
- feature 不足で fail しているか（typo ではなく）

### GREEN - 最小実装

test を通すための最も単純な code を書く。

余計な機能を足さない。他の code を refactor しない。test を超えて「改善」しない。

### Verify GREEN - 通ることを確認する

**必須。**

確認すること:
- test が通る
- 他の test も通る
- 出力が clean（error / warning なし）

### REFACTOR - 整理する

green の後だけ:
- 重複を取り除く
- 名前を改善する
- helper を抽出する

test は green のままに保つ。挙動は足さない。

## なぜ順番が重要か

tests-after は「これは何をするか？」に答える。tests-first は「これは何をすべきか？」に答える。

tests-after は実装に引っ張られる。自分が作ったものを test するのであって、要求されていたものを test するとは限らない。

tests-first は、実装前に edge case を発見させる。tests-after は、思い出せたものだけを後追いで確認する。

## よくある合理化

- "Too simple to test" → 単純な code も壊れる
- "I'll write tests after" → それは TDD ではない
- "I manually tested it" → 再現性がない
- "Deleting code is wasteful" → 信頼できない code を残す方が損

## 要点

先に test。失敗を確認。最小実装。通ることを確認。必要なら整理。
