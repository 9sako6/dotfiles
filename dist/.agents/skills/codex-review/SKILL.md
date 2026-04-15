---
name: codex-review
description: OpenAI Codex CLIプラグインを使って外部AIにコードレビューを委任し、実装バイアスを排除した指摘を得る。別視点のセルフレビューが必要なときに使用する。
---

# Codexレビュー

実装したAI自身のセルフレビューにはバイアスが残る。別のAI（OpenAI Codex）に委任することで、独立した視点の指摘を得る。

## 前提条件

Codexプラグインが未セットアップの場合、先に実行する:

1. `/plugin marketplace add openai/codex-plugin-cc`
2. `/plugin install codex@openai-codex`
3. `/reload-plugins`
4. `/codex:setup`（インストール・認証を対話的に実施）

## ワークフロー

```
- [ ] Step 1: スコープ確認
- [ ] Step 2: レビュー実行
- [ ] Step 3: 結果確認・対応
```

### Step 1: スコープ確認

ユーザーにレビュー対象を確認する:

| 対象 | 指定方法 |
|---|---|
| 作業ツリー全体（デフォルト） | 引数なし |
| 特定ブランチとの差分 | `--base <branch>` |
| バックグラウンド実行 | `--background` |

スコープが不明確なら作業ツリー全体をデフォルトとする。

### Step 2: レビュー実行

目的に応じてコマンドを選択し実行する:

| 目的 | コマンド |
|---|---|
| バグ・ロジックエラーの検出 | `/codex:review` |
| 設計判断・前提への異議申し立て | `/codex:adversarial-review` |

- 通常のセルフレビューには `/codex:review` を使う
- 設計の妥当性を問いたい場合のみ `/codex:adversarial-review` を使う
- ユーザーから指定がなければ `/codex:review` をデフォルトとする
- Codexの出力はそのまま保持し、書き換えない

### Step 3: 結果確認・対応

Codexの出力をユーザーに提示し、対応方針を決める:

- **重大な指摘**（バグ・セキュリティ）→ 修正必須
- **改善提案** → ユーザー判断
- **スタイル指摘** → 任意

Codexの指摘はあくまで参考意見。最終判断はユーザーが行う。
