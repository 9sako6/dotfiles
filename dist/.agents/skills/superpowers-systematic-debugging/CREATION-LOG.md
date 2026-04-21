> [!NOTE]
> このファイルは `obra/superpowers` の `skills/systematic-debugging/CREATION-LOG.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Creation Log: Systematic Debugging Skill

重要な skill をどう抽出し、構造化し、堅牢化したかを示す reference example。

## Source Material

`/Users/jesse/.claude/CLAUDE.md` から debugging framework を抽出:
- 4 phase の systematic process（Investigation → Pattern Analysis → Hypothesis → Implementation）
- 中核命令: ALWAYS root cause を見つけ、NEVER symptom を直す
- 時間的圧力や合理化に耐えるよう設計された rule

## 抽出時の判断

**含めたもの:**
- 4 phase framework 全体と rule
- short-cut を防ぐ指示（"NEVER fix symptom"、"STOP and re-analyze"）
- 圧力耐性を持つ wording（"even if faster"、"even if I seem in a hurry"）
- 各 phase の具体的 step

**除外したもの:**
- project 固有の文脈
- 同じ rule の重複表現
- narrative な説明（原則に圧縮）

## テストアプローチ

4 つの validation test を作成:

1. **Academic Context** - 圧力なしでも完全準拠するか
2. **Time Pressure + Obvious Quick Fix** - shortcut を拒否できるか
3. **Complex System + Uncertainty** - 多層 tracing ができるか
4. **Failed First Fix** - shotgun fix に走らず再分析できるか

**結果:** すべて通過。合理化は検出されなかった。

## Key Insight

最も重要だったのは、**その場で正当化したくなる shortcut を anti-pattern として明示したこと**。Claude が "just one quick fix" と考えた瞬間に、それが誤りだと目に入ることで認知的なブレーキがかかる。
