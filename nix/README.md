# Nix 設定

このディレクトリでは、Nix で宣言する macOS の system 設定とユーザー環境をまとめて管理する。
実現手段の名前ではなく責務でファイルを分ける。初回セットアップや詳しい挙動は [運用ガイド](../docs/operations.md) を参照する。

## ファイル

- `default.nix`: root flake から読み込む入口。system 設定と Home Manager の接続を組み立てる
- `flake.nix.template`: private repository の root flake のひな型
- `home.nix`: Home Manager で管理する home 配置と user toolset の適用
- `homebrew-packages.nix`: Nix では合理的に管理しない Homebrew formula / cask
- `homebrew-shellenv.zsh`: nix-homebrew が管理する Homebrew を zsh から使うための設定
- `packages.nix`: Nix 固有 artifact と期待バージョンの正本
- `system.nix`: macOS の既定値、サービス、Nix、Homebrew など system scope の設定

公開 flake は repository root の `flake.nix` / `flake.lock` にある。共有設定ファイルの実体は `home/` に置く。
マシン固有の設定や認証情報は `nix/` に置かない。

## ユーザー常設ツール

普通のCLIやtoolchainは[管理境界](../docs/repo-map.md#管理境界)に従い、miseへ移行する。`packages.nix` はNix固有artifact（AnkiConnect、Nightlight）だけを管理し、通常のCLIは持たない。

`packages.nix` は Home Manager 専用 module ではなく、Nix固有artifactを返す共有 toolset。Bun・Git など通常の CLI は `home/.config/mise/config.toml` を正本とし、CI と Bootstrap も同じ設定を参照する。そのため Nix に CI 専用 package は持たない。

Nix package は `flake.lock` だけにバージョン管理を委ねず、期待バージョンを `packages.nix` に明示して assertion する。

miseへ移したツールは `home/.config/mise/config.toml` に宣言する。root `.mise.toml` はrepo runtimeのタスクを管理し、`[tools]` を持たない。

## Homebrew

複数の Mac で共有してよい formula / cask は `homebrew-packages.nix` に追加する。CLI は `brews`、GUI アプリは `casks` に入れ、それぞれアルファベット順を保つ。
公開できないものは private repository の `modules` に追加する。

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin が管理する。ユーザー常設 CLI は Nix を優先し、Homebrew は Nix が合理的でない場合の例外とする。

## private 設定

非公開のシステム設定は、公開ルートから`dotfiles.local.toml`を通じて取り込みます。指定先の要件と設定例はCLIの[private.path](../cli/README.md#privatepath)を参照してください。

## 反映と更新

システムの確認と反映はCLIの[plan](../cli/README.md#plan)と[apply](../cli/README.md#apply)を参照してください。

`nix-darwin`、`nix-homebrew`、`nixpkgs`、`zundamonotify` の具体的な revision は root の `flake.lock` で固定する。`flake.lock` は手で編集しない。
