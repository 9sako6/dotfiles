# 運用ガイド

設定・ツール、エージェント資源、macOSシステムを、それぞれのmiseタスクで反映する。リポジトリに移動して実行する。

| 変更したもの | 実行するコマンド |
|---|---|
| 通常の設定、コピー対象、ユーザーツール、シェルプラグイン | `mise run home:apply` |
| APMで管理する指示・スキル | `mise run agents:apply` |
| macOS、Homebrew、private Nix、Night Shift、AnkiConnect、localllm | `mise run system:apply` |

管理境界とバージョン固定の規則は[設計](repo-map.md)に従う。変更前に対象の宣言元を確認し、その領域のタスクだけを実行する。

## 初回セットアップ

Apple Silicon macOSで `./install.sh` を実行する。別の場所から取得する場合は、スクリプト全体を保存してから実行する。

Bootstrapはorigin/masterを取得し、固定miseとBunを導入して、home、agents、systemの順に反映する。既存checkoutに未コミット変更、別ブランチ、origin/masterから分岐したコミットがあれば停止する。成功後はmasterをorigin/masterへ接続する。

既存環境の移行では、固定miseを確認してから同じ3つのタスクを順に実行する。旧dotfilesコマンドは使わない。READMEとcli/READMEは旧構成の資料として残しているため、現在の手順は本書を参照する。

## 設定の編集とツール導入

通常の設定は `.mise.toml` の `[dotfiles]` からcheckoutへリンクする。編集はそのまま反映される。追加した配置やコピー対象を反映するときは `mise run home:apply` を実行する。プレビューは `mise dotfiles apply --dry-run` で確認できるが、コピー・ツール導入はこのプレビューに含まれない。

ツールの宣言元は `home/.config/mise/config.toml`、取得先の固定は同じディレクトリの `mise.lock`。home:applyはこの設定を明示して `mise install --locked` を実行する。バイナリはmiseのユーザー共有領域に入り、リポジトリ専用のツールディレクトリは作らない。ホームのmise設定で常用ツールを有効にし、各プロジェクトの設定では必要な版を選ぶ。

シェルプラグインは同じmise設定の `[bootstrap.repos]` に40桁のコミットで固定する。home:applyで取得し、zshは取得済みのファイルを直接読む。シェル起動時にGit検査やダウンロードを行わない。

`dotfiles.toml` の `copy` に指定したものは実ファイルとして配置する。`.agents`、`.claude`、`.codex` 配下はagents:apply、それ以外はhome:applyが担当する。指定したディレクトリ全体を所有するため、コピー元から消した子要素は配置先からも消す。親や同階層のランタイムファイルは保持する。未変更ファイルのinodeとmtimeは保ち、変更ファイルは同じディレクトリ内でrenameして置き換える。

コピー元にsymlinkがある場合や、配置先の親がsymlinkの場合は停止する。checkoutの同じコピー元を指すリンクは実体へ移行できる。それ以外のリンクやファイルとディレクトリの衝突は、管理元を確認してから解消する。`copy` とmiseのリンク宣言を同じパスに重ねない。

miseは宣言から外した古いリンクを削除しない。コピー対象そのものを宣言から外した場合も配置済みファイルは残す。不要になった配置は所有を確認して手動で削除する。

## エージェント資源

`mise run agents:apply` はhomeディレクトリのAPMプロジェクトで `apm install --frozen --only apm`、`apm compile --clean`、生成物のコピーを順に実行する。生成に失敗した場合はホームへコピーしない。

依存を変更するときは次のタスクを使う。APMの変更、再生成、配備までをまとめて行う。

```sh
mise run agents:install owner/repository
mise run agents:uninstall owner/repository
mise run agents:update
```

生成元は `home/.apm/`、依存宣言とlockは `home/apm.yml` と `home/apm.lock.yaml`。生成された指示やスキルを直接編集しない。AGENTS.mdの再生成もAPMに任せる。

