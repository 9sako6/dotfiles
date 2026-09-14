# 設計

## 管理境界

ファイルは次の 6 区分で管理する。共有可能な設定と非公開にすべき情報を同一リポジトリに混在させず、かつ単一の構成ルートから安全に組み立てるための境界である。

- `repo runtime` — この repo 自身を動かすために必要なファイル。home directory には配備しない。
- `home-managed user tools` — `nix/home.nix` と `nix/packages.nix` にユーザー単位の宣言を置く。共有設定ファイルの実体は `home/` に置く。root flake からシステム世代と一緒に反映し、devcontainer から実ファイルとして見える必要があるものだけ `dotfiles.toml` の `copy` で配備する。
- `system configuration` — 公開ルートの `flake.nix` / `flake.lock` と `nix/system.nix` に Mac 全体の設定を置く。nix-darwin で反映し、Homebrew 本体と cask もここで管理する。公開 `flake.nix` が唯一の構成ルートである。
- `private system configuration` — 公開できない追加設定。独立した別 root flake は設けず、ローカル設定 `dotfiles.local.toml` の `private.path` 経由で公開 root flake の評価時に結合する。
- `local-only` — マシン固有の設定。repo にコミットせず、Git 管理外の `dotfiles.local.toml` や各マシンのローカルファイルに置く。機密情報は含めず、必要に応じて個別にバックアップする。
- `secrets` — 認証情報や鍵などの機密。repo、`home/`、および `dotfiles.local.toml` のいずれにも入れない（最終判断はユーザーが行う）。

Nix の実現手段ごとにトップレベルディレクトリを分けない。`nix/` を Nix 宣言の単一入口とし、`default.nix` が全体を組み立て、`home.nix` / `packages.nix` / `system.nix` が責務を分担する。

### 置き場所の判断

- repo の実行だけに必要 → `repo runtime`
- ユーザー単位の home 配置 → `nix/home.nix`
- ユーザー単位で常設するパッケージ → `nix/packages.nix`
- ユーザーへ配る共有設定ファイルの実体 → `home/`
- Mac 全体へ反映したい公開設定 → `system configuration` (`flake.nix` / `nix/system.nix`)
- Mac 全体へ反映したい非公開設定 → `private system configuration` (`dotfiles.local.toml` の `private.path` で指定)
- マシン固有の非機密設定 → `local-only` (`dotfiles.local.toml` 等)
- 機密情報 → `secrets`（リポジトリおよびローカル TOML の外で管理）

## Bootstrap と設計の原則

### 構成ルートと設定合成

