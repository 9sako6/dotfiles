# 運用ガイド

macOS 環境を前提とする。管理境界は [repo-map.md](repo-map.md) の「管理境界」で定義された内容に従う。

## 変更前の確認

変更に着手する前に、管理区分を次の順序で確定する。

```mermaid
flowchart TD
    start["変更対象を確認"] --> known{"管理区分を判断できるか"}
    known -->|"はい"| boundary{"管理区分"}
    known -->|"いいえ"| map["docs/repo-map.md を確認"]
    map --> resolved{"管理区分を判断できたか"}
    resolved -->|"はい"| boundary
    resolved -->|"いいえ"| proposal["不足している判断基準を特定し、更新案を作る"]
    proposal --> ask["ユーザーに確認する"]
    boundary -->|"repo runtime"| repo["リポジトリ固有の規則は project rule に置く"]
    boundary -->|"home-managed user tools"| home["skill には配備先でも使える一般規則だけを書く"]
    boundary -->|"system configuration"| system["Mac 全体の設定は公開 flake.nix と nix/system.nix に置く"]
    boundary -->|"private system configuration"| private["非公開設定は dotfiles.local.toml の private.path で結合する"]
    boundary -->|"local-only"| local["repo に入れず、各マシンの dotfiles.local.toml 等に置く"]
    boundary -->|"secrets"| secrets["repo、home/、dotfiles.local.toml に入れず、ユーザーに確認する"]
    repo --> change["変更に進む"]
    home --> change
    system --> change
    private --> change
```

### 生成物

`apm.lock.yaml`、`flake.lock`、`nix/localllm/uv.lock` などの生成物は手動で編集しない。
固定ファイルの更新は開発時のみ意図的に行い、`plan` や `apply` の実行中に自動更新されることはない。

```mermaid
flowchart TD
    target["変更対象"] --> generated{"生成物か"}
    generated -->|"いいえ"| edit["直接変更する"]
    generated -->|"はい"| locate["生成元と再生成手順を探す"]
    locate --> found{"手順を特定できたか"}
    found -->|"いいえ"| ask["ユーザーに確認する"]
    found -->|"はい"| regenerate["正規の手順で再生成する"]
    regenerate --> order["列挙順も生成器に委ねる"]
```

## 初回セットアップ

```sh
curl -fsSL https://dot.9sako6.com | sh
```

公開リポジトリ直下の `flake.nix` が唯一の構成ルートである。
Home Manager は nix-darwin のモジュールとして組み込まれているため、システムと通常のホーム設定は同一の `apply` で反映する。
`dotfiles.toml` の `copy` 対象は、システム反映の成功後に Rust CLI が `$HOME` へ実体として配備する。

Home Managerの配備先に既存ファイルがある場合は、`.pre-home-manager` 接尾辞を付与して退避される。
`curl | sh` の実行時は、確認の入力のみを制御端末から読み取り、ダウンロード中のスクリプトを入力値として消費しない。

## 日常コマンド

