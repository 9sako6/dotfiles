# dotfiles

## セットアップ

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/master/install.sh)"
```

既存ファイルがある場合は `~/.dotfiles-backups/` に退避します。

## 運用

```sh
mise run link:check  # 反映前に確認
mise run link        # 反映
mise run test        # テスト
```
