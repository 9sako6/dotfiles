# 運用ガイド

この文書は、この repo をどう安全に使うかをまとめた運用ガイドです。まず壊さない確認をし、そのあと反映する順序を基本にします。

管理対象の境界は [docs/repo-map.md](repo-map.md) の「管理境界」を正本とします。

## 初回セットアップ

```sh
curl -fsSL dot.9sako6.com | bash
```

- `install.sh` は bootstrap 専用です。
- clone 後に `mise trust`、repo 実行用の `mise install`、`mise run setup`、ホームへ反映された `mise` 設定に対する `mise install` を順に呼びます。
- 既存ファイルは `~/.dotfiles-backups/` に退避されます。

## 日常コマンド

```sh
mise run doctor
mise run link:check
mise run link
mise run setup
mise run test
```

- `mise run doctor`
  - 安全な確認用コマンドです。現在の配備計画を見る標準入口として使います。
- `mise run link:check`
  - filesystem を変更せず、バックアップと symlink の計画だけ確認します。
- `mise run link`
  - `dist/` の内容を home directory に反映します。
- `mise run setup`
  - 初回向けです。リンク反映に加えて追加セットアップも行います。
- `mise run test`
  - repo の契約テストをまとめて実行します。

## 変更前後の基本手順

1. まず `mise run doctor` か `mise run link:check` で確認する
2. 必要な変更を入れる
3. `mise run test` で回帰を確認する
4. 最後に `mise run link` で反映する

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
