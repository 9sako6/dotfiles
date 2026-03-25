# dotfiles

## セットアップ

```sh
sh -c "$(curl -fsSL https://raw.githubusercontent.com/9sako6/dotfiles/master/install.sh)"
```

既存ファイルがある場合は `~/.dotfiles-backups/` に退避します。

## 運用

```sh
mise install
mise run link:check
mise run link
mise run test
```

## 編集

```sh
$EDITOR dist/.zshrc
mise run link
```
