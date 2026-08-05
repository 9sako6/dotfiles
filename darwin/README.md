# macOS のシステム設定

このディレクトリでは、nix-darwin で Mac 全体へ反映する設定を管理する。
初回セットアップや `home/` の配備手順は [運用ガイド](../docs/operations.md) を参照する。

## ファイル

- `configuration.nix`: macOS の既定値、サービス、Nix、システムパッケージ
- `flake.lock`: `flake.nix` から生成する依存のロック
- `flake.nix`: nix-darwin が使う依存と構成
- `homebrew-packages.nix`: Homebrew の formula と cask
- `homebrew-shellenv.zsh`: nix-homebrew が管理する Homebrew を zsh から使うための設定

パッケージの追加と削除は `homebrew-packages.nix` で行い、各リストはアルファベット順に保つ。
マシン固有の設定や認証情報はここへ置かない。

## いつもの変更手順

1. `dot pull` で `origin/master` の変更を取り込む。
2. 対象の Nix ファイルを編集する。
3. `./bin/build-system.sh` で、Mac へ反映せずに構成をビルドする。
4. `mise run test` を実行する。
5. `mise run install:system` で Mac へ反映する。
6. `dot doctor` で未反映の設定がないことを確認する。

`install:system` は必要に応じて `sudo` の認証を求める。設定の反映には `darwin-rebuild` を直接実行せず、このコマンドを使う。

## 依存の更新

`nix-darwin`、`nix-homebrew`、`nixpkgs`、`zundamonotify` を更新するときは、`flake.nix` の参照先を commit SHA で指定し直してからロックを更新する。

```sh
nix flake lock ./darwin
./bin/build-system.sh
mise run test
mise run install:system
```

`flake.lock` は手で編集しない。更新後は `flake.nix` と `flake.lock` の差分を一緒に確認する。

## ロールバック

反映後に問題が起きたら、保持済みの世代を確認して直前の世代へ戻す。

```sh
mise run install:system:generations
mise run install:system:rollback
```

古い世代の削除条件や、特定の世代へ戻す方法は [運用ガイドのロールバック](../docs/operations.md#ロールバック) にまとめている。
