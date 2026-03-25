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

## 管理しているCLIツール

グローバル `mise` 設定で `zoxide`、`atuin`、`delta`、`fd`、`bat`、`eza` を管理します。既存操作を急に置き換えない方針なので、まずは「使えるようにする」だけに留めています。

`atuin` は sync なしのローカル運用で有効化し、既存の `Ctrl-R` 履歴キーバインドはそのまま残します。`delta` は Git pager として使い、`fd`、`bat`、`eza` は alias の強制上書きなしで導入します。

## ローカル設定と秘密情報

`dist/` には共有してよい設定だけを置きます。マシン固有の設定や秘密情報は `dist/` に入れず、未追跡の `~/.zsh.d/local.zsh` と `~/.zsh.d/secrets.zsh` に置きます。

`dist/.zsh.d/local.zsh` と `dist/.zsh.d/secrets.zsh` は誤って追加しないように `.gitignore` で除外します。
