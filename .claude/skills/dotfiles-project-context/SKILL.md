---
name: dotfiles-project-context
description: Use when working in the dotfiles repository - provides project structure, key commands, deployment strategy, and conventions
user-invocable: false
---

# dotfiles プロジェクトコンテキスト

## 概要

`dist/` 以下の設定ファイルをホームディレクトリにシンボリックリンク（一部はコピー）で配備するdotfiles管理リポジトリ。

ランタイム: **Bun** / ツール管理: **mise** / テスト: `bun test`

## ディレクトリ構造

```
dist/           配備対象ファイル（ホームディレクトリへリンクされる）
scripts/        配備ロジック（TypeScript）
  link-dist.ts  エントリポイント
  lib/          補助ライブラリ（link-dist, paths, backup, fs, setup）
tests/          契約テスト（bun:test）
docs/           永続ドキュメント（repo-map, operations）
tmp/            一時メモ（gitignore済み）
```

## 主要コマンド

```bash
mise run doctor       # 現在の配備計画を確認（ドライラン）
mise run link:check   # デプロイ前確認（ドライラン）
mise run link         # dist/ をホームへ配備実行
mise run setup        # 初回セットアップ（link + brew bundle + zinit）
mise run test         # テスト実行（= bun test）
```

## デプロイ戦略

`scripts/lib/link-dist.ts` の `planLinkActions` が `dist/` を再帰的に走査し、各ファイルのアクションを決定する。

**アクション種別**:
- `link` — シンボリックリンクを作成（デフォルト）
- `copy` — ファイルをコピー（devcontainer互換性のため）
- `backup` — 既存ファイルを `~/.dotfiles-backups/TIMESTAMP/` に退避してから link/copy
- `noop` — 既に正しい状態、何もしない

**コピー対象** (`COPY_INSTEAD_OF_LINK`):
```ts
".claude/CLAUDE.md"
".claude/settings.json"
```
これら以外はすべてシンボリックリンク。

新たにコピー対象にする場合は `scripts/lib/link-dist.ts` の `COPY_INSTEAD_OF_LINK` に追記する。

## テスト

```bash
bun test                    # 全テスト実行
bun test tests/foo.test.ts  # 特定ファイルのみ
```

テストファイル一覧:
- `tests/bootstrap.test.ts` — install.sh の動作
- `tests/link-dist.test.ts` — リンク配備ロジック（最大）
- `tests/repo-layout.test.ts` — リポジトリ構成の期待値
- `tests/shell-config.test.ts` — zsh設定の期待値
- `tests/setup.test.ts` — セットアップロジック

変更後は必ず `mise run test` で回帰確認すること。

## 重要ファイル

| ファイル | 用途 | 注意点 |
|---------|------|--------|
| `dist/.claude/CLAUDE.md` | AI Agentルール | デプロイ先にコピーされる |
| `dist/.claude/settings.json` | Claude Code設定 | デプロイ先にコピーされる |
| `dist/.config/mise/config.toml` | ツール管理（atuin, bat, eza等） | ツール追加時に編集 |
| `dist/.config/git/hooks/pre-commit` | gitleaks + local hooks チェーン | |
| `scripts/lib/link-dist.ts` | デプロイエンジン | `COPY_INSTEAD_OF_LINK` を管理 |
| `AGENTS.md` | AI Agent向け運用ルール | dist/ に秘密情報を入れない等 |

## セキュリティ

- `dist/` にシークレットを置かない（gitignoreで除外: `.zsh.d/local.zsh`, `.zsh.d/secrets.zsh`）
- pre-commit で gitleaks が staged ファイルをスキャン
- CI（GitHub Actions）でも gitleaks が実行される
