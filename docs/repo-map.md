# 設計

## 管理境界

ファイルは次の 6 区分で管理する。共有可能な設定と非公開にすべき情報を同一リポジトリに混在させないための境界である。

- `repo runtime` — この repo 自身を動かすために必要なファイル。home directory には配備しない。
- `home-managed user tools` — `nix/home.nix` と `nix/packages.nix` にユーザー単位の宣言を置く。共有設定ファイルの実体は `home/` に置く。通常は root flake から system generation と一緒に反映し、devcontainer から実ファイルとして見える必要があるものだけ `.dotfiles.json` の `copy` で配備する。
- `system configuration` — root の `flake.nix` / `flake.lock` と `nix/system.nix` に Mac 全体の設定を置く。nix-darwin で反映し、Homebrew 本体と cask もここで管理する。
- `private system configuration` — 公開できない追加設定だけを別の root flake に置く。公開 `darwinModules.default` と `lib.mkDarwinSystem` を利用し、共有設定を複製しない。
- `local-only` — マシン固有の設定。repo に入れず、各マシンに手で置く（例: ホームディレクトリ内の `.zsh.d/local.zsh`）。
- `secrets` — 認証情報や鍵などの機密。repo と `home/` のどちらにも入れない（推奨置き場の例: 各マシンのホームディレクトリ内の `.zsh.d/secrets.zsh`）。

Nix の実現手段ごとにトップレベルディレクトリを分けない。`nix/` を Nix 宣言の単一入口とし、`default.nix` が全体を組み立て、`home.nix` / `packages.nix` / `system.nix` が責務を分担する。

### 置き場所の判断

- repo の実行だけに必要 → `repo runtime`
- ユーザー単位の home 配置 → `nix/home.nix`
- ユーザー単位で常設するパッケージ → `nix/packages.nix`
- ユーザーへ配る共有設定ファイルの実体 → `home/`
- Mac 全体へ反映したい → `nix/system.nix`
- Mac 全体へ反映したいが公開できない → `private system configuration`
- マシン固有 → `local-only`
- 機密 → `secrets`（最終判断はユーザーが行う）

## Bootstrap の原則

- `bin/` は直接実行するリポジトリの entrypoint を配置し、`lib/` は各 entrypoint が利用する内部実装を受け持つ。
- `install.sh` は新しい Mac をセットアップする唯一の入口とし、repo tools、system + Home Manager、user tools と repositories の順序で実行を制御する。
- `dotfiles` CLI は日常的な system の plan / apply に使う。リポジトリのテスト実行制御（オーケストレーション）は CLI に持たせず、CI では各テストコマンドを直接実行する。
- Nix で合理的に管理できる CLI やツールチェーンは `nix/packages.nix` に定義を集約する。`home/.config/mise/config.toml` の `[tools]` は、Nix へ移行中の既存ツールおよび Nix で合理的に管理できない明示的な例外のみに限定する。新しいツールは追加せず、既存ツールのバージョンや配布元を変更する際は、同一の変更で Nix へ移行可能かをあらかじめ判断する。mise は残る例外の管理と補助タスクの実行を受け持つ。
- `dotfiles` CLI のリポジトリ固有ロジックは `cli/` 配下の Rust 実装に集約する。標準コマンドの実行自体は外部プロセスへ委ねる場合でも、引数の検証、source の選択、実行手順、失敗時のハンドリングといった判断や手順は Rust 側に集約する。
- `plan` は Lix や稼働中のシステムを変更せず、`.dotfiles.json` に記載された home copy 定義も検証と表示だけを行う。Lix の導入、ビルド済み世代のアクティベーション、Home Manager のアクティベーション、source 選択状態の永続化、home copy の実体反映は、すべて `apply` のみが行う。
- public source は実行ユーザーとローカルのリポジトリチェックアウトを入力として root flake を評価する。一方、private source はユーザーを root flake 内で明示し、コミット済みの lock ファイルから pure に評価する。
- system source の選択状態は `/etc/nix-darwin/flake.nix` のシンボリックリンクのみで管理し、リポジトリ独自の sidecar 状態を持たない。
- ユーザー向けのインストールタスクは、`home/` ではなくホームディレクトリを入力とする。`apply` 適用後に配置されたホームディレクトリ上の設定ファイルを用いて実行し、リポジトリ内の管理元パスを直接参照しない。
- 編集内容を即座に反映させたい通常の設定ファイルは、稼働中のリポジトリチェックアウトへの out-of-store link とする。
- Nix で宣言する system および home の設定はすべて `nix/` に集約する。nix-darwin や Home Manager はあくまで実現手段であり、トップレベルディレクトリの分割境界にはしない。
- ログインユーザーが常用するツールは、`environment.systemPackages` ではなく `nix/packages.nix` で定義し、Home Manager の `home.packages` を介して利用する。
- CI 上で同一の CLI やツールチェーンを必要とする場合も、個別にバージョン定義を持たず、root flake が公開する共通の Nix ツールセットを利用する。GitHub Actions ではバイナリキャッシュを活用し、同一 store path の再取得や再ビルドを防ぐ。
- devcontainer から参照するエージェント向けリソースは、`/nix/store` やホスト固有の絶対パスシンボリックリンクにしてはならない。`.dotfiles.json` の `copy` に列挙したファイルまたはディレクトリのみを、実ファイルとして `$HOME` 配下に配備する。列挙されたディレクトリ配下は dotfiles が所有し、同期時にはコピー元（source）に存在しない子要素を削除するが、親ディレクトリや同階層にある他のランタイムファイルには影響を与えない。
- `.dotfiles.json` は `copy` キーのみを受け付ける。指定するパスは、重複がなく、アルファベット順に整列され、相対パス表記であり、かつ相互に包含関係を持たないものでなければならない。未知のキーや不正なパスが含まれる場合は、system の plan / apply を実行する前にエラーとして拒否する。
- 標準機能で要件を満たせるホームディレクトリへの配備は Home Manager に任せる。`.dotfiles.json` によるファイルコピーは devcontainer の環境境界を越えるための限定的な例外措置であり、これ以外の独自マニフェストや配置状態は保持しない。
- ブートストラップ処理の妥当性は E2E テストで検証する。シェルの実行順序や全体の導線の検証を、内部実装手順を固定化するユニットテストで代替してはならない。

