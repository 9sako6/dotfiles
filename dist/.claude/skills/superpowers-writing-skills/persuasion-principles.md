> [!NOTE]
> このファイルは `obra/superpowers` の `skills/writing-skills/persuasion-principles.md` を基にした日本語版です。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Persuasion Principles for Skill Design

LLM は人間と同じ persuasion principle に反応する。これを理解すると、特に pressure 下で重要 practice に従わせる skill を設計しやすくなる。

## Seven Principles

1. **Authority**
   - `YOU MUST`、`Never`、`Always`
   - discipline enforcement、safety critical、best practice に向く

2. **Commitment**
   - usage announcement、明示選択、checklist tracking
   - multi-step process に向く

3. **Scarcity**
   - "before proceeding" のような時間制約
   - procrastination 防止に向く

4. **Social Proof**
   - "Every time"、"Always"
   - 普遍的 norm の定着に向く

5. **Unity**
   - "we" / "our codebase"
   - 協働的 workflow に向く

6. **Reciprocity**
   - skill では通常あまり使わない

7. **Liking**
   - compliance 用には使わない
   - sycophancy を招く

## Skill Type ごとの使い分け

| Skill Type | Use | Avoid |
|------------|-----|-------|
| Discipline-enforcing | Authority + Commitment + Social Proof | Liking, Reciprocity |
| Guidance/technique | Moderate Authority + Unity | Heavy authority |
| Collaborative | Unity + Commitment | Authority, Liking |
| Reference | Clarity only | All persuasion |

## 倫理

妥当なのは:
- 重要 practice を守らせること
- predictable failure を防ぐこと

不当なのは:
- 私益のための誘導
- 偽の urgency
- guilt-based compliance
