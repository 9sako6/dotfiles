# リポジトリ構成

このリポジトリは、`dist/` に置いた共有可能な dotfiles を home directory に安全に反映するためのものです。初回導線は `install.sh`、日常運用は `mise` の task と Bun の script が担当します。

## 主要ディレクトリ

- `dist/`
  - 実際に home directory へ配備する設定や補助コマンドを置きます。
  - `dist/.Brewfile` は macOS アプリ管理用です。
  - `dist/mybin/` は配備される自作コマンド置き場です。
- `scripts/`
  - `mise` task から呼ばれる TypeScript の入口です。
  - `scripts/setup.ts` は初回セットアップ、`scripts/link-dist.ts` は `dist/` の反映を担当します。
- `scripts/lib/`
  - リンク計画、バックアップ、追加セットアップなどの実処理です。
- `tests/`
  - repo の契約テストです。
  - bootstrap、配備計画、repo layout、shell config の期待値を固定しています。
- `docs/`
  - 長く残す価値のある恒久的なドキュメントを置きます。
- `tmp/`
  - 実装計画、設計メモ、一時作業メモを置く場所です。永続化前提ではありません。

## 入口と責務

- `install.sh`
  - ワンコマンド導線専用です。
  - clone、`mise` の導入、`mise run setup` の起動までを担当します。
- `.mise.toml`
  - この repo で必要な最小限の tool と task を定義します。
  - repo 実行の標準入口です。task は Bun の TypeScript script を直接起動します。

## 管理境界

この repo では、ファイルを次の 4 区分に分けて扱います。

- `repo runtime`
  - この repo 自体を開発・検証・保守するためのファイルです。
  - 例: `.mise.toml`, `scripts/`, `tests/`, `tsconfig.json`, `.github/`
  - home directory へは配備しません。
- `dist-managed user tools`
  - home directory へ配備して、日常利用する設定や補助コマンドです。
  - 例: `dist/.zshrc`, `dist/.config/mise/config.toml`, `dist/mybin/`
  - 共有してよい内容だけを入れます。
- `local-only`
  - マシン固有で、repo には入れないローカル専用設定です。
  - 例: `~/.zsh.d/local.zsh`
  - 共有しない前提で各マシンに手で置きます。
- `secrets`
  - 認証情報、token、鍵、個人情報などの機密です。
  - repo と `dist/` のどちらにも入れません。
  - 推奨置き場の例として `~/.zsh.d/secrets.zsh` を使えますが、最終判断はユーザーに委ねます。

### 置き場所の判断ルール

- repo を動かすためだけに必要なら `repo runtime` に置きます。
- home directory に配備して複数マシンで共有したいなら `dist-managed user tools` に置きます。
- マシン固有なら `local-only` に置きます。
- 機密は repo と `dist/` に入れず、置き場所の最終判断はユーザーが行います。

## 変更時の目安

- dotfiles 自体を変えるなら `dist/` を触ります。
- 反映ロジックを変えるなら `scripts/` と `tests/` を一緒に見ます。
- 導線や運用説明を変えるなら `README.md` と `docs/` を触ります。

## 安全境界

- `dist/` には共有してよい設定だけを入れます。
- マシン固有設定は `~/.zsh.d/local.zsh` に置きます。
- 秘密情報は repo と `dist/` に入れません。`~/.zsh.d/secrets.zsh` は推奨置き場の例です。
