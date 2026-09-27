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

flake.lock は上流の inputs やリビジョンを固定するものであり、選択された cask の一覧を記録するものではないため、既存パッケージの選択・解除のみで更新する必要はありません。

| トリガー | コマンド |
| --- | --- |
| flake inputs の追加・変更 | `nix flake lock` ([公式ドキュメント](https://docs.lix.systems/manual/lix/nightly/command-ref/new-cli/nix3-flake-lock.html)) |
| nixpkgs など固定された input の意図的な更新 | `nix flake update nixpkgs` ([公式ドキュメント](https://docs.lix.systems/manual/lix/nightly/command-ref/new-cli/nix3-flake-update.html)) |
| スキル依存の追加・削除・更新 | 対応する `dotfiles agents install`、`uninstall`、`update` および生成される `home/apm.lock.yaml` |

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

Piの導入は見送り、APMの管理対象は`home/apm.yml`の`targets`のみとします。現行のAPM 0.26.0のまま更新は不要で、`compilation.output`を`.codex/AGENTS.md`、`strategy`を`single-file`と指定してリポジトリの`home/`配下に生成します。ホームへの実体配備は`dotfiles.toml`の`copy`配列に列挙した相対パスのみで行い、生成物の再配置やツール別の個別コピーはありません。OpenCodeは`opencode.json`の`instructions`設定でCodexと同じ生成ファイルを参照します。

システムの確認・反映、設定の一覧、エージェント管理のコマンドは[CLIのUsage](../cli/README.md#usage)を参照してください。個別の説明は[settings](../cli/README.md#settings)と[agents](../cli/README.md#agents)にあります。

`dotfiles settings`

dotfiles settingsの既存一覧の下にpackages、system、services、agentsを追加しました。稼働状態の照合やシステム構築・反映、lock更新は行わず、宣言構成から一覧JSONをNix storeへ生成します。入力が同一ならNix標準の評価・ビルドキャッシュを再利用します。

settingsは端末実行とパイプ処理のいずれにおいても全画面表示やスクロール操作を行わず、全一覧の取得と整形が完了してから標準出力にまとめて一度だけ出力して終了します。packagesはname、manager、currentの3列で構成され、currentには宣言バージョン（固定のないHomebrewは—）を表示し、最新バージョンの照会と列は廃止された。

packagesにはHome Managerとenvironment.systemPackagesの宣言バージョン（Lix、zundamonotify、dotfiles CLIを含む）を表示する。名前・管理方式・版が一致する重複をまとめ、バージョン情報のない補助ラッパーを除外する。systemには、Nixのガベージコレクション自動実行や削除条件の引数、Homebrewの自動更新・パッケージ更新・宣言外パッケージ削除方針の有効値を表示する。旧範囲の世代との比較では、既存のネイティブ差分へフォールバックして架空の追加扱いを避ける。

リソース宣言の差分はsettingsと同様の表形式で表示され、追加は緑の`+`、削除は赤の`-`で示されます。copyリソースは変更が生じる対象のみハッシュ値とともにdeployment表へ表示します。宣言一覧が同一でもシステム世代が異なる場合は前後のrevision（またはビルド識別子）を出力し、CLI自身の更新なども確認できます。過去世代と比較できない場合は、従来のNixやHomebrewの差分を併記します。Night Shiftや音声入力ショートカットの設定は、`nix/macos-settings.nix`の共通宣言を反映し、一覧からも参照されます。

変更がない場合、planとapplyは差分表示や確認を行わずに正常終了する。変更がある場合は差分を標準出力に一度だけ出力し、applyのみ表示後にyesの確認を行う。

planとapplyの実行中は、設定の準備、システムの評価、ビルド、差分の確認という各処理の開始と終了時の所要時間に加え、同じ処理が続く間は10秒ごとに経過秒数が標準エラー出力に表示されます（完了割合の表示ではありません）。進捗通知に外部コマンドの非公開の診断情報は含まれず、経過秒数の追加表示によって処理が継続中であることを確認できます。差分一覧は標準出力に一括表示され、applyのyes確認に進む前に進捗通知は停止します。

実行時に入力ハッシュが一致した場合、Nix全体のinventory評価やシステム評価、build、nix-env、darwin-rebuild、Homebrew反映を省略します。処理はcopy対象の実体差分確認と表示に進み、対話的な承認後に配置処理を実施します。差分がない場合はそのまま終了します。初回適用時や旧世代の記録が存在しない場合、あるいはCLI、Nixコード、private flake、ローカル設定に変更がある場合は、通常システム評価と反映処理を実行します。settingsなどの管理項目も通常の一覧表示を維持します。

短縮経路を利用するには、更新後のRust CLIから通常applyを一度実行し、世代に入力記録を作成する必要があります。更新前CLIで新しいCLIを反映した段階では入力記録がないため、更新後CLIで再度通常applyを実行してください。

通常経路のplanおよびapplyでは宣言一覧を評価して差分を先行表示し、applyのyes確認後に必要なシステム評価とビルドを行い、処理前後に入力とアクティブ世代を再検証した上で同一の固定入力に基づく世代を反映する。宣言一覧が同一でも世代が異なる場合は評価時の出力パスを比較して差分を示し、旧世代とのinventory形式が異なる場合のみ従来のビルド付きネイティブ差分へフォールバックするため処理時間を要する。

通常の反映ではビルド済みの更新先システム内にある`sw/bin/dotfiles`から`apply-built`を実行し、copy-onlyでは現在の世代内の同等CLIを使用します。これにより、呼び出し元の旧CLIが新しい内部コマンドに対応していない場合でもCLI自身を更新できます。なお、シェル接続の引数形式は既存CLIとの互換性維持のために保持しており、世代内CLIが欠落しているか実行不能な場合はsudoの実行前に失敗します。

copy対象ファイルの変更のみを反映するcopy-onlyでは、sudoを呼び出さないためパスワード入力が不要です。システム構成やCLIの更新を伴う通常applyでは、引き続きsudoを使用します。本変更の初回導入時は、CLIのシステム更新と共通ロックの準備のために通常applyの実行が一度必要となり、パスワード入力が求められる場合があります。

公開リポジトリの更新には通常のGit操作を使います。

```sh
git pull
```

その他の補助タスクは `mise tasks` で一覧できる。mise 本体の状態確認には `mise ls --missing` や `mise prune --tools` などの標準コマンドを使用する。

ユーザー単位の常設ツールは Nix 管理を原則とし、新規ツールを mise の `[tools]` に追加しない。既存の mise 管理ツールを更新する際は、Nix へ移行可能かを事前に確認し、合理的に移行できる場合は `nix/packages.nix` と Home Manager へ移す。mise に残すのは Nix で合理的に管理できない例外のみとし、その理由を設定から判別できる状態を維持する。

公開構成の flake ルートはリポジトリ直下の `flake.nix` および `flake.lock` である。Nix で宣言するシステムやホーム設定は `nix/`、共有設定ファイルの実体は `home/` に配置する。

通常の設定ファイルや `.config`、`.zsh.d`、`mybin` は、稼働中のリポジトリへの直接のシンボリックリンクとし、編集内容を即座に反映させる。
devcontainerから参照するエージェント用設定の実体配備は、CLIの[copy](../cli/README.md#copy)を参照してください。対象パスの制約、ディレクトリ配下の同期範囲、Home Managerとの重複検査、権限の扱いを説明しています。

## Zinitプラグインの検証

プラグイン読み込みには`dotfiles zinit verify`による検証が必要です。新CLI未導入時は検証不能として読み込みが拒否されるため、通常のapplyでCLIを更新してください。本コマンドはNixやdotfilesのチェックアウトを要求せず、外部コマンドはGitだけを使います。

検証処理はRustのtomlクレートでmiseの`config.toml`を直接解析します。配備先のGit HEADが設定の40桁コミットと一致し、dirtyでないプラグインのみを正常と判定します。設定の構文エラーなど全体エラー時はstdoutへ出力されません。

問題のあるプラグインはstderrへ診断を出力して拒否し、1件でも拒否があれば終了コード1を返します。その際も正常なプラグインはstdoutへ出力されるため、zshrcは1回の呼び出しで得られた検証済みプラグインのみを読み込みます。

## CLI のビルドキャッシュ

`.github/workflows/cache-cli.yml` は、`master` への push または手動実行時に macOS arm64 上で Lix をセットアップし、`.#dotfiles` をビルド（Rustの回帰テストを含む）します。CLIのversionと`.#dotfiles.version`の一致を検証後、公開 CLI と実行時依存のみを [Cachix](https://docs.cachix.org/getting-started) へ [push](https://docs.cachix.org/pushing) します。Cachix 1.11.1 は root flake の `.#cachix` から取得し、`nix build` や `run` では `--no-update-lock-file` および `--no-write-lock-file` で固定ファイル更新を禁止しています。なお、本ワークフローのアップロード対象に private 構成を含むシステム世代や LLM モデルは含めません。

### 必要な設定

1. Cachix で公開 cache を作成します。
2. cache 限定書き込み token を GitHub Actions secret の `CACHIX_AUTH_TOKEN`、cache 名を Actions variable の `CACHIX_CACHE_NAME` に登録します（token を公開ファイルへ書かないでください）。
3. 利用者固有のCachix cache URLと公開鍵は、`dotfiles.local.toml` の `private.path` で結合する非公開側 `darwinModules.default` 内の `nix.settings.substituters` および `nix.settings.trusted-public-keys` に追加してください。その際、既定のNix公式キャッシュは保持したまま追記し、設定完了後に `dotfiles apply` を実行して変更を反映します。

CLIのNix derivationバージョンはcli/の内容ハッシュに基づくsource-...として定義し、システムのconfigurationRevisionと独立して管理します。CIではこのバージョンが.#dotfiles.versionと一致することを検証します。直接cargo buildを実行した場合は、cli/に最後に触れたコミットと未コミット変更の有無（dirty）を表示します。CLIキャッシュは、cli/のソースに加えflake.lock由来のtoolchain等ビルド入力が同一である場合に再利用します。cli/以外のコミット進行のみによる再コンパイルを抑止します。

完成済み出力がローカルNix storeにあれば出力を直接指定して一時GCルートで保持し、依存ビルド計画の再走査を避ける。未取得または保持に失敗する場合は同じ固定derivationから従来どおりキャッシュ取得やビルドを行い、入力やアクティブ世代の検証は省略しない。

個別のシステム世代はこのリポジトリのCIで公開しないため、最上位世代のallowSubstitutesをfalseに設定して外部キャッシュへの問い合わせを省略しています。CLI本体や依存パッケージのバイナリキャッシュ取得は維持されます。

公開・非公開・ローカル設定の入力が同一であればローカルのNix評価キャッシュを再利用し、入力変更時やキャッシュ削除時には再評価を行います。評価キャッシュの外部アップロードはなく、Cachixは公開CLI成果物の配信にのみ利用されます。copy計画の策定とapply直前の入力検証は毎回実施します。Homebrewの確認・反映は通常のシステム反映経路で行います。

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

macOS 27環境においてNix版OpenCode 1.18.13がコード署名不正によりSIGKILLで終了する事象に対応するため、設定検査の失敗時に終了コードおよびシグナル名を表示するよう改修するとともに、Darwin用ビルドで実行前に再署名を行い署名後のstripを停止する修正（[nixpkgs PR #550458](https://github.com/NixOS/nixpkgs/pull/550458)）をバックポート適用しました。

ローカルLLMの有効化とモデル指定はCLIの[localllm設定](../cli/README.md#localllm)を参照してください。設定が無効でも、開発・検証目的で`nix build .#localllm`を明示的に実行するとパッケージをビルドします。以前に取得したモデルデータは無効化だけでは削除されず、不要になったストアパスは後続のNixガベージコレクションで回収されます。

新しく導入されるモデルID `qwen3.8-9b-distill-4bit` は、Qwen3.5-9BをベースとしたEmperoの `Qwen3.8-9B-Distill` について、[配布元](https://huggingface.co/PocketAiHub/Qwen3.8-9B-MLX) が公開している4bit変換版を採用した非公式蒸留モデルです。動作環境は既存の `mlx-vlm` 0.7.0 を用い、取得時のバージョンおよびファイルハッシュはカタログで固定して管理されます。

利用するモデルの選択は、`dotfiles.local.toml` 内の `localllm.models` に1件のみを記述し、`localllm.default_model` にも同じIDを指定して行います。初回取得データ量の目安は新モデルが約6GB、引き続き選択可能な従来の `qwen3.8-27b-4bit` が約16GBです。なお、これらはストレージ取得時の容量であり、実際の運用時にはモデルの重みに加えて会話処理等の作業メモリが別途必要となります。

モデル推論バックエンドは `nix/packages.nix` が実装を所有し、[uv2nix](https://pyproject-nix.github.io/uv2nix/usage/getting-started.html) と `nix/localllm/uv.lock` により mlx-vlm 0.7.0 および MLX 0.32.2 の Metal 向け wheel に固定されており、実行時のパッケージ追加は行わない。動作検証としては、Apple Silicon 向けの qwen3.8-27b-4bit と qwen3.8-9b-distill-4bit の両モデルにおいて、合成した機密でない入力を用いて Metal 推論および OpenCode を通したファイル作成・読み取りの動作を確認しているほか、9B モデルでも実際に bash ツール経由でのファイル書き込みと読み取りが動作している。

サーバーはメモリを抑えるためKVキャッシュ4bit、prefill 64トークン、同時リクエスト1件とし、システムのGPUメモリ上限や常駐アプリの状態は変更しません。そのため、長い入力では応答までに数分かかることがあります。

`localllm chat` では、TUI入力欄へのログ混入を防ぐため、推論サーバーの標準出力および標準エラー出力を専用データ領域の `server.log` に書き込みます。ログファイルは起動ごとに上書きされ、起動時にそのパスが端末に表示されます。なお、単体起動の `localllm serve` では従来通り端末へ直接ログを出力します。

localllmのOpenCodeに通知する会話上限について、長文の早期要約を避けるためモデル設定に合わせて262,144トークンへと拡張しました。なお、1回あたりの回答上限は1,024トークンのまま維持しています。

localllm専用のprogress-instructions.mdをNixに同梱し、OpenCodeのinstructionsから追加で読み込む構成を追加しました。これにより、初回のツール呼び出し前における作業説明や有意な結果が得られた後の発見と次の手順の日本語報告、同一応答内でのツール呼び出し実行、および実際の結果に基づく作業報告を指示しています。

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

## 性能変更の検証

1. **ボトルネックの特定**：Issue等に記録された準備、差分、評価、ビルド、反映の各段階の所要時間から遅い工程を特定する。利用者報告の実測値、コードに基づく推論、未確認事項を明確に区別して扱う。
2. **比較条件の統一**：未変更での再実行、リソース内容のみの変更、CLI実装またはNix構成の変更の各条件を揃えて比較し、初回実行時とキャッシュ有効時を分けて計測する。
3. **測定結果の記録**：実行コマンド、一般的な環境情報、所要時間、実際の評価やビルド発生有無を記録する。計測前に向上率や閾値を断定せず、副作用を伴わない計測にはplan機能や隔離されたfixtureを活用する。
4. **振る舞いの担保**：短縮経路を通る場合でも、変更検出、入力やアクティブ世代の再検証、排他処理、失敗時の記録維持が正常に働くことをRustの振る舞いテストで検証する。表示文字列や外部呼び出し回数の検査のみで済ませない。

背景と計測事例の詳細は[#151](https://github.com/9sako6/dotfiles/issues/151)および[#152](https://github.com/9sako6/dotfiles/issues/152)を参照する。

[短縮経路の計測結果と再現手順](apply-performance.md)を記録している。

## 検証

変更した振る舞いはコマンドやスクリプトを用いて観測する。リポジトリ全体を検証する際はラッパーを挟まず、CI と同一のテストコマンドを直接実行する。

```sh
bun install --frozen-lockfile
bun run tsc --noEmit
bun test ./tests ./home/.apm/skills/anki/tools/*.test.ts
cargo fmt --check --manifest-path cli/Cargo.toml
cargo clippy --locked --manifest-path cli/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path cli/Cargo.toml
cargo test --locked --manifest-path cli/Cargo.toml --test zinit -- --ignored
cargo test --locked --manifest-path cli/Cargo.toml --test activation -- --ignored
cargo test --locked --manifest-path cli/Cargo.toml --bin dotfiles system::fast_path_tests::nix_generation_contains_the_inputs_used_by_the_copy_fast_path -- --ignored --exact
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
