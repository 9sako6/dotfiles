# Nix 設定

このディレクトリでは、Nix で宣言する macOS の system 設定とユーザー環境をまとめて管理する。
実現手段の名前ではなく責務でファイルを分ける。初回セットアップや詳しい挙動は [運用ガイド](../docs/operations.md) を参照する。

## ファイル

- `default.nix`: root flake から読み込む入口。system 設定と Home Manager の接続を組み立てる
- `flake.nix.template`: private repository の root flake のひな型
- `home.nix`: Home Manager で管理する home 配置と user toolset の適用
- `homebrew-packages.nix`: Nix では合理的に管理しない Homebrew formula / cask
- `homebrew-shellenv.zsh`: nix-homebrew が管理する Homebrew を zsh から使うための設定
- `packages.nix`: ユーザー配備と CI/bootstrap が共有する Nix package と期待バージョンの正本
- `system.nix`: macOS の既定値、サービス、Nix、Homebrew など system scope の設定

公開 flake は repository root の `flake.nix` / `flake.lock` にある。共有設定ファイルの実体は `home/` に置く。
マシン固有の設定や認証情報は `nix/` に置かない。

## ユーザー常設ツール

普通のCLIやtoolchainは[パッケージと環境の原則](../docs/repo-map.md#パッケージと環境の原則)に従い、miseへ段階的に移行する。`packages.nix` はNix固有artifactと未移行のツールを管理する。現在はGit、Quintがここに残っている。

`packages.nix` は Home Manager 専用 module ではなく、共有 toolset を返す。`home.nix` はユーザー向けの `packages` を `home.packages` に適用する。CI と初回 bootstrap が必要とする Bun・Rust・Git は `ciPackages` として分離し、root flake の `.#ciTools` から提供する。これにより、普通の CLI をユーザー配備から外しても CI と bootstrap のビルド経路を保つ。

Nix package は `flake.lock` だけにバージョン管理を委ねず、期待バージョンを `packages.nix` に明示して assertion する。
versioned attribute がある場合はそれを使い、コメントにも完全なバージョンを残す。

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
