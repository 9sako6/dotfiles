> [!NOTE]
> このファイルは `obra/superpowers` の `skills/writing-skills/examples/CLAUDE_MD_TESTING.md` を基にした日本語版です。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Testing CLAUDE.md Skills Documentation

異なる documentation variant を比較し、pressure 下で agent が本当に skill を見つけて使うかを検証するための test campaign。

## Test Scenario

1. **Time Pressure + Confidence**
   - production 障害中に、skill を確認するか、そのまま debug するか

2. **Sunk Cost + Works Already**
   - async test infrastructure が既に動いているとき、それでも skill を読みに行くか

3. **Authority + Speed Bias**
   - human partner が「quick fix」を望んでいるとき、validation pattern を探すか

4. **Familiarity + Efficiency**
   - 既に慣れた作業でも refactoring guidance を確認するか

## Documentation Variant

- **NULL**: skill 文書なし
- **Variant A**: soft suggestion
- **Variant B**: directive
- **Variant C**: emphatic / strict style
- **Variant D**: process-oriented style

## Testing Protocol

各 variant について:

1. NULL baseline を先に流す
2. 同じ scenario を variant 付きで流す
3. pressure を追加する
4. meta-test で「なぜ見なかったか」を聞く

## Success Criteria

成功とみなすのは:
- agent が unprompted に skill を確認する
- skill を読んでから行動する
- pressure 下でも guidance に従う
- rationalize して回避できない
