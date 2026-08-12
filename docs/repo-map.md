# 設計

## 管理境界

ファイルは次の 6 区分で扱う。共有してよい設定と共有してはいけない情報を同じ repo に混ぜないための境界。

- `repo runtime` — この repo 自身を動かすために必要なファイル。home directory には配備しない。
- `home-managed user tools` — `home/` に置く共有ユーザー設定。通常は root flake の Home Manager module から system generation と一緒に反映し、devcontainer から実ファイルとして見える必要があるものだけ `.dotfiles.json` の `copy` で配備する。
- `system configuration` — root の `flake.nix` / `flake.lock` と `darwin/` に置く macOS module。nix-darwin で Mac 全体へ反映し、Homebrew 本体と cask もここで管理する。
- `private system configuration` — 公開できない追加設定だけを別の root flake に置く。公開 `darwinModules.default` と `lib.mkDarwinSystem` を利用し、共有設定を複製しない。
- `local-only` — マシン固有の設定。repo に入れず、各マシンに手で置く（例: `~/.zsh.d/local.zsh`）。
- `secrets` — 認証情報や鍵などの機密。repo と `home/` のどちらにも入れない（推奨置き場の例: `~/.zsh.d/secrets.zsh`）。

### 置き場所の判断

- repo の実行だけに必要 → `repo runtime`
- 複数マシンで共有したい → `home-managed user tools`
- Mac 全体へ反映したい → `system configuration`
- Mac 全体へ反映したいが公開できない → `private system configuration`
- マシン固有 → `local-only`
- 機密 → `secrets`（最終判断はユーザーが行う）

## Bootstrap の原則

- `bin/` は直接実行する repository entrypoint、`lib/` は entrypoint が利用する内部実装を所有する。
- `install.sh` は新しい Mac を一発で構築する唯一の入口とし、repo tools、system + Home Manager、user tools と repositories の順序を所有する。
- `dotfiles` CLI は日常の system plan/apply と repository test の正本とする。`apply` と `test` は mise task として重複公開しない。
- `.mise.toml` は tool version と補助 task を所有する。bootstrap や `dotfiles` CLI の処理を task 側へ複製しない。
- `dotfiles` CLI の repo 固有ロジックは `cli/` の Rust 実装へ集める。標準コマンドの実行自体は外部プロセスへ委ねても、引数検証、source 選択、手順、失敗時の扱いは Rust 側を正本とする。
- `plan` は Lix や active system を変更せず、`.dotfiles.json` の home copy 定義も検証と表示だけ行う。`apply` だけが Lix 導入、build 済み世代の activation、Home Manager activation、source 選択の永続化、home copy の反映を行う。
- public source は実行ユーザーと local dotfiles checkout を入力にして root flake を評価する。private source はユーザーを root flake で明示し、committed lock file から pure に評価する。
- system source の選択状態は `/etc/nix-darwin/flake.nix` の symlink だけとし、独自の sidecar state を持たない。
- user 向け install task は `home/` ではなく `~/` を入力にする。`apply` 後の home directory 上の設定を使って実行し、repo 内の管理元パスを直接参照しない。
- 編集を即時反映する通常設定は live checkout への out-of-store link とする。
- devcontainer から参照する agent resources は `/nix/store` や host 固有の絶対 symlink にしない。`.dotfiles.json` の `copy` に列挙したファイルまたはディレクトリだけを `$HOME` へ実体配備する。列挙したディレクトリは dotfiles がそのディレクトリ以下を所有し、同期時に source にない子を削除するが、親ディレクトリや兄弟の runtime file は触らない。
- `.dotfiles.json` は `copy` だけを受け付け、パスは重複なし、アルファベット順、相対パス、相互に非包含とする。未知の key や不正な path は system plan/apply より前に拒否する。
- 標準機能で足りる home 配備は Home Manager に任せる。`.dotfiles.json` の copy は devcontainer 境界を越えるための限定的な例外とし、別の manifest や配置 state は持たない。
- bootstrap の正しさは e2e で確認する。shell の順序や導線の確認を、内部手順を固定する unit test に逃がさない。

## バージョンピン留め

依存は、別のマシンや時点でも同じものを取得できる形式で指定する。

| 指定する場所 | 形式 |
|---|---|
| mise `[tools]` | `major.minor.patch` |
| GitHub Actions | commit SHA とバージョンコメント |
| Nix `flake.nix` input | 追従する branch / channel |
| Nix `flake.lock` | exact revision |
| その他 | 厳密なバージョンまたはrevision |

Nix flake は `flake.nix` に追従先の意図を書き、具体的な revision は `flake.lock` に任せる。`latest`、`^x.y`、`~x.y`、`@v4` は固定として扱わない。GitHub Actions は `@abc123 # v4.3.1` の形で書く。

Homebrew の formula と cask は nix-darwin の宣言へ集める。バージョン管理が必要な CLI は mise `[tools]` または Nix で扱う。

`install.sh` だけはバージョン管理ツールの導入前に動く。コミット自身の SHA を既定値として埋め込まず、
実行時に `origin/master` の先端を取得する。既存 checkout にローカル変更、別 branch、または
`origin/master` から分岐した commit があれば更新しない。
