> [!NOTE]
> このファイルは `obra/superpowers` の `skills/test-driven-development/testing-anti-patterns.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Testing Anti-Patterns

この文書は、TDD を形だけにしてしまう anti-pattern を集めた reference である。

## 代表的な anti-pattern

- 実装の後で test を書く
- mock の挙動だけを test して、実際の behavior を検証しない
- 一つの test に複数責務を詰め込む
- 失敗の確認を飛ばす
- 何が壊れたか分からない曖昧な test 名にする
- green になる前に refactor し始める

## 原則

- test は 1 つの behavior を示す
- test 名は期待する behavior を説明する
- failing test を見てから production code に進む
- mock は最後の手段であり、便利だからという理由では使わない
