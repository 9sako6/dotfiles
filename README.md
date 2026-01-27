# dotfiles

zsh, git settings.

## セットアップ

`curl` だけで chezmoi をインストールし、`home/` を配布元として適用します。

```sh
sh -c "$(curl -fsLS get.chezmoi.io)" -- init 9sako6
chezmoi -S ~/.local/share/chezmoi/home apply
```

- `~/.local/bin/chezmoi` にインストールされます。
- 既存ファイルは chezmoi 側の挙動に従って退避/上書きされます。

## 運用（chezmoi）

```sh
chezmoi -S ~/dotfiles/home diff
chezmoi -S ~/dotfiles/home apply
chezmoi -S ~/dotfiles/home edit ~/.zshrc
chezmoi -S ~/dotfiles/home update
```

## バージョン管理・タスク（mise）

必要な言語/コマンドとタスクは [mise](https://github.com/jdx/mise) を使います。

```sh
mise install
mise run diff
mise run apply
mise run update
mise run doctor
```