## バージョンピン留め

依存は、別のマシンや時点でも同じものを取得できる形式で指定する。

| 指定する場所 | 形式 |
|---|---|
| mise `[tools]` | `major.minor.patch` |
| GitHub Actions | commit SHA とバージョンコメント |
| Nix `flake.nix` input | 追従する branch / channel |
| Nix `flake.lock` | exact revision |
| Nix user package | package attribute + expected version assertion + バージョンコメント |
| その他 | 厳密なバージョンまたはrevision |

Nix flake では `flake.nix` に追従先ブランチやチャネルの意図を記述し、具体的なリビジョンの固定は `flake.lock` に委ねる。`latest`、`^x.y`、`~x.y`、`@v4` といった指定は固定形式として扱わない。GitHub Actions のアクション指定は `@abc123 # v4.3.1` のようにコミット SHA とコメントを併記する形式とする。

Nix でユーザー常設ツールを管理する際は、`flake.lock` による取得元リビジョンの固定にとどまらず、`nix/packages.nix` 内で期待するバージョンを明示してアサーションを行う。利用可能な場合は `go_1_26` や `rustPackages_1_97` などのバージョン付き属性（versioned attribute）を選択し、コメントにも完全なバージョン番号を記載する。nixpkgs の更新によって実際のバージョンが変わった場合は、期待バージョンを意図的に更新するまで評価を失敗させる。

Homebrew の formula および cask は `nix/homebrew-packages.nix` に集約する。ユーザー単位で常設する CLI ツールは Nix による管理を原則とし、Nix で合理的に管理できないものに限って mise、Homebrew、公式インストーラーなど、各ツールに適した自然な導入手段を例外として用いる。mise の `[tools]` は段階的に縮小すべき移行対象であり、例外として残す場合はその理由を設定やコメントから判別できる状態にする。

`install.sh` は各種バージョン管理ツールの導入前に実行される。そのため、自身が属するコミットの SHA をスクリプト内に既定値として埋め込まず、実行時に `origin/master` の最新コミットを取得する。なお、既存のローカルチェックアウトに変更がある場合、別ブランチにいる場合、または `origin/master` から分岐したコミットが存在する場合は自動更新を行わない。
