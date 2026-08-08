# macOS のシステム設定

このディレクトリでは、root flakeから読み込むnix-darwin moduleとHome Manager moduleを管理する。
初回セットアップや詳しい挙動は [運用ガイド](../docs/operations.md) を参照する。

## ファイル

- `../flake.lock`: root `flake.nix` から生成する依存のロック
- `../flake.nix`: nix-darwin / Home Manager が使う依存と公開出力
- `configuration.nix`: macOS の既定値、サービス、Nix、システムパッケージ
- `flake.nix.template`: private repository の root flake のひな型
- `home-manager.nix`: nix-darwin と Home Manager の統合
- `home.nix`: `home/` から home directory へ配備する Home Manager 設定
- `homebrew-packages.nix`: 公開してよい Homebrew の formula と cask
- `homebrew-shellenv.zsh`: nix-homebrew が管理する Homebrew を zsh から使うための設定

マシン固有の設定や認証情報はここへ置かない。Mac 全体へ反映したいが公開できない設定は、
別の private repository で管理する。

## Home Manager

Home Manager は nix-darwin module として組み込む。通常の設定ファイルと `.config`、`.zsh.d`、`mybin` は `mkOutOfStoreSymlink` でlive dotfiles checkoutを参照し、編集を即時反映する。

APMが生成する `.agents`、`.claude`、`.codex` はNix store由来のleaf linkにして、agentが管理対象ファイルへ書き込んでもsource repositoryへ逆流しないようにする。どちらの方式でもディレクトリ自体は占有せず、local-only、secrets、runtime fileと共存できる。

home の反映も `plan` / `apply` に含まれる。これは `system:plan` / `system:apply` のaliasであり、standaloneのhome deployerは使わない。

## 公開設定を変更する

1. 対象の Nix ファイルまたは `home/` を編集する。
2. `mise run plan --default` で、Mac へ反映せずに build と差分を確認する。
3. `mise run test` を実行する。
4. `mise run apply --default` で反映する。
5. `mise run doctor` で未反映の設定がないことを確認する。

`apply` は必要に応じて Lix を導入し、`sudo` の認証を求める。
設定の反映には `darwin-rebuild` を直接実行せず、この task を使う。

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
mise run plan git@example.com/owner/private-dotfiles.git
mise run apply git@example.com/owner/private-dotfiles.git
```

`plan` は build と差分表示だけを行い、使用する source を変更しない。
`apply` が成功すると private source が選択される。以後は URL を省略できる。

```sh
mise run plan
mise run apply
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
commit、push してから `plan` と `apply` を実行する。

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin が管理する。
バージョンを固定したい開発 CLI は Homebrew ではなく、mise の `[tools]` または Nix で管理する。

## 公開側の依存を更新する

`home-manager`、`nix-darwin`、`nix-homebrew`、`nixpkgs`、`zundamonotify` の具体的な revision は
root `flake.lock` で固定する。通常は `flake.nix` に commit SHA を書かない。

```sh
mise run system:update
```

この task は root `flake.lock` を更新したあと、公開構成の `plan --default` と test suite を実行する。
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
