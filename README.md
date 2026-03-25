# dotfiles

zsh, git, tmux, and helper scripts.

## セットアップ

買いたての macOS でも `sh` 1 発でセットアップを始められます。

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/master/install.sh)"
```

このスクリプトは次を行います。

1. `~/dotfiles` にリポジトリを clone
2. `mise` を `~/.local/bin/mise` にインストール
3. `mise install` で必要なツールを導入
4. `dist/` 配下を `~` にシンボリックリンクで配置
5. 追加セットアップとして zinit をインストール

既存ファイルや既存ディレクトリがある場合は、上書きせずに `~/.dotfiles-backups/<timestamp>/` へ退避してから新しい symlink を張ります。

## 運用

配布元は `dist/` です。`dist/.zshrc` は `~/.zshrc` に、`dist/mybin/timer` は `~/mybin/timer` に対応します。

普段の操作は [mise](https://mise.jdx.dev/) タスクで行います。

```sh
# 初回のみ
mise install

# dry-run
mise run link:check

# 実際に反映
mise run link

# テスト
mise run test

# 現在のリンク計画を確認
mise run doctor
```

## 配置ルール

- `dist/` のパスがそのまま `~` 配下へ対応する
- 配布物は symlink で配置する
- `dist/mybin` や `dist/.zsh.local` のようなディレクトリは、原則そのディレクトリごと symlink にする
- `dist/.config` は共存しやすいようにコンテナとして扱い、その直下の管理対象ディレクトリ単位で symlink にする
- 既存の通常ファイル、通常ディレクトリ、別ターゲットの symlink は `~/.dotfiles-backups/<timestamp>/` に退避する
- すでに同じターゲットを指す symlink は変更しない

## 編集

`dist/` を直接編集して、`mise run link` で反映する。

例:

```sh
$EDITOR dist/.zshrc
mise run link
```
