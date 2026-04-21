---
name: superpowers-verification-before-completion
description: 作業完了、修正済み、成功状態などを主張する直前、または commit や PR 作成の前に使う。成功を述べる前に verification command を実行し、その出力を確認する必要がある。常に主張より証拠が先
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/verification-before-completion/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/verification-before-completion/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# 完了前の検証

## 概要

検証なしに「完了した」と言うのは、効率ではなく不誠実である。

**中核原則:** 主張より先に証拠。常に。

**このルールの字面に違反することは、その精神に違反することである。**

## 鉄則

```
NO COMPLETION CLAIMS WITHOUT FRESH VERIFICATION EVIDENCE
```

このメッセージの中で verification command を実行していないなら、成功しているとは主張できない。

## Gate Function

```
BEFORE claiming any status or expressing satisfaction:

1. IDENTIFY: どの command がこの主張を証明するか？
2. RUN: FULL command を実行する（fresh、complete）
3. READ: 出力全体を読み、exit code を見て、failure 数を数える
4. VERIFY: 出力はその主張を裏付けているか？
   - If NO: 証拠つきで実際の状態を述べる
   - If YES: 証拠つきで主張する
5. ONLY THEN: 主張する

どれかを飛ばす = 検証していない、つまり嘘をついている
```

## よくある failure

| Claim | Requires | Not Sufficient |
|-------|----------|----------------|
| Tests pass | Test command output: 0 failures | Previous run, "should pass" |
| Linter clean | Linter output: 0 errors | Partial check, extrapolation |
| Build succeeds | Build command: exit 0 | Linter passing, logs look good |
| Bug fixed | Test original symptom: passes | Code changed, assumed fixed |
| Regression test works | Red-green cycle verified | Test passes once |
| Agent completed | VCS diff shows changes | Agent reports "success" |
| Requirements met | Line-by-line checklist | Tests passing |

## Red Flags - STOP

- "should"、"probably"、"seems to" を使い始めている
- 検証前に満足を表現しそうになっている（"Great!"、"Perfect!"、"Done!" など）
- 検証前に commit/push/PR しようとしている
- agent の success report を信じている
- partial verification に頼っている
- "just this once" と思っている
- 疲れて早く終わらせたくなっている
- **verification を実行せずに成功をにおわせるあらゆる wording**

## 合理化の防止

| Excuse | Reality |
|--------|---------|
| "Should work now" | verification を実行する |
| "I'm confident" | 自信 ≠ 証拠 |
| "Just this once" | 例外はない |
| "Linter passed" | linter ≠ compiler |
| "Agent said success" | 独立に検証する |
| "I'm tired" | 疲労は言い訳にならない |
| "Partial check is enough" | 部分確認では何も証明できない |
| "Different words so rule doesn't apply" | 字面より精神 |

## 主なパターン

**Tests:**
```
✅ [Run test command] [See: 34/34 pass] "All tests pass"
❌ "Should pass now" / "Looks correct"
```

**Regression tests（TDD Red-Green）:**
```
✅ Write → Run (pass) → Revert fix → Run (MUST FAIL) → Restore → Run (pass)
❌ "I've written a regression test"（red-green verification なし）
```

**Build:**
```
✅ [Run build] [See: exit 0] "Build passes"
❌ "Linter passed"（linter は compile を見ない）
```

**Requirements:**
```
✅ Re-read plan → Create checklist → Verify each → Report gaps or completion
❌ "Tests pass, phase complete"
```

**Agent delegation:**
```
✅ Agent reports success → Check VCS diff → Verify changes → Report actual state
❌ Trust agent report
```

## なぜ重要か

24 件の failure memory から:
- human partner に "I don't believe you" と言われた - 信頼が壊れた
- 未定義関数を shipping しかけた - crash するはずだった
- 未実装要件のまま shipping しかけた - 不完全な機能になった
- 偽の完了報告 → 差し戻し → 手戻りで時間を失った
- 次の原則に違反する: "Honesty is a core value. If you lie, you'll be replaced."

## 適用タイミング

**必ず適用するのは次の前:**
- あらゆる成功 / 完了主張の variation
- あらゆる満足表現
- 作業状態に関するあらゆる肯定文
- commit、PR 作成、task 完了
- 次の task へ進む前
- agent への delegation 前

**このルールが適用されるもの:**
- 正確な定型句
- 言い換えや同義語
- 成功を示唆する含意
- 完了 / 正しさを示唆するあらゆる communication

## 要点

**検証に近道はない。**

command を実行する。出力を読む。そのあとで結果を主張する。

これは交渉の余地がない。