システムの確認・反映、設定の一覧、エージェント管理のコマンドは[CLIのUsage](../cli/README.md#usage)を参照してください。個別の説明は[settings](../cli/README.md#settings)と[agents](../cli/README.md#agents)にあります。

公開リポジトリの更新には通常のGit操作を使います。

```sh
git pull
```

その他の補助タスクは `mise tasks` で一覧できる。mise 本体の状態確認には `mise ls --missing` や `mise prune --tools` などの標準コマンドを使用する。

ユーザー単位の常設ツールは Nix 管理を原則とし、新規ツールを mise の `[tools]` に追加しない。既存の mise 管理ツールを更新する際は、Nix へ移行可能かを事前に確認し、合理的に移行できる場合は `nix/packages.nix` と Home Manager へ移す。mise に残すのは Nix で合理的に管理できない例外のみとし、その理由を設定から判別できる状態を維持する。

公開構成の flake ルートはリポジトリ直下の `flake.nix` および `flake.lock` である。Nix で宣言するシステムやホーム設定は `nix/`、共有設定ファイルの実体は `home/` に配置する。

通常の設定ファイルや `.config`、`.zsh.d`、`mybin` は、稼働中のリポジトリへの直接のシンボリックリンクとし、編集内容を即座に反映させる。
devcontainerから参照するエージェント用設定の実体配備は、CLIの[copy](../cli/README.md#copy)を参照してください。対象パスの制約、ディレクトリ配下の同期範囲、Home Managerとの重複検査、権限の扱いを説明しています。

## 設定ファイル仕様 (dotfiles.toml / dotfiles.local.toml)

設定項目、既定値、マージ規則、設定例はCLIの[設定ファイル](../cli/README.md#設定ファイル)を参照してください。

## 反映ライフサイクルとソース管理

入力の扱いはCLIの[実行対象の選択](../cli/README.md#実行対象の選択)、反映順序と整合性の設計は[設計文書](repo-map.md#cli-と評価反映の整合性)を参照してください。失敗時は[ロールバック](#ロールバック)の手順で復旧します。

## ロールバック

システムを直前の正常な世代に戻す場合は、以下のコマンドを実行する。

```sh
mise run system:rollback
sudo darwin-rebuild switch --rollback
```

ロールバック時の注意点:
- 反映失敗時に途中で停止した場合、結果記録シンボリックリンクは前世代を指しているが、システムや Homebrew に部分的な変更が残されている可能性がある。
- システム世代を戻した後、実体配備したホームファイルを復元するには、過去のコミット済みチェックアウトを明示的に指定して適用する。
- ソース記録シンボリックリンクの差し戻しや整合性の確認は、システムとホームの復元が検証された後にのみ行う。
- 過去世代の固定ファイルおよびビルドキャッシュは破棄せず保持しなければならない。

Nix のガベージコレクションは日本時間で毎日 0:00 に実行され、2日を超えた古い世代を削除する。
手動で `nix-collect-garbage` を実行する場合も、ロールバックに必要な世代が含まれていないことを事前に確認する。

## ローカル LLM と OpenCode (localllm)

ローカルLLMの有効化とモデル指定はCLIの[localllm設定](../cli/README.md#localllm)を参照してください。設定が無効でも、開発・検証目的で`nix build .#localllm`を明示的に実行するとパッケージをビルドします。以前に取得したモデルデータは無効化だけでは削除されず、不要になったストアパスは後続のNixガベージコレクションで回収されます。

新しく導入されるモデルID `qwen3.8-9b-distill-4bit` は、Qwen3.5-9BをベースとしたEmperoの `Qwen3.8-9B-Distill` について、[配布元](https://huggingface.co/PocketAiHub/Qwen3.8-9B-MLX) が公開している4bit変換版を採用した非公式蒸留モデルです。動作環境は既存の `mlx-vlm` 0.7.0 を用い、取得時のバージョンおよびファイルハッシュはカタログで固定して管理されます。

利用するモデルの選択は、`dotfiles.local.toml` 内の `localllm.models` に1件のみを記述し、`localllm.default_model` にも同じIDを指定して行います。初回取得データ量の目安は新モデルが約6GB、引き続き選択可能な従来の `qwen3.8-27b-4bit` が約16GBです。なお、これらはストレージ取得時の容量であり、実際の運用時にはモデルの重みに加えて会話処理等の作業メモリが別途必要となります。

モデル推論バックエンドは `nix/packages.nix` が実装を所有し、[uv2nix](https://pyproject-nix.github.io/uv2nix/usage/getting-started.html) と `nix/localllm/uv.lock` により mlx-vlm 0.7.0 および MLX 0.32.2 の Metal 向け wheel に固定されており、実行時のパッケージ追加は行わない。動作検証としては、Apple Silicon 向けの qwen3.8-27b-4bit と qwen3.8-9b-distill-4bit の両モデルにおいて、合成した機密でない入力を用いて Metal 推論および OpenCode を通したファイル作成・読み取りの動作を確認しているほか、9B モデルでも実際に bash ツール経由でのファイル書き込みと読み取りが動作している。

サーバーはメモリを抑えるためKVキャッシュ4bit、prefill 64トークン、同時リクエスト1件とし、システムのGPUメモリ上限や常駐アプリの状態は変更しません。そのため、長い入力では応答までに数分かかることがあります。

localllmのOpenCodeに通知する会話上限について、長文の早期要約を避けるためモデル設定に合わせて262,144トークンへと拡張しました。なお、1回あたりの回答上限は1,024トークンのまま維持しています。

@prevalentware/opencode-goal-plugin 0.1.49を採用し、Nixでバージョンとソースハッシュを固定するとともに、依存関係（effect、zod、間接依存）をnix/localllm/goal-plugin/package-lock.jsonにより固定してビルド時にバンドル化しているため、起動時の追加取得は発生しません。「/goal <目的>」でのタスク開始、「/goal」での進捗確認、「/pause_goal」「/resume_goal」による一時停止・再開に対応しています。目的は専用の永続データ領域に保存され、会話要約後のコンテキスト引き継ぎおよび自律的な自動続行が可能です。

Nix管理のユーザーツールに `FFmpeg 8.1.2` を追加し、`ffmpeg` および `ffprobe` を提供します。`dotfiles apply` の実行後は `localllm chat` 上から通常のコマンド名でそのまま実行でき、他の既存コマンドと同様に `PATH` 経由で連携して利用できます。

- **実行コマンド:**
  - `localllm chat -- <opencode-arguments>` (プロンプト実行引数を渡す)
  - `localllm serve` (推論サーバーの単独起動)
  - `localllm check` (Metal 演算のみを確認)
- **実行特性:**
  - 常駐デーモンは存在せず、オンデマンドで単一の所有プロセスツリーとして起動する。
  - 外部ネットワークからの接続を受け付けないローカル接続用のアドレスにバインドする。
  - 初回ビルド時または初回取得時には、数 GB のモデルデータ取得が発生し得る旨が事前に通知される。
- **リソースの回収:**
  - `localllm` を無効化（`enabled = false`）した構成一式には、LLM 固有の依存関係は一切含まれない。
  - 使用されなくなったモデルデータは後続の Nix ガベージコレクションによって削除されるが、他の構成と共有されている基本依存関係は維持される。
- **OpenCode 統合とセキュリティ制限:**
  - OpenCode 1.18.13をベースとし、専用のXDG設定および履歴領域を割り当てています。Nix管理下のGoalプラグインのみを有効化し、外部プラグイン、外部スキル、ユーザー定義MCP、会話共有機能、外部プロバイダへの自動切替はすべて無効化しています。クライアントは起動元のPATH、HOME、ツール用環境変数を継承しつつOpenCode固有の環境変数を専用設定で置換し、推論サーバーは最小限の独立環境として分離・運用しています。
  - 推論サーバーは `sandbox-exec` により外向き通信を遮断し、ローカル接続専用の構成を維持します。`OpenCode` 本体および起動される子コマンドはネットワーク接続が可能で、`webfetch` および `websearch` を許可しています。今回のランチャーにより `OPENCODE_ENABLE_EXA=1` が設定され、APIキー不要の[組み込みExa検索](https://opencode.ai/docs/tools/#websearch)が自動的に有効になります。推論はローカルで実行しますが、検索語やアクセス先URL、ツール経由で送信する内容は外部へ送信されます。完全な通信遮断状態ではありません。
  - 自動テストでは、ローカルサーバー不在時における適切な失敗、クラウド推論への自動切り替えが発生しないこと、ユーザーPATH上のコマンド実行とHOME環境の引き継ぎ、Web取得および検索ツールの利用可能性、推論サーバーの外向き通信遮断を検証します。なお、CI上では巨大モデルの展開やGPUによる推論実行は行いません。

## 検証

変更した振る舞いはコマンドやスクリプトを用いて観測する。リポジトリ全体を検証する際はラッパーを挟まず、CI と同一のテストコマンドを直接実行する。

```sh
bun install --frozen-lockfile
bun run tsc --noEmit
bun test ./tests ./home/.apm/skills/anki/tools/*.test.ts
cargo fmt --check --manifest-path cli/Cargo.toml
cargo clippy --locked --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path cli/Cargo.toml
nix build --no-link .#checks.aarch64-darwin.composition .#checks.aarch64-darwin.configuration .#checks.aarch64-darwin.modelFetch
opencode_package="$(nix build --no-link --print-out-paths .#localllmClient)"
goal_plugin="$(nix build --no-link --print-out-paths .#localllmGoalPlugin)"
DOTFILES_TEST_OPENCODE="$opencode_package/bin/opencode" DOTFILES_TEST_GOAL_PLUGIN="$goal_plugin" python3 -m unittest discover -s nix/localllm -p 'test_*.py'
python3 -m unittest discover -s nix/tests -p 'test_*.py'
nix build --no-link --offline .#checks.aarch64-darwin.modelFetch
```

- `nix build` によるテストでは、固定ハッシュを持つ小さなテスト用データを用いてモデル取得ロジックを検証し、反復ビルド時のキャッシュ動作を確認する。
- 純粋なシステム評価テストとして `nix eval --raw .#darwinConfigurations.current.system.drvPath` を実行し、テスト用ユーザー設定を用いてローカルの非純粋性を排除した評価が通ることを検証する。
- 依存固定ファイルの更新は開発時のみ明示的に行い、以下のコマンドを使用する。
  - `uv lock --project nix/localllm`
  - `nix flake lock`

設定ファイルやソースコードの文面そのものを直接検査するテストは作成しない。

## 変更前後の基本手順

1. 上の手順で管理区分を確定
2. `home-managed user tools` や構成を変更する場合は、[plan](../cli/README.md#plan)でHome Manager、パッケージ差分、およびcopy対象を確認
3. 必要な変更を入れる
4. 「検証」の手順を実施
5. 変更した管理区分に応じて反映
   - `home-managed user tools` — [apply](../cli/README.md#apply)
   - `system configuration` — [apply](../cli/README.md#apply)
   - `private system configuration` — [ローカル設定](../cli/README.md#設定ファイル)を更新し[apply](../cli/README.md#apply)

`repo runtime` の変更に対する反映コマンドはない。

Homebrew 本体は nix-homebrew、formula と cask は nix-darwin、通常のホームディレクトリ設定は Home Manager、devcontainer から参照可能な copy 対象は Rust CLI がそれぞれ管理する。

GitHub-hosted な macOS runner では、Home Manager のユーザー反映とシステム派生のビルドを個別に検証し、nix-darwin のシステム反映は実施しない。Nix ストアのパスは GitHub Actions のバイナリキャッシュを活用してワークフロー間で再利用する。新規 Mac への反映検証は、Homebrew の入っていない VM または実機で確認する。
