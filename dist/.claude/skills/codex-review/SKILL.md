---
name: codex-review
description: Use when Claude Code で OpenAI Codex plugin が使え、実装後やマージ前に独立した別視点のコードレビューを追加したいとき。自分の実装バイアスを避けたい場合、現在の作業ツリーやベースブランチとの差分に対して bug-focused review や adversarial review を依頼するときに使う。
compatibility: Designed for Claude Code with the OpenAI Codex plugin installed
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
---

# Codexレビュー

実装したAI自身のセルフレビューにはバイアスが残る。Claude Code から OpenAI Codex plugin を使って別のAIに委任することで、独立した視点の指摘を得る。

## 前提条件

この skill は **Claude Code 専用**。OpenAI Codex plugin が未セットアップ、または slash command が使えない場合は、先にセットアップしてそこで止まる:

1. `/plugin marketplace add openai/codex-plugin-cc`
2. `/plugin install codex@openai-codex`
3. `/reload-plugins`
4. `/codex:setup`（インストール・認証を対話的に実施）

`/codex:review` または `/codex:adversarial-review` が利用可能になったことを確認してから次に進む。

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
`--background` を使う場合は、結果を回収して読むところまでこの skill の責務とする。

### Step 2: レビュー実行

目的に応じてコマンドを選択し実行する:

| 目的 | コマンド |
|---|---|
| バグ・ロジックエラーの検出 | `/codex:review` |
| 設計判断・前提への異議申し立て | `/codex:adversarial-review` |

- 通常のセルフレビューには `/codex:review` を使う
- 設計の妥当性を問いたい場合のみ `/codex:adversarial-review` を使う
- ユーザーから指定がなければ `/codex:review` をデフォルトとする
- Step 1 で確認したスコープに合わせて、必要なら `--base <branch>` や `--background` を同じコマンドに付ける
- Codexの出力はそのまま保持し、書き換えない

実行例:

- 作業ツリー全体の通常レビュー: `/codex:review`
- `main` との差分レビュー: `/codex:review --base main`
- バックグラウンドで通常レビュー: `/codex:review --background`
- `main` との差分に対する adversarial review: `/codex:adversarial-review --base main`

### Step 3: 結果確認・対応

Codexの出力をユーザーに提示し、対応方針を決める:

- **重大な指摘**（バグ・セキュリティ）→ 修正必須
- **改善提案** → ユーザー判断
- **スタイル指摘** → 任意

`--background` を使った場合は、レビューが完了したことを確認し、出力を回収してから上記の分類に進む。
レビューが失敗した、または出力が取得できない場合は、失敗を明示して再実行するかユーザーに次の方針を確認する。

Codexの指摘はあくまで参考意見。最終判断はユーザーが行う。