## システム反映とprivate設定

`mise run system:apply` はログインユーザーとして実行する。必要なら固定Lixを導入し、Nixでシステムと補助資源をビルドする。補助資源とNight Shiftを反映した後、sudoで標準system profileを更新し、生成されたnix-darwinをactivateする。home:applyとagents:applyはNixやsudoを起動しない。

公開設定は `dotfiles.toml`。マシン固有の設定はGit管理外の `dotfiles.local.toml` に置く。`private.path` はローカル設定だけで指定し、公開checkoutからの相対パスも使える。

```toml
[private]
path = "../private-dotfiles"
```

private flakeは `darwinModules.default` を公開する。既存のHome Managerのhome.file、packages、launchd、stateVersionとspecialArgsを維持する。公開配置と重なるprivate home.fileはNix評価時に拒否する。ユーザーLaunchAgentはprivate Nix moduleのlaunchd宣言に置く。旧 `[services.agents]` に宣言が残っていればsystem:applyは停止する。

公開・privateともNixのGit入力を使う。追跡済みの未コミット編集は評価に入り、未追跡・無視されたファイルは入らない。新しいNixファイルはgit addしてから評価する。例外は、明示的に読み込むdotfiles.local.tomlだけ。ビルド中の宣言変更を固定する独自snapshotは持たないため、反映中に設定を編集しない。

補助資源はNightlight、AnkiConnect、有効時のlocalllm。Nixのout-linkを状態ディレクトリの `dotfiles/current` に登録し、依存をGCから保持する。状態ディレクトリはXDG_STATE_HOME、未指定時はホームの `.local/state`。配置したリンクの参照先だけを `dotfiles/artifacts.json` に記録する。版変更やlocalllmの無効化では、記録と一致するリンクだけを更新・削除する。同じパッケージへ解決する旧Home Managerリンクも移行できる。Ankiのprofileやmedia、LLMの履歴は削除しない。

Night Shiftは `[settings.night_shift]` のstart、end、temperatureで指定する。ローカル設定では個々の値を上書きできる。時刻はHH:MM、temperatureは0〜100の整数。スケジュールと色温度を反映し、手動のON/OFFは変更しない。

途中で失敗したら原因を解消し、同じタスクを再実行する。system profile更新後にactivationが失敗した場合は部分変更が残る場合がある。直前の世代へ戻すには次を実行する。

```sh
mise run system:rollback
```

ロールバックはmacOSシステムを戻す。実体コピーした設定とエージェント資源は、戻したいcheckoutのhome:apply・agents:applyで復元する。

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

miseの `conda:ffmpeg` で `FFmpeg 8.1.2` を固定し、`ffmpeg`、`ffplay`、`ffprobe` を提供します。`mise run home:apply` の実行後は `localllm chat` 上から通常のコマンド名でそのまま実行でき、他の既存コマンドと同様に `PATH` 経由で連携して利用できます。

- **実行コマンド:**
  - `localllm chat -- <opencode-arguments>` (プロンプト実行引数を渡す)
  - `localllm serve` (推論サーバーの単独起動)
  - `localllm check` (Metal 演算のみを確認)
- **実行特性:**
  - 常駐デーモンは存在せず、オンデマンドで単一の所有プロセスツリーとして起動する。
  - 外部ネットワークからの接続を受け付けないローカル接続用のアドレスにバインドする。
- **リソースの回収:**
  - `localllm` を無効化（`enabled = false`）した構成一式には、LLM 固有の依存関係は一切含まれない。
  - 使用されなくなったモデルデータは後続の Nix ガベージコレクションによって削除されるが、他の構成と共有されている基本依存関係は維持される。