利用者向けの設定項目、既定値、記述例はCLIの[設定ファイル](../cli/README.md#設定ファイル)を参照してください。

- 公開リポジトリ直下の `flake.nix` が唯一の構成ルートである。すべての構成は公開rootを起点に評価される。
- 構成設定は、リポジトリ共有の `dotfiles.toml` と、Git 管理外の `dotfiles.local.toml` の 2 つで管理する。
- 設定ファイルの構文解析は Nix の組み込み関数 [builtins.fromTOML](https://nix.dev/manual/nix/2.35/language/builtins.html#builtins-fromTOML) が行い、型検査や設定検証は Nix モジュールのスキーマが担当する。CLI は TOML 構文解析エラー時の出力を抑制して設定値の漏洩を防ぐ。スキーマ定義違反、型不一致、未知のキーが検出された場合も評価を停止する。
- 設定テーブルのマージは、「既定値 < 公開共有 (`dotfiles.toml`) < ローカル (`dotfiles.local.toml`)」の順序で再帰的に行われる。
- 配列およびスカラー値はマージされず、`false` や空配列 `[]` を含め、上位の設定で完全に置換される。
- `copy` セクションは共有の `dotfiles.toml` でのみ定義可能とし、ローカルファイルでの指定は拒否される。指定するパスは相対パス表記であり、重複がなく、アルファベット順に整列され、かつ相互に包含関係を持たないものでなければならない。
- `private` セクションはローカルの `dotfiles.local.toml` でのみ定義可能である。`private.path` は公開リポジトリルートからの相対パスまたは絶対パス入力を受け付ける（文書内では可搬な相対パスで表現する）。
- `dotfiles.local.toml` は機密情報を含んではならず、個別にバックアップする。不在時は既定値が適用され、非公開パスの探索は行われない。読み取り不可や構文不正がある場合は処理を停止する。

### CLI と評価・反映の整合性

コマンドの構文、オプション、実行例は[CLIのUsage](../cli/README.md#usage)を参照してください。

- `bin/` は直接実行するリポジトリの入口を配置し、`lib/` は各入口が利用する内部実装を受け持つ。
- `install.sh` は新しい Mac をセットアップする唯一の入口とし、repo tools、system + Home Manager、user tools と repositories の順序で実行を制御する。
- `dotfiles` CLI は日常的な system の plan / apply に使う。リポジトリのテスト実行制御は CLI に持たせず、CI では各テストコマンドを直接実行する。
- `dotfiles` CLI は、追跡対象の未コミット変更のスナップショットとローカル設定入力を記録してビルドを行う。チェックアウト全体を無条件に参照する `path:.` は使用せず、ローカルファイルを Git に強制追加することもない。
- `plan` および `apply` の実行中に、自動的な `git clone`、`git pull`、または依存固定ファイルの更新は一切行わない。
- ローカル設定による非純粋性は、明示的かつ一時的なマニフェストとして検証済み Nix エントリへ渡す箇所に限定し、公開 CI 環境は完全に純粋に保つ。
- `plan` は、システム派生および Brewfile を構築し、Nix が返す copy 対象一覧を元に Rust CLI が固定ソースから配備計画を作成する。一時的な GC ルートを作成してプレビュー期間中の破棄を防ぐ。稼働中のシステムやファイル配置は変更しない。
- `apply` は排他ロックを取得し、プレビュー確認後に記録された入力が変更されていないことを再検証した上で、同一の世代をシステムに反映する。
- 反映順序は、Nix ネイティブ反映 -> 固定された home copy の実体配備 -> ソース記録シンボリックリンクの更新、の順序を厳密に実行する。
- 反映結果を記録するシステム側のシンボリックリンクは、反映結果の記録としてのみ更新され、選択状態を操作するためのスイッチとしては使用しない。
- 反映が失敗した場合、結果記録シンボリックリンクは直前の正常世代を指したまま保持されるが、システム、ホーム、Homebrew が部分的に変更された状態になる可能性がある。ロールバックは `sudo darwin-rebuild switch --rollback` または `mise run system:rollback` で行い、ホームの実体ファイルは過去のコミット済みチェックアウトから明示的に復元する。検証が完了するまで古い固定ファイルやキャッシュは保持する。

### ローカル LLM と OpenCode

- Apple Silicon向けローカル推論モデル `qwen3.8-27b-4bit`、`qwen3.8-9b-distill-4bit` の提供
- パッケージ依存関係は [uv2nix](https://pyproject-nix.github.io/uv2nix/usage/getting-started.html) および `nix/localllm/uv.lock` を通じて `mlx-vlm 0.7.0` および Metal 向け wheel `mlx 0.32.2` に厳密に固定し、`nix/packages.nix` が実装を所有する。実行時の動的なパッケージ導入は行わない。
- ローカルモデルが有効化されている場合、ソートされた一意の既知モデル ID リストが定義され、起動時には単一のモデルのみがロードされ、`default_model` はモデルリスト内に存在しなければならない。
- 無効化（`enabled = false`）された構成一式には LLM 固有の依存関係は含まれず、使用されなくなったモデルデータは後続の GC で回収される（共有依存関係は保持される）。
- ランチャーには `localllm chat -- <opencode-arguments>`、`localllm serve`、`localllm check` を用意する。常駐デーモンは持たず、オンデマンドで単一の所有プロセスツリーとして起動し、ローカル接続用のアドレスにバインドする。初回ビルド時には数 GB のモデルデータ取得が発生し得る旨が事前に通知される。
- `localllm check` は Metal 演算のみを確認する。
- OpenCode 1.18.13を採用し、専用のXDG設定および履歴領域を用いて運用します。外部スキルやプラグイン、ユーザー定義MCP、会話共有、外部プロバイダーへの自動切り替えは引き続き無効化されています。クライアントには起動元のPATH、HOME、ツール用環境変数を引き継ぐ一方、OpenCode固有の環境変数は専用設定で置き換えて混入を防ぎ、推論サーバーは最小限の独立した環境で分離して稼働させます。
- 推論サーバーは `sandbox-exec` により外向き通信を遮断し、ローカル接続専用の構成を維持します。`OpenCode` 本体および起動される子コマンドはネットワーク接続が可能で、`webfetch` および `websearch` を許可しています。今回のランチャーにより `OPENCODE_ENABLE_EXA=1` が設定され、APIキー不要の[組み込みExa検索](https://opencode.ai/docs/tools/#websearch)が自動的に有効になります。推論はローカルで実行しますが、検索語やアクセス先URL、ツール経由で送信する内容は外部へ送信されます。完全な通信遮断状態ではありません。
- 自動テストでは、ローカルサーバー不在時における適切な失敗、クラウド推論への自動切り替えが発生しないこと、ユーザーPATH上のコマンド実行とHOME環境の引き継ぎ、Web取得および検索ツールの利用可能性、推論サーバーの外向き通信遮断を検証します。なお、CI上では巨大モデルの展開やGPUによる推論実行は行いません。

### パッケージと環境の原則

- Nix で合理的に管理できる CLI やツールチェーンは `nix/packages.nix` に定義を集約する。`home/.config/mise/config.toml` の `[tools]` は、Nix へ移行中の既存ツールおよび Nix で合理的に管理できない明示的な例外のみに限定する。新しいツールは追加せず、既存ツールのバージョンや配布元を変更する際は、同一の変更で Nix へ移行可能かをあらかじめ判断する。mise は残る例外の管理と補助タスクの実行を受け持つ。
- ログインユーザーが常用するツールは、`environment.systemPackages` ではなく `nix/packages.nix` で定義し、Home Manager の `home.packages` を介して利用する。
- 編集内容を即座に反映させたい通常の設定ファイルは、稼働中のリポジトリへの直接のシンボリックリンクとする。
- devcontainer から参照するエージェント用設定は、Nix ストアやホスト固有の絶対パスシンボリックリンクにしてはならない。`dotfiles.toml` の `copy` に列挙したファイルまたはディレクトリのみを、実ファイルとして `$HOME` 配下に配備する。列挙されたディレクトリ配下は dotfiles が所有し、同期時にはコピー元に存在しない子要素を削除するが、親ディレクトリや同階層にある他のランタイムファイルには影響を与えない。
- `copy`宣言されたパスと一致または親子関係にある公開構成のHome Managerリンクのみを自動除外する。その他の有効な`home.file.target`が`copy`対象と重複した場合は、別名キー経由の`target`指定も含めてNix評価時に拒否する。
- 標準機能で要件を満たせるホームディレクトリへの配備は Home Manager に任せる。`dotfiles.toml` によるファイルコピーは devcontainer の環境境界を越えるための限定的な例外措置であり、これ以外の独自マニフェストや配置状態は保持しない。
- CI 上で同一の CLI やツールチェーンを必要とする場合も、個別にバージョン定義を持たず、root flake が公開する共通の Nix ツールセットを利用する。GitHub Actions ではバイナリキャッシュを活用し、同一ストアパスの再取得や再ビルドを防ぐ。
- セットアップ処理の妥当性は E2E テストで検証する。シェルの実行順序や全体の導線の検証を、内部実装手順を固定化するユニットテストで代替してはならない。

## バージョンピン留め

依存は、別のマシンや時点でも同じものを取得できる形式で指定する。

| 指定する場所 | 形式 |
|---|---|
| mise `[tools]` | `major.minor.patch` |
| GitHub Actions | commit SHA とバージョンコメント |
| Nix `flake.nix` input | 追従する branch / channel |
| Nix `flake.lock` | exact revision |
| uv (`nix/localllm/uv.lock`) | exact version / wheel hash (uv2nix 経由) |
| Nix user package | package attribute + expected version assertion + バージョンコメント |
| その他 | 厳密なバージョンまたはrevision |

Nix flake では `flake.nix` に追従先ブランチやチャネルの意図を記述し、具体的なリビジョンの固定は `flake.lock` に委ねる。`latest`、`^x.y`、`~x.y`、`@v4` といった指定は固定形式として扱わない。GitHub Actions のアクション指定は `@abc123 # v4.3.1` のようにコミット SHA とコメントを併記する形式とする。

Python および MLX 関連の依存関係は `nix/localllm/pyproject.toml` および `nix/localllm/uv.lock` で厳密に固定し、[uv2nix](https://pyproject-nix.github.io/uv2nix/usage/getting-started.html) を介して Nix ビルドに変換する。ロックの更新は開発時に `uv lock --project nix/localllm` および `nix flake lock` を用いて明示的に行う。

Nix でユーザー常設ツールを管理する際は、`flake.lock` による取得元リビジョンの固定にとどまらず、`nix/packages.nix` 内で期待するバージョンを明示してアサーションを行う。利用可能な場合は `go_1_26` や `rustPackages_1_97` などのバージョン付き属性を選択し、コメントにも完全なバージョン番号を記載する。nixpkgs の更新によって実際のバージョンが変わった場合は、期待バージョンを意図的に更新するまで評価を失敗させる。

Homebrew の formula および cask は `nix/homebrew-packages.nix` に集約する。ユーザー単位で常設する CLI ツールは Nix による管理を原則とし、Nix で合理的に管理できないものに限って mise、Homebrew、公式インストーラーなど、各ツールに適した自然な導入手段を例外として用いる。mise の `[tools]` は段階的に縮小すべき移行対象であり、例外として残す場合はその理由を設定やコメントから判別できる状態にする。

`install.sh` は各種バージョン管理ツールの導入前に実行される。そのため、自身が属するコミットの SHA をスクリプト内に既定値として埋め込まず、実行時に `origin/master` の最新コミットを取得する。なお、既存のローカルチェックアウトに変更がある場合、別ブランチにいる場合、または `origin/master` から分岐したコミットが存在する場合は自動更新を行わない。
