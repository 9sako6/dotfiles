> [!NOTE]
> このファイルは `obra/superpowers` の `skills/writing-skills/testing-skills-with-subagents.md` を基にした日本語版です。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Testing Skills With Subagents

## 概要

**skill の test は、process documentation に対する TDD である。**

skill なしで scenario を流し、agent が fail するのを見る（RED）。その失敗を防ぐ skill を書き、同じ scenario で compliance を確認する（GREEN）。その後、loophole を塞ぐ（REFACTOR）。

## 使うタイミング

次のような skill を test する:
- discipline を強制する skill
- time / effort / rework の cost を伴う skill
- "just this once" と合理化されやすい skill
- immediate goal（速さ）と quality が衝突する skill

次のものは通常 test しなくてよい:
- pure reference skill
- 破るべき rule を持たない skill
- bypass する incentive がない skill

## TDD Mapping

| TDD Phase | Skill Testing |
|-----------|---------------|
| RED | skill なしで baseline failure を見る |
| Verify RED | rationalization を記録する |
| GREEN | skill を書く |
| Verify GREEN | pressure scenario で compliance を確認する |
| REFACTOR | loophole を塞ぐ |

## Good Scenario の条件

- concrete option を持つ（A/B/C）
- real constraint がある（time、cost、authority）
- actual path や situation がある
- "What should you do?" ではなく "What do you do?" と問う
- "I'd ask my human partner" に逃げられない

## Pressure Type

- Time
- Sunk cost
- Authority
- Economic
- Exhaustion
- Social pressure
- "Pragmatic vs dogmatic" framing

最も良い test は 3 つ以上の pressure を組み合わせる。

## REFACTOR

agent がなお rule を破るなら、その excuse を verbatim で記録する:

- "This case is different because..."
- "I'm following the spirit not the letter"
- "Being pragmatic means adapting"
- "Deleting X hours is wasteful"

そのうえで:
- rule の明示否定を足す
- rationalization table に entry を加える
- red flag に追記する
- description に trigger symptom を足す

## 要点

skill は、agent が rule を破りたくなる real pressure 下で test されて初めて信用できる。