- **OpenCode 統合とセキュリティ制限:**
  - OpenCode 1.18.13をベースとし、専用のXDG設定および履歴領域を割り当てています。Nix管理下のGoalプラグインのみを有効化し、外部プラグイン、外部スキル、ユーザー定義MCP、会話共有機能、外部プロバイダへの自動切替はすべて無効化しています。クライアントは起動元のPATH、HOME、ツール用環境変数を継承しつつOpenCode固有の環境変数を専用設定で置換し、推論サーバーは最小限の独立環境として分離・運用しています。
  - 推論サーバーは `sandbox-exec` により外向き通信を遮断し、ローカル接続専用の構成を維持します。`OpenCode` 本体および起動される子コマンドはネットワーク接続が可能で、`webfetch` および `websearch` を許可しています。今回のランチャーにより `OPENCODE_ENABLE_EXA=1` が設定され、APIキー不要の[組み込みExa検索](https://opencode.ai/docs/tools/#websearch)が自動的に有効になります。推論はローカルで実行しますが、検索語やアクセス先URL、ツール経由で送信する内容は外部へ送信されます。完全な通信遮断状態ではありません。
  - 自動テストでは、ローカルサーバー不在時における適切な失敗、クラウド推論への自動切り替えが発生しないこと、ユーザーPATH上のコマンド実行とHOME環境の引き継ぎ、Web取得および検索ツールの利用可能性、推論サーバーの外向き通信遮断を検証します。なお、CI上では巨大モデルの展開やGPUによる推論実行は行いません。

## FFmpegの配布と検証

FFmpegはconda-forgeのDarwin arm64配布をmise 2026.7.7で導入する。`home/.config/mise/mise.lock` に本体と依存artifactのURL・SHA256を固定し、通常のapplyでは `mise install --locked` を使う。lockはmacOS arm64上の固定miseで生成する。Linuxからのcross-platform lockはmacOSのvirtual packageを解決できず、不完全なエントリを出力する場合があるため使用しない。

旧Nix版にあったSRT/RISTと外部Theora/Speex/Xvidエンコーダは、未使用の追加機能として今回の移行で省く。これらを指定するライブ転送や書き出しは対応範囲に含めない。内蔵デコーダの有無と外部エンコーダの有無は別であり、全形式の互換性を保証するものではない。

`tests/test_ffmpeg.py` は合成入力でH.264/AAC変換、ffprobe、全フレームのデコード、WAV/MP3音声抽出、PNG画像抽出、stream copyによるremuxを検証する。macOS CIでは同じ試験を固定nixpkgsの旧配布とmise配布へそれぞれ実行する。ffplayは起動可能な版の確認だけを行い、GUI再生やハードウェアアクセラレーションは試験しない。

```sh
DOTFILES_TEST_FFMPEG="$(mise which ffmpeg)" \
DOTFILES_TEST_FFPLAY="$(mise which ffplay)" \
DOTFILES_TEST_FFPROBE="$(mise which ffprobe)" \
  python3 -m unittest discover -s tests -p test_ffmpeg.py -v
```

## 検証

```sh
bun install --frozen-lockfile
bun run tsc --noEmit
bun test ./tests ./home/.apm/skills/anki/tools/*.test.ts
nix build --no-link --no-update-lock-file --no-write-lock-file \
  .#checks.aarch64-darwin.artifacts \
  .#checks.aarch64-darwin.composition \
  .#checks.aarch64-darwin.configuration
python3 -m unittest discover -s nix/tests -p 'test_*.py'
```

Bunのテストはコピーの更新・削除、未変更時の保持、symlinkの拒否、補助資源の所有確認、APM失敗後の再実行を検証する。実際のmiseタスクも隔離したホームで実行し、NixとRustが不要なことを確認する。Nixのテストはprivate合成、無効なLLM依存の除外、補助資源のGC rootを確認する。

push時のtest workflowに加え、public-install-smokeを手動実行すると、固定ツールの導入、APMの生成・配備、シェルプラグイン、公開システムのビルドをmacOSで確認できる。CIでは実機のmacOS設定をactivateせず、巨大モデルの推論も実行しない。
