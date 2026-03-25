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

## ローカル設定と秘密情報

`dist/` には共有してよい設定だけを置きます。マシン固有の設定や秘密情報は `dist/` に入れず、未追跡の `~/.zsh.d/local.zsh` と `~/.zsh.d/secrets.zsh` に置きます。

`dist/.zsh.d/local.zsh` と `dist/.zsh.d/secrets.zsh` は誤って追加しないように `.gitignore` で除外します。
