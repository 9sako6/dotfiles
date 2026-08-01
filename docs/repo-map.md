# 設計

## 管理境界

ファイルは次の 5 区分で扱う。共有してよい設定と共有してはいけない情報を同じ repo に混ぜないための境界。

- `repo runtime` — この repo 自身を動かすために必要なファイル。home directory には配備しない。
- `home-managed user tools` — home directory に配備して日常利用する設定。mise は開発 CLI と repository bootstrap だけを管理する。
- `system configuration` — `darwin/` に置く macOS の設定。nix-darwin で Mac 全体へ反映し、Homebrew 本体と cask もここで管理する。
- `local-only` — マシン固有の設定。repo に入れず、各マシンに手で置く（例: `~/.zsh.d/local.zsh`）。
- `secrets` — 認証情報や鍵などの機密。repo と `home/` のどちらにも入れない（推奨置き場の例: `~/.zsh.d/secrets.zsh`）。

### 置き場所の判断

- repo の実行だけに必要 → `repo runtime`
- 複数マシンで共有したい → `home-managed user tools`
- Mac 全体へ反映したい → `system configuration`
- マシン固有 → `local-only`
- 機密 → `secrets`（最終判断はユーザーが行う）

## Bootstrap の原則

- `install.sh` は bootstrap に徹する。外部前提の導入と task の実行順制御だけを担当し、複数責務を 1 つの task に隠さない。
- `.mise.toml` の task は 1 task 1 責務に保つ。依存関係がある処理も、観測可能な単位に分ける。
- `install:system` が Lix の有無を確認し、Lix Installer のチェックサムを検証してから nix-darwin を適用する。利用者は実行順を指定しない。
- user 向け install task は `home/` ではなく `~/` を入力にする。`apply` 後の home directory 上の設定を使って実行し、repo 内の管理元パスを直接参照しない。
- 標準コマンドで足りる処理は shell で書き、薄いラッパーで包まない。TypeScript を使うのは repo 固有のロジックがあるときだけ。
- bootstrap の正しさは e2e で確認する。shell の順序や導線の確認を、内部手順を固定する unit test に逃がさない。

## バージョンピン留め

依存はすべて厳密にピン留めする。同じ repo を別マシン・別時点で動かしても結果がずれないようにするため。

- mise は `major.minor.patch`
- GitHub Actions は commit SHA ＋ バージョンコメント（semver タグより immutable）
- nix-homebrew は flake input の commit SHA
- Homebrew の formula / cask は nix-darwin の宣言へ集約する。バージョン管理が必要な CLI は mise `[tools]` または Nix で扱う
- `install.sh` は、バージョン管理ツール導入前のため、ユーザーの明示的な許可があれば固定を緩めてよい
