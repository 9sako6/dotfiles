> [!NOTE]
> このファイルは `obra/superpowers` の `skills/systematic-debugging/defense-in-depth.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Defense-in-Depth Validation

## 概要

無効な data が原因の bug を直したとき、1 箇所に validation を足せば十分に見える。しかし、その 1 箇所の check は別 code path、refactoring、mock によって回避されうる。

**中核原則:** data が通る**すべての layer** で validation する。その bug を構造的に不可能にする。

## なぜ複数 layer なのか

単一 validation: "We fixed the bug"  
複数 layer: "We made the bug impossible"

異なる layer は異なるケースを拾う:
- entry validation は大半の bug を捕まえる
- business logic は edge case を捕まえる
- environment guard は文脈依存の危険を防ぐ
- debug logging は他 layer が失敗したときの forensics を助ける

## 4 つの Layer

### Layer 1: Entry Point Validation
**目的:** API 境界で明らかに無効な input を弾く

### Layer 2: Business Logic Validation
**目的:** その operation に対して data が意味を持つことを保証する

### Layer 3: Environment Guard
**目的:** 特定文脈での危険な operation を防ぐ

### Layer 4: Debug Instrumentation
**目的:** forensic 用に文脈を保存する

## パターンの適用

bug を見つけたら:

1. **data flow を辿る** - 壊れた値はどこで生まれ、どこで使われるか？
2. **checkpoint を洗い出す** - data が通るすべての地点を列挙する
3. **各 layer に validation を足す** - entry、business、environment、debug
4. **各 layer を test する** - Layer 1 を迂回して、Layer 2 が拾うことを確認する

## 核心

**1 箇所の validation で止まらないこと。** あらゆる layer に check を足す。
