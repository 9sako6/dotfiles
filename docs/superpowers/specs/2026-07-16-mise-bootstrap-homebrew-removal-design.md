# mise bootstrap へのユーザー環境統合

## 目的

Homebrew 本体、`.Brewfile`、`brew bundle` を廃止し、ユーザー向け CLI、macOS アプリ、Git、zinit の宣言と導入を `dist/.config/mise/config.toml` に集約する。

初回セットアップは引き続き `install.sh` 一つを入口とし、既存の dotfiles 配備が持つバックアップ、prune、置換失敗時の復元契約を維持する。

## 管理境界

- repo runtime はルートの `.mise.toml` が管理する。repo task を実行する Bun と Node だけを `[tools]` に置く。
- dist-managed user tools は `dist/.config/mise/config.toml` が管理する。
- `install.sh` は repo の取得、mise 本体の導入、trust、repo bootstrap の開始だけを担当する。
- Homebrew CLI と `.Brewfile` は管理対象から削除する。

## ユーザー環境の宣言

`dist/.config/mise/config.toml` は次の三種類を管理する。

### 厳密にバージョン固定する CLI

既存の `[tools]` に次を追加する。

| コマンド | mise entry | 移行時のバージョン |
| --- | --- | --- |
| `gh` | `gh` | `2.96.0` |
| `codex` | `codex` | `0.142.5` |
| `devcontainer` | `npm:@devcontainers/cli` | `0.87.0` |
| `herdr` | `github:ogulcancelik/herdr` | `0.7.1` |

バージョンは移行時点で実際に利用中のコマンドから取得した値を使う。各 backend が macOS arm64 で目的の executable を提供することを、適用前の隔離された HOME で検証する。

### mise bootstrap package

厳密なバージョン固定に適した mise tool backend がない項目は、`[bootstrap.packages]` で宣言する。

```toml
[bootstrap.packages]
"brew:git" = "latest"
"brew-cask:ghostty" = "latest"
"brew-cask:raycast" = "latest"
```

これらは mise 内蔵の Homebrew 互換 installer が扱う。Homebrew CLI は導入せず、Git、Ghostty、Raycast に限り `latest` を許容する。

### zinit

zinit は `[bootstrap.repos]` で `~/.local/share/zinit/zinit.git` に配置し、現在と同じ commit `55d19f86f627c9995db9885d0971d9b6701fe0d3` に固定する。

## Bootstrap フロー

`install.sh` は mise を厳密に `2026.7.7` へ固定する。既存の `~/.local/bin/mise` が異なるバージョンなら、同じ公式 installer で固定バージョンへ収束させる。これにより `mise bootstrap` が存在しない旧版を残さない。

その後のフローは次の通りとする。

1. dotfiles repo がなければ clone する。
2. mise `2026.7.7` を必要に応じて導入する。
3. repo の `.mise.toml` を trust する。
4. repo root で `mise bootstrap --yes` を実行する。
5. repo bootstrap が Bun と Node を導入する。
6. repo の `bootstrap` task が `apply` を先に実行する。
7. 配備後に `$HOME` へ移動し、ユーザー設定で `mise bootstrap --yes` を実行する。
8. ユーザー bootstrap が packages、zinit、`[tools]` の順で収束させる。

repo bootstrap とユーザー bootstrap は config root が異なるため、再帰ではなく二段階の収束として扱う。ユーザー bootstrap は必ず `apply` 完了後に実行し、repo 内の `dist/` を直接入力にしない。

## `.Brewfile` の撤去

`dist/.Brewfile` と `.dotfiles.json` の symlink entry を削除するだけでは、既存の `~/.Brewfile` symlink が壊れた状態で残る。

既存の prune 機構を拡張し、`.dotfiles.json` で明示した、配布元に存在しない top-level path も退避対象にできるようにする。`.Brewfile` を prune entry として宣言し、既存の `~/.Brewfile` を通常の `~/.dotfiles-backups/` 配下へ移動する。任意の未管理ファイルを削除せず、明示された path だけを扱う。

この拡張後も、copy directory 配下の余剰ファイルを prune する既存挙動は維持する。

## エラー処理

- mise 本体を固定バージョンへ導入できなければ bootstrap を開始しない。
- repo の配備に失敗した場合、ユーザー bootstrap を開始しない。
- package、repo、tool のいずれかが失敗した場合、mise bootstrap の終了コードをそのまま失敗として返す。
- dotfiles の既存バックアップと置換失敗時の復元処理は変更しない。
- mise 内蔵 brew/cask installer が対象を扱えない場合、Homebrew CLI へ暗黙に fallback しない。

## 検証

実装は次の順でテストする。

1. top-level stale path が削除ではなく退避されることを filesystem 上で確認する。
2. ユーザー config の bootstrap を隔離 HOME で実行し、CLI backend の解決と executable を確認する。外部 package の実インストールは dry-run/status で検証する。
3. `mise run dev:test` を実行する。
4. GitHub Actions の public install smoke test で公開 install command からユーザー環境が収束することを確認する。
5. 実環境を変更しない `mise run plan` と `mise bootstrap --dry-run` で最終差分を確認する。

test は設定ファイルの文字列ではなく、コマンド実行結果、生成された plan、終了コード、filesystem 上の状態を観測する。

## 完了条件

- `install.sh` に Homebrew の導入、`brew shellenv`、`brew bundle` がない。
- `dist/.Brewfile` が存在しない。
- 全七項目と zinit が `dist/.config/mise/config.toml` から管理される。
- CLI 四項目は上記の厳密なバージョンに固定される。
- Git、Ghostty、Raycast は mise bootstrap package として `latest` に収束する。
- 既存の `~/.Brewfile` は削除ではなくバックアップされる。
- 初回セットアップと再実行が成功し、全契約 test が通る。
