# 運用ガイド

この文書は、この repo をどう安全に使うかをまとめた運用ガイドです。まず壊さない確認をし、そのあと反映する順序を基本にします。

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

## ツールバージョン管理

`.mise.toml` と `dist/.config/mise/config.toml` に記載する全ツールのバージョンは `major.minor.patch` 形式でピン留めすること。

```toml
# 良い例
bun = "1.3.12"
node = "24.14.1"

# 禁止
bun = "latest"
bun = "1.3"
bun = "1"
```

バージョンを更新する際は `mise ls-remote <tool>` で最新を確認し、フルバージョンで指定する。

## 守ること

- `dist/` には共有してよい設定だけを置きます。
- 秘密情報、個人トークン、マシン固有値は `dist/` に入れません。
- ローカル専用設定は `~/.zsh.d/local.zsh` と `~/.zsh.d/secrets.zsh` に置きます。
- `tmp/` は一時ファイル置き場です。長く残したい内容だけ `docs/` に移します。
