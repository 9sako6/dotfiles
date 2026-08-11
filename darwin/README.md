# macOS のシステム設定

このディレクトリでは、nix-darwin で Mac 全体へ反映する公開設定を管理する。
初回セットアップや詳しい挙動は [運用ガイド](../docs/operations.md) を参照する。

## ファイル

- `configuration.nix`: macOS の既定値、サービス、Nix、システムパッケージ
- `flake.nix.template`: private repository の root flake のひな型
- `homebrew-packages.nix`: 公開してよい Homebrew の formula と cask
- `homebrew-shellenv.zsh`: nix-homebrew が管理する Homebrew を zsh から使うための設定

公開 flake は repository root の `flake.nix` / `flake.lock` にある。
マシン固有の設定や認証情報はここへ置かない。Mac 全体へ反映したいが公開できない設定は、
別の private repository で管理する。

## 公開設定を変更する

1. 対象の Nix ファイルを編集する。
2. `dotfiles plan --default` で、Mac へ反映せずに build と差分を確認する。
3. `dotfiles test` を実行する。
4. `dotfiles apply --default` で反映する。

`apply` は必要に応じて Lix を導入し、`sudo` の認証を求める。
設定の反映には `darwin-rebuild` を直接実行せず、`dotfiles apply` を使う。

## private 設定を作る

private repository の root に、この repository の `darwin/flake.nix.template` を
`flake.nix` としてコピーする。次に `primaryUser` を実際の macOS account name に置き換え、
公開できない差分だけを `modules` に追加する。

```nix
modules = [
  {
    homebrew.casks = [
      "private-app"
    ];
  }
];
```

共有設定は private repository に複製しない。private root flake から、公開側の
`darwinModules.default` と `lib.mkDarwinSystem` を利用する。

設定後は private repository で lock file を生成し、`flake.nix` と `flake.lock` を commit、push する。
実行時には remote の既定 branch にある最新の push 済み commit を取得する。local の未 push 変更は使えない。

```sh
nix flake lock
git add flake.nix flake.lock
git commit
git push
```

初回は credential を含まない SSH または HTTPS clone URL を指定する。

```sh
dotfiles plan git@example.com/owner/private-dotfiles.git
dotfiles apply git@example.com/owner/private-dotfiles.git
```

`plan` は build と差分表示だけを行い、使用する source を変更しない。
`apply` が成功すると private source が選択される。以後は URL を省略できる。

```sh
dotfiles plan
dotfiles apply
```

公開設定へ戻す場合は `--default` を指定する。

## Homebrew の formula や cask を追加する

複数の Mac で共有してよいものは `homebrew-packages.nix` に追加する。CLI は `brews`、
GUI アプリは `casks` に入れ、それぞれアルファベット順を保つ。

```nix
{
  brews = [
    "example-cli"
  ];
  casks = [
    "example-app"
  ];
}
```

公開できないものは private repository の `modules` に追加する。追加後は lock file を必要に応じて更新し、
commit、push してから `dotfiles plan` と `dotfiles apply` を実行する。

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin が管理する。
バージョンを固定したい開発 CLI は Homebrew ではなく、mise の `[tools]` または Nix で管理する。

## 公開側の依存を更新する

`nix-darwin`、`nix-homebrew`、`nixpkgs`、`zundamonotify` の具体的な revision は
root の `flake.lock` で固定する。通常は `flake.nix` に commit SHA を書かない。

```sh
mise run system:update
```

この task は root の `flake.lock` を更新したあと、公開構成の `dotfiles plan --default` と `dotfiles test` を実行する。
commit、push、apply は行わないため、差分を確認してから明示的に実行する。

private repository など、この公開設定へ依存する flake の更新は依存側で管理する。
public repository 側は依存先の repository 名、配置、更新方法を持たない。

`flake.lock` は手で編集しない。

## ロールバック

反映後に問題が起きたら、保持済みの直前の世代へ戻す。

```sh
mise run system:rollback
```

古い世代の削除条件は [運用ガイドのロールバック](../docs/operations.md#ロールバック) を参照する。
