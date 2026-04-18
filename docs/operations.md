# 運用ガイド

この文書は、この repo をどう安全に使うかをまとめた運用ガイドです。まず壊さない確認をし、そのあと反映する順序を基本にします。

管理対象の境界は [docs/repo-map.md](repo-map.md) の「管理境界」を正本とします。

この repo は macOS 前提です。

## 初回セットアップ

```sh
curl -fsSL dot.9sako6.com | bash
```

- `install.sh` は bootstrap 専用です。
- clone 後に、必要なら Homebrew を導入し、その後 `mise trust`、repo 実行用の `mise install`、`mise run install:user`、`mise run agents:build`、`mise run apply` を順に呼びます。
- 各 task は 1 つの責務だけを持ちます。`install:user` は Brewfile、dist の mise 設定、zinit を含む user tool 導入、`apply` は `dist/` の反映です。
- `mise run agents:build` は `apm` 導入後に実行され、生成された agents 用ファイルは最後の `mise run apply` で home directory へ反映されます。
- 既存ファイルは `~/.dotfiles-backups/` に退避されます。

## 日常コマンド

```sh
mise run plan
mise run apply
mise run dev:test
```

- `mise run plan`
  - filesystem を変更せず、配備計画を一覧で確認します。
- `mise run apply`
  - `dist/` の内容を home directory に反映します。
- `mise run dev:test`
  - repo の契約テストをまとめて実行します。

## ツール管理

```sh
mise run install:user
mise run install:user:zinit
mise run agents:build
```

- `mise run install:user`
  - Brewfile、dist/.config/mise/config.toml、zinit をまとめて導入します。
- `mise run install:user:zinit`
  - zinit が未導入のときだけ導入します。
- `mise run agents:build`
  - apm で agents 用リソースをビルドします。

## Bootstrap 原則

- `install.sh` は bootstrap に徹する。
  - 外部前提の導入と task の実行順制御だけを担当し、repo 内の複数責務を 1 つの `setup` に隠さない。
- `.mise.toml` の task は 1 task 1責務に保つ。
  - 依存関係がある処理でも、`install:user`、`install:user:zinit`、`agents:build`、`apply` のように観測可能な単位へ分ける。
- 標準コマンドで足りる処理は shell で書く。
  - `brew`、`mise`、`git` などの既存コマンドを薄い TypeScript ラッパーで包まない。
- TypeScript を使うのは repo 固有のロジックがあるときだけにする。
  - たとえばリンク計画や差分計算のように、この repo 自身の振る舞いを実装するときに限る。
- bootstrap の正しさは e2e で確認する。
  - shell の順序や導線の確認を、内部手順を固定する unit test に逃がさない。

## 変更前後の基本手順

1. まず `mise run plan` で確認する
2. 必要な変更を入れる
3. `mise run dev:test` で回帰を確認する
4. 最後に `mise run apply` で反映する

## バージョンピン留めルール

**すべての依存関係・ツールのバージョンは厳密にピン留めすること。範囲指定・曖昧指定は禁止。**

### mise（`.mise.toml`・`dist/.config/mise/config.toml`）

`major.minor.patch` 形式で指定する。

```toml
# 良い例
bun = "1.3.12"
node = "24.14.1"

# 禁止
bun = "latest"
bun = "1.3"
bun = "1"
```

repo の実行入口は `.mise.toml` の task と Bun script に寄せる。

### GitHub Actions

commit SHA で固定する（semver タグより厳格で immutable）。

```yaml
# 良い例（commit SHA + バージョンコメント）
uses: actions/checkout@34e114876b0b11c390a56381ad16ebd13914f8d5 # v4.3.1

# 禁止
uses: actions/checkout@v4
uses: actions/checkout@v4.2.2
uses: actions/checkout@main
```

### Homebrew（`.Brewfile`）

Homebrew は Brewfile でのバージョン指定をサポートしていないため例外。バージョン管理が必要なツールは Brewfile ではなく mise で管理する。

## 守ること

- `dist/` には共有してよい設定だけを置きます。
- 秘密情報、個人トークン、マシン固有値は `dist/` に入れません。
- ローカル専用設定は `~/.zsh.d/local.zsh` と `~/.zsh.d/secrets.zsh` に置きます。
- `tmp/` は一時ファイル置き場です。長く残したい内容だけ `docs/` に移します。
