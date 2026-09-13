# Nix 設定

このディレクトリでは、Nix で宣言する macOS の system 設定とユーザー環境をまとめて管理する。
実現手段の名前ではなく責務でファイルを分ける。初回セットアップや詳しい挙動は [運用ガイド](../docs/operations.md) を参照する。

## ファイル

- `default.nix`: root flake から読み込む入口。system 設定と Home Manager の接続を組み立てる
- `flake.nix.template`: private repository の root flake のひな型
- `home.nix`: Home Manager で管理する home 配置と user toolset の適用
- `homebrew-packages.nix`: Nix では合理的に管理しない Homebrew formula / cask
- `homebrew-shellenv.zsh`: nix-homebrew が管理する Homebrew を zsh から使うための設定
- `packages.nix`: ユーザーが常設する Nix package と期待バージョンの正本
- `system.nix`: macOS の既定値、サービス、Nix、Homebrew など system scope の設定

公開 flake は repository root の `flake.nix` / `flake.lock` にある。共有設定ファイルの実体は `home/` に置く。
マシン固有の設定や認証情報は `nix/` に置かない。

## ユーザー常設ツール

ユーザーとして常に使える状態にしたい CLI や toolchain は、原則 `packages.nix` で管理する。
現在は Bun、Git、Go、Quint、Rust toolchain をここで管理する。

`packages.nix` は Home Manager 専用 module ではなく、共有 toolset を返す。`home.nix` はその package list を `home.packages` に適用し、root flake は同じ list を `.#userTools` として公開する。CI もこの `.#userTools` を使うため、ユーザー環境と CI に別々のバージョン定義を持たない。

Nix package は `flake.lock` だけにバージョン管理を委ねず、期待バージョンを `packages.nix` に明示して assertion する。
versioned attribute がある場合はそれを使い、コメントにも完全なバージョンを残す。

Nix で合理的に管理できないものだけ例外とし、Homebrew、mise、公式 installer など自然な方法を選ぶ。例外理由は設定やコメントに残す。現時点では root `.mise.toml` に tool 例外はなく、`[tools]` を持たない。

## Homebrew

複数の Mac で共有してよい formula / cask は `homebrew-packages.nix` に追加する。CLI は `brews`、GUI アプリは `casks` に入れ、それぞれアルファベット順を保つ。
公開できないものは private repository の `modules` に追加する。

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin が管理する。ユーザー常設 CLI は Nix を優先し、Homebrew は Nix が合理的でない場合の例外とする。

## private 設定

非公開のシステム設定は、公開ルートから`dotfiles.local.toml`を通じて取り込みます。指定先の要件と設定例はCLIの[private.path](../cli/README.md#privatepath)、旧構成からの移行は[移行手順](../docs/operations.md#旧-private-root-からの移行手順)を参照してください。

## 反映と更新

システムの確認と反映はCLIの[plan](../cli/README.md#plan)と[apply](../cli/README.md#apply)を参照してください。

`nix-darwin`、`nix-homebrew`、`nixpkgs`、`zundamonotify` の具体的な revision は root の `flake.lock` で固定する。`flake.lock` は手で編集しない。
