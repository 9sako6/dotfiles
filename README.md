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

## 運用

dotfiles リポジトリをクローンした後は、[mise](https://github.com/jdx/mise) タスクで管理します。

```sh
# miseのインストール（初回のみ）
mise install

# 日常的な操作
mise run status    # 未適用の変更を確認
mise run diff      # 変更内容の詳細を確認
mise run apply     # 変更を適用（ホームディレクトリに反映）

# ファイル編集
mise run edit ~/.zshrc  # chezmoi経由で編集（推奨）
# または直接編集: ~/dotfiles/home/dot_zshrc を編集 → mise run apply

# リモートとの同期
mise run update    # リモートから更新を取得して適用
mise run doctor    # 問題をチェック
```

**Tips:**
- `mise run apply` は verbose モードで確実に変更を反映します
- 直接 `~/dotfiles/home/dot_*` を編集した後も `mise run apply` で反映できます
- `mise run status` で未適用の変更を確認できます

<details>
<summary>生の chezmoi コマンドを使いたい場合</summary>

```sh
chezmoi -S ~/dotfiles/home diff
chezmoi -S ~/dotfiles/home apply -v
chezmoi -S ~/dotfiles/home edit ~/.zshrc
chezmoi -S ~/dotfiles/home status
chezmoi -S ~/dotfiles/home update
```

</details>
