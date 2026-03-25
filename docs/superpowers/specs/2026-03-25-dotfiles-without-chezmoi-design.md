# Dotfiles Without Chezmoi Design

**Date:** 2026-03-25

## Goal

`chezmoi` 管理をやめて、`dist/` 配下を配布元とするシンプルな dotfiles リポジトリに戻す。買いたての macOS 環境で 1 コマンド実行するだけで、必要なランタイム導入、設定ファイルの配置、追加セットアップまで完了できる状態を作る。

## Background

現在のリポジトリは `home/` 配下に `chezmoi` 形式のファイルを持ち、`.mise.toml` と `README.md` も `chezmoi` 前提の運用になっている。`chezmoi` 導入以前の履歴には、`dist/` を配布元としてホームディレクトリへシンボリックリンクを張る TypeScript 実装が存在していた。

今回の目的は、その以前のシンプルな配布モデルへ戻しつつ、以下を満たすこと。

- `dist/` の見た目をそのまま `~` 配下へ対応づけられる
- 既存ファイルやディレクトリがあっても安全に退避できる
- 初回セットアップが `sh` だけで開始できる
- 実装本体とテストは TypeScript で管理できる
- `mise` を中心にランタイムと日常タスクを管理できる

## Non-Goals

- マルチユーザー向けの汎用 dotfiles 配布
- `chezmoi` のテンプレート機能や secret 管理の置き換え
- ホームディレクトリ外への一般化されたデプロイ
- 既存ディレクトリ内容の賢いマージ

## Requirements

### Functional

1. `dist/` 配下のパスは、そのまま `~` 配下へ 1:1 で対応づける
2. 各配布対象はシンボリックリンクとして配置する
3. 既存の通常ファイル、通常ディレクトリ、別ターゲットを指すシンボリックリンクは安全に退避してから張り直す
4. 同じターゲットを指す既存シンボリックリンクは変更しない
5. 退避先は `~/.dotfiles-backups/<timestamp>/...` とし、元の相対パスを保持する
6. 初回セットアップは `sh` で起動できる入口スクリプトを提供する
7. 追加セットアップ処理も初回フローに含める
8. `mise` タスクで日常運用と検証を実行できる
9. 既存の `home/dot_*` 構成から `dist/` への移行を行う

### Operational

1. ランタイムは `mise` で導入する
2. 実装言語は TypeScript、実行環境は `bun` とする
3. テストはローカルで繰り返し実行しやすいこと
4. `README.md` には新しいセットアップ手順と運用手順を記載する

## Proposed Architecture

### Repository Layout

移行後の構成は次を想定する。

```text
dist/
  .zshrc
  .zshenv
  .gitconfig
  .config/...
  mybin/...
scripts/
  setup.ts
  link-dist.ts
  lib/
    backup.ts
    fs-plan.ts
    linker.ts
    paths.ts
tests/
  link-dist.test.ts
  setup.test.ts
install.sh
.mise.toml
README.md
```

`dist/` が唯一の配布元であり、`home/` と `chezmoi` 特有の命名は廃止する。

### Setup Flow

`install.sh` は買いたての macOS でも動く最小の入口とする。

1. `~/dotfiles` がなければ GitHub から clone する
2. `mise` 未導入なら公式インストールスクリプトで導入する
3. `mise install` を実行して `bun` など必要なツールを導入する
4. `bun run scripts/setup.ts` を実行する

`scripts/setup.ts` はオーケストレーションに専念し、以下を順に実行する。

1. 前提チェック
2. `dist/` から `~` へのリンク処理
3. 追加セットアップ処理
4. 完了サマリ表示

### Linking Model

リンク処理は `scripts/link-dist.ts` が担当する。内部では `dist/` を再帰走査して、各ソースパスをホーム配下の対応する絶対パスへ変換する。

各エントリに対する動作は次のとおり。

- 配置先が存在しない
  - 親ディレクトリを必要に応じて作成し、symlink を作成する
- 配置先が symlink で、同じターゲットを指す
  - no-op
- 配置先が symlink で、別ターゲットを指す
  - 退避してから新しい symlink を作成する
- 配置先が通常ファイルまたは通常ディレクトリ
  - 退避してから symlink を作成する

今回の設計では、既存ディレクトリの中身マージは行わない。既存ディレクトリがある場合は丸ごと退避し、`dist/` 側ディレクトリへの symlink を置く。source of truth を `dist/` に一本化し、ホーム側でのドリフトを避けるためである。

### Backup Model

退避は毎回一意なタイムスタンプ単位でまとめる。

- ベースディレクトリ: `~/.dotfiles-backups/<timestamp>/`
- 退避例: `~/.zshrc` は `~/.dotfiles-backups/20260325T123456/.zshrc`
- `~/.config/nvim` は `~/.dotfiles-backups/20260325T123456/.config/nvim`

これにより、復旧時に元のパス構造を見失わない。

### Tooling and Tasks

`.mise.toml` は少なくとも次の責務を持つ。

- `bun` の導入
- `mise run link`: 実際のリンク処理
- `mise run link:check`: dry-run
- `mise run test`: `bun test`
- `mise run doctor`: `dist/` やリンク状態の検査
- 必要なら `mise run setup`: フルセットアップ

## Migration Plan

既存リポジトリからの移行では、次を行う。

1. `home/dot_*` と `home/exact_*` を `dist/` 構成へ移す
2. `chezmoi` 依存の `README.md` 記述を置き換える
3. `.mise.toml` の `chezmoi` タスクを廃止し、新タスクに差し替える
4. 必要なら一回限りの変換スクリプトを作り、移行作業を機械化する
5. 不要になった `chezmoi` 関連ファイルと前提を削除する

## Testing Strategy

テストは `bun test` で実行するファイルシステムテストを中心に組む。一時ディレクトリを使い、以下のケースを必須とする。

1. 配置先が存在しない新規ファイルのリンク作成
2. 既存ファイルの退避とリンク置換
3. 既存ディレクトリの退避とリンク置換
4. 同一ターゲット symlink の no-op
5. 別ターゲット symlink の退避と張り直し
6. ネストした親ディレクトリの作成
7. dry-run で実ファイルが変更されないこと
8. バックアップ先に元の相対パスが保持されること

初回セットアップ全体は、必要に応じて `install.sh` を直接叩く統合寄りの検証も追加する。ただし外部ダウンロードを伴う部分は単体テスト対象から切り離し、TypeScript 側では副作用の少ない形でテストできるように設計する。

## Error Handling

- `HOME` が取得できない場合は即時エラー
- `dist/` が存在しない場合は即時エラー
- 退避やリンク作成に失敗した場合は対象パスを含むエラーメッセージを返す
- dry-run と実行モードは明示的に分け、誤って変更が入らないようにする

## Risks and Mitigations

- 既存ディレクトリを丸ごと退避するため、初回適用時の変更量が大きい
  - dry-run と backup path の明示で確認可能にする
- `install.sh` はネットワークに依存する
  - clone と `mise` 導入だけに責務を絞り、本体ロジックはリポジトリ内に置く
- `dist/` への移行時にパス変換ミスが起こりうる
  - 変換後のツリーとリンク結果をテストで検証する

## Success Criteria

- リポジトリから `chezmoi` 依存が除去されている
- `dist/` の内容が `~` に symlink として安全に反映される
- 既存ファイルは `~/.dotfiles-backups/<timestamp>/` に退避される
- 買いたての macOS で `install.sh` により初回セットアップが完了する
- `mise run test` と dry-run により継続的に安全性を確認できる
