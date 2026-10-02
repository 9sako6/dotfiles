# 設計

## 管理境界

ファイルは次の 6 区分で管理する。共有可能な設定と非公開にすべき情報を同一リポジトリに混在させず、かつ単一の構成ルートから安全に組み立てるための境界である。

- `repo runtime` — この repo 自身を動かすために必要なファイル。home directory には配備しない。
- `home-managed user tools` — 普通のCLIは `home/.config/mise/config.toml` に宣言し、miseで固定する。Nightlight、AnkiConnect、localllmはNix固有artifactとしてRustがrootlessに配備する。共有設定ファイルの実体は `home/` に置き、public live symlink と copy は Rust CLI が配備する。devcontainerから実ファイルとして見える必要があるものだけ `dotfiles.toml` の `copy` で配備する。
- `system configuration` — 公開ルートの `flake.nix` / `flake.lock` と `nix/system.nix` に Mac 全体の設定を置く。nix-darwin で反映し、Homebrew 本体と cask もここで管理する。公開 `flake.nix` が唯一の構成ルートである。
- `private system configuration` — 公開できない追加設定。private側は独立したGit checkout / flakeを維持し、ローカル設定 `dotfiles.local.toml` の `private.path` 経由で公開 root flake の評価時に結合する。
- `local-only` — マシン固有の設定。repo にコミットせず、Git 管理外の `dotfiles.local.toml` や各マシンのローカルファイルに置く。機密情報は含めず、必要に応じて個別にバックアップする。
- `secrets` — 認証情報や鍵などの機密。repo、`home/`、および `dotfiles.local.toml` のいずれにも入れない（最終判断はユーザーが行う）。

Nix の実現手段ごとにトップレベルディレクトリを分けない。`nix/` を Nix 宣言の単一入口とし、`default.nix` が全体を組み立て、`home.nix` / `packages.nix` / `system.nix` が責務を分担する。

### 置き場所の判断

- repo の実行だけに必要 → `repo runtime`
- public live symlink / copy の home 配置 → Rust CLI。private Home Manager 構成は従来どおり Nix に残す
- ユーザー単位で常設する普通のCLI → `home/.config/mise/config.toml`
- Nix固有artifact → `nix/packages.nix`
- ユーザーへ配る共有設定ファイルの実体 → `home/`
- Mac 全体へ反映したい公開設定 → `system configuration` (`flake.nix` / `nix/system.nix`)
- Mac 全体へ反映したい非公開設定 → `private system configuration` (`dotfiles.local.toml` の `private.path` で指定)
- マシン固有の非機密設定 → `local-only` (`dotfiles.local.toml` 等)
- 機密情報 → `secrets`（リポジトリおよびローカル TOML の外で管理）

## Bootstrap と設計の原則

### 変更を設計するときの判断基準

- **必要性の確認**：機能追加前に、解消する具体的問題と既存機構で不足する点を確認する。TOML等の構造化設定は正式パーサーで扱い、空白や引用符の揺らぎを独自正規表現で処理したり、手書きの第二正本を増やしたりしない。
- **反映範囲の局所化**：リソース配備等の単機能更新は必要な処理だけで完結できる構造を目指し、無関係なCLI再コンパイルやシステム全体の評価、ビルド、activationを要求しない。処理の省略は入力や世代の検証と排他を担保した上で安全に判断する。
- **依存範囲の分離**：CLIコード、Nix構成、配備対象データの依存関係を明確に分ける。リポジトリのコミット進行のみを理由にCLIやシステム全体の再ビルドを発生させず、コミット追跡の表示と再ビルド判定は独立して設計する。
- **性能改善の順序**：キャッシュ追加の前に、不要な評価、ビルド、外部プロセス起動の削減を先行させる。キャッシュ適用時のみならず、非適用時も含めた実測で速度向上が確認できるまで完了としない。

### 構成ルートと設定合成

利用者向けの設定項目、既定値、記述例はCLIの[設定ファイル](../cli/README.md#設定ファイル)を参照してください。

- 公開リポジトリ直下の `flake.nix` が唯一の構成ルートである。system構成だけを公開rootから評価する。日常のpublic home/tools/services/settingsはRust/miseが扱う。
- 構成設定は、リポジトリ共有の `dotfiles.toml` と、Git 管理外の `dotfiles.local.toml` の 2 つで管理する。
- 通常コマンドはRustのTOMLパーサーで設定を検証する。Nixへ進む場合は既存の設定スキーマでも検証し、両者の受理・拒否とマージ結果を同じ入力例で照合する。モデルカタログは `nix/localllm/catalog.json` を共通入力にする。エラーに設定値を含めない。
- 設定テーブルのマージは、「既定値 < 公開共有 (`dotfiles.toml`) < ローカル (`dotfiles.local.toml`)」の順序で再帰的に行われる。
- 配列およびスカラー値はマージされず、`false` や空配列 `[]` を含め、上位の設定で完全に置換される。
- `copy` セクションは共有の `dotfiles.toml` でのみ定義可能とし、ローカルファイルでの指定は拒否される。指定するパスは相対パス表記であり、重複がなく、アルファベット順に整列され、かつ相互に包含関係を持たないものでなければならない。
- `private` セクションはローカルの `dotfiles.local.toml` でのみ定義可能である。`private.path` は公開リポジトリルートからの相対パスまたは絶対パス入力を受け付ける（文書内では可搬な相対パスで表現する）。
- `dotfiles.local.toml` は機密情報を含んではならず、個別にバックアップする。不在時は既定値が適用され、非公開パスの探索は行われない。読み取り不可や構文不正がある場合は処理を停止する。

### CLI と評価・反映の整合性

コマンドの構文、オプション、実行例は[CLIのUsage](../cli/README.md#usage)を参照してください。

- `bin/` は直接実行するリポジトリの入口を配置し、`lib/` は各入口が利用する内部実装を受け持つ。
- **Rust CLI**：新規のdotfiles固有の状態判断、排他、反映制御、記録更新とそのテストを所有する。shellは初回導入と薄い外部プロセス接続、Nixは構成宣言に絞る。既存実装の一括移行は不要であり、変更対象の範囲から順次寄せる。依存関係は標準ライブラリおよび既存のRust依存を優先し、必要なクレートは根拠を示して追加できる。OS標準や既存導入済み、新規Cargo依存を回避できることのみを理由にPerl、Python、Bunなどの別環境へ新規実装を分散させない。
- `install.sh` は新しい Mac をセットアップする唯一の入口とし、miseの固定Rust、rootless CLI、通常apply、repositoriesの順序で実行を制御する。
- `dotfiles` CLI は日常的な system の plan / apply に使う。リポジトリのテスト実行制御は CLI に持たせず、CI では各テストコマンドを直接実行する。
- `dotfiles` CLI は、追跡対象の未コミット変更のスナップショットとローカル設定入力を記録してビルドを行う。チェックアウト全体を無条件に参照する `path:.` は使用せず、ローカルファイルを Git に強制追加することもない。
- `plan` および `apply` の実行中に、自動的な `git clone`、`git pull`、または依存固定ファイルの更新は一切行わない。
- ローカル設定による非純粋性は、明示的かつ一時的なマニフェストとして検証済み Nix エントリへ渡す箇所に限定し、公開 CI 環境は完全に純粋に保つ。
- RustがGit追跡ファイルの内容・mode・symlinkを一時領域へ固定し、前後の変更を検査する。未追跡ファイルは含めない。privateの `flake.lock` はコミット済みかつ未変更でなければならない。コピーした固定ソース自体も確認後に再検証する。
- system入力は `flake.nix`、`flake.lock` と `nix/` 内のsystemに必要な10ファイルに限定する。対象一覧は `cli/src/system/inputs.rs` が所有する。CLI、home、mise、user-services、user-settings、artifact実装はこの入力に含めない。制限したソースだけでprivate Home Managerを含むhostを評価できることをcomposition testで検証する。
- `dotfiles-system-inputs` をactive generationから読み、Nixを起動する前にsystem変更を判定する。キーはsystemソース、privateの追跡内容、checkout位置、ユーザー、HOME。privateがある場合は、private moduleが参照し得るマージ済み設定全体も含める。この場合のcopy/localllm宣言変更は保守的にsystem変更として扱う。単なるコミット進行、CLIやhomeの内容変更はキーに含めない。
- artifactはsystemと別に、固定recipeと設定のidentity、manifest、登録済みGC rootを照合する。成功した評価結果は `artifact-evaluation.json` に保存するが、homeの所有証明には使わない。欠損・破損・root消失時は再評価し、通常のhome/tools/service変更で評価を繰り返さない。
- 一つのPlanを一度表示し、applyだけが一度yesを確認する。確認後はartifactの必要なrealization、home、tools、artifact配置、user settings、user servicesの順に進む。system変更があるときだけ、その後にsystemをbuildし、入力を再確認してactivation直前にsudoを使う。system activationはrootless CLIの内部操作で行い、世代内CLIやhomeの再計画に依存しない。
- planは配置、ツール導入、service操作、設定変更、成功記録の更新をしない。古いinventoryから移行するときもsystemを事前buildせず、予定世代と宣言差分を表示する。Nixが必要なcold pathでは固定inputの取得・store登録・評価が発生する。
- system identityとartifactの固定結果が有効なら、no-op、tool-only、home-only、service-only、CLI-onlyでNixプロセスを起動しない。artifact cacheの再構築ではrootless Nixを使うが、systemが同じならnix-darwinとsudoは呼ばない。
- public Home Manager userは定義しない。Home Manager moduleとprivate用sharedModuleは残し、private側が定義するhome.file、packages、launchd、stateVersionを維持する。public rootless対象とprivate所有の衝突は変更前に拒否する。privateからpublic homeの実体をsystem入力として読む方式は、以前からのprojected sourceの境界外である。
- 排他ロックには安定した共通ファイルを用い、親プロセス終了後も継承したファイルディスクリプタを保持する子の反映プロセスが生存している間はロックを維持する。ソース記録の更新は反映成功時のみ行う。
- 反映結果を記録するシステム側のシンボリックリンクは、反映結果の記録としてのみ更新され、選択状態を操作するためのスイッチとしては使用しない。
- 反映が失敗した場合、結果記録シンボリックリンクは直前の正常世代を指したまま保持されるが、システム、ホーム、Homebrew が部分的に変更された状態になる可能性がある。ロールバックは `sudo darwin-rebuild switch --rollback` または `mise run system:rollback` で行い、ホームの実体ファイルは過去のコミット済みチェックアウトから明示的に復元する。検証が完了するまで古い固定ファイルやキャッシュは保持する。

### ローカル LLM と OpenCode

- Apple Silicon向けローカル推論モデル `qwen3.8-27b-4bit`、`qwen3.8-9b-distill-4bit` の提供
- パッケージ依存関係は [uv2nix](https://pyproject-nix.github.io/uv2nix/usage/getting-started.html) および `nix/localllm/uv.lock` を通じて `mlx-vlm 0.7.0` および Metal 向け wheel `mlx 0.32.2` に厳密に固定し、`nix/packages.nix` が実装を所有する。実行時の動的なパッケージ導入は行わない。
- ローカルモデルが有効化されている場合、ソートされた一意の既知モデル ID リストが定義され、起動時には単一のモデルのみがロードされ、`default_model` はモデルリスト内に存在しなければならない。
- 無効化（`enabled = false`）された構成一式には LLM 固有の依存関係は含まれず、使用されなくなったモデルデータは後続の GC で回収される（共有依存関係は保持される）。
- ランチャーには `localllm chat -- <opencode-arguments>`、`localllm serve`、`localllm check` を用意する。常駐デーモンは持たず、オンデマンドで単一の所有プロセスツリーとして起動し、ローカル接続用のアドレスにバインドする。初回ビルド時には数 GB のモデルデータ取得が発生し得る旨が事前に通知される。
- `localllm check` は Metal 演算のみを確認する。
- OpenCode 1.18.13をベースとし、専用のXDG設定および履歴領域を割り当てています。Nix管理下のGoalプラグインのみを有効化し、外部プラグイン、外部スキル、ユーザー定義MCP、会話共有機能、外部プロバイダへの自動切替はすべて無効化しています。クライアントは起動元のPATH、HOME、ツール用環境変数を継承しつつOpenCode固有の環境変数を専用設定で置換し、推論サーバーは最小限の独立環境として分離・運用しています。
- 推論サーバーは `sandbox-exec` により外向き通信を遮断し、ローカル接続専用の構成を維持します。`OpenCode` 本体および起動される子コマンドはネットワーク接続が可能で、`webfetch` および `websearch` を許可しています。今回のランチャーにより `OPENCODE_ENABLE_EXA=1` が設定され、APIキー不要の[組み込みExa検索](https://opencode.ai/docs/tools/#websearch)が自動的に有効になります。推論はローカルで実行しますが、検索語やアクセス先URL、ツール経由で送信する内容は外部へ送信されます。完全な通信遮断状態ではありません。
- 自動テストでは、ローカルサーバー不在時における適切な失敗、クラウド推論への自動切り替えが発生しないこと、ユーザーPATH上のコマンド実行とHOME環境の引き継ぎ、Web取得および検索ツールの利用可能性、推論サーバーの外向き通信遮断を検証します。なお、CI上では巨大モデルの展開やGPUによる推論実行は行いません。

### パッケージと環境の原則

- 普通のCLIやツールチェーンは、バージョンを固定して `home/.config/mise/config.toml` の `[tools]` で管理する。移行時はmise backendの公式配布元と実行動作を確認し、同じツールのNix宣言を除く。GUI、system service、platform依存、Nix固有artifactは別の管理境界として扱う。
- FFmpegはmiseの `conda:ffmpeg` で固定する。旧Nix版との差分と検証範囲は[運用ガイド](operations.md#ffmpegの配布と検証)に記録する。NightlightはmacOS専用helper、AnkiConnectとlocalllmはNix固有artifactとして扱う。Anki GUI 26.05はsystem側のNix packageで維持する。CIやbootstrapが使うNix toolsetの依存も、各ツールを移行する前に確認する。private moduleの既存interfaceとHome Managerの利用方式は維持する。
- 編集内容を即座に反映させたい通常の設定ファイルは、稼働中のリポジトリへの直接のシンボリックリンクとする。
- devcontainer から参照するエージェント用設定は、Nix ストアやホスト固有の絶対パスシンボリックリンクにしてはならない。`dotfiles.toml` の `copy` に列挙したファイルまたはディレクトリのみを、実ファイルとして `$HOME` 配下に配備する。列挙されたディレクトリ配下は dotfiles が所有し、同期時にはコピー元に存在しない子要素を削除するが、親ディレクトリや同階層にある他のランタイムファイルには影響を与えない。
- `copy`宣言されたパスと一致または親子関係にある公開構成のHome Managerリンクのみを自動除外する。その他の有効な`home.file.target`が`copy`対象と重複した場合は、別名キー経由の`target`指定も含めてNix評価時に拒否する。
- copy処理の設計：プレビュー処理と同一のfingerprint（内容およびファイルモード）比較により、未変更ファイルとサブディレクトリへの書き込みを省いてinodeとmtimeを保持する。変更対象のファイルは同一ディレクトリの一時ファイルへ内容とモードを設定したのち、rename操作によって置換する。
- public live symlink と copy は Rust CLI が所有し、private Home Manager構成は従来のNix合成を維持する。`dotfiles.toml` によるファイルコピーは devcontainer の環境境界を越えるための限定的な例外措置である。public copyの前回成功結果は `$XDG_STATE_HOME/dotfiles/home.json`（未指定時は `$HOME/.local/state/dotfiles/home.json`）に記録する。これは前回の所有確認にだけ使い、desired stateの正本は引き続きrepoの宣言とする。
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

Nix でユーザー常設ツールを管理する際は、`flake.lock` による取得元リビジョンの固定にとどまらず、`nix/packages.nix` 内で期待するバージョンを明示してアサーションを行う。利用可能な場合はバージョン付き属性を選択し、コメントにも完全なバージョン番号を記載する。nixpkgs の更新によって実際のバージョンが変わった場合は、期待バージョンを意図的に更新するまで評価を失敗させる。

Homebrewのformulaとcaskは `nix/homebrew-packages.nix` に集約する。普通のCLIの管理方式は[パッケージと環境の原則](#パッケージと環境の原則)に従う。miseで管理できない配布形式や実行時の制約がある場合は、その根拠を確認して管理境界を決める。

`install.sh` は各種バージョン管理ツールの導入前に実行される。そのため、自身が属するコミットの SHA をスクリプト内に既定値として埋め込まず、実行時に `origin/master` の最新コミットを取得する。なお、既存のローカルチェックアウトに変更がある場合、別ブランチにいる場合、または `origin/master` から分岐したコミットが存在する場合は自動更新を行わない。

### Public user LaunchAgent の宣言と反映

`user-services.toml` は public user LaunchAgent の唯一の宣言元で、Rust CLI が同じ公開 snapshot から読み、plist と追加・変更・削除の差分を生成する。system input へは含めない。Rust CLI は確認済み Plan の plist、所有記録、launchd の登録状態を再検証して反映する。サービスの前提となる tools と home を先に配備し、system変更を伴う場合もservice反映後にsystem build/activationへ進む。各rootless工程の前後で入力と現在の世代を確認し、activation成功後は更新先の世代とsource recordを検証する。

`[[agents]]` の必須項目は `label` と `argv`。任意項目は `run_at_load`、`keep_alive`（既定 false）、正の `start_interval`、`start_calendar_interval`（minute/hour/day/weekday/month）、`working_directory`。interval と calendar は同時指定できない。未知のキー、大文字小文字のみ異なるものを含む重複 label、空 argv、制御文字、不正な schedule/path は拒否する。raw shell hook や任意の plist key は受け付けない。これは実行ファイルの sandbox ではない。

`argv[0]` は絶対パスまたは `~/` で始まる安定した実行入口を明記する。mise tool は [mise shims](https://mise.jdx.dev/dev-tools/shims.html) を使い、標準構成なら `~/.local/share/mise/shims/node` 等を指定する。`MISE_SHIMS_DIR` / `shims_dir` / data directory を変更した環境では、利用者がその構成に対応した実際の shim path を指定する。CLI は bare command から shim path を推測しない。mise install の版固定パス、Nix store の実行パス、shell interpreter と env の直接指定は拒否する。`~/` 展開は実行入口と working directory のみで、他の argv 要素に shell 展開は行わない。shim は作業ディレクトリに対応した mise 設定を選ぶので、必要に応じて working directory を指定する。

前回成功結果は `$XDG_STATE_HOME/dotfiles/user-services.json`（既定 `~/.local/state/dotfiles/user-services.json`）の version 1、home、agents（label → plist SHA-256）の記録として読む。宣言や起動指示は記録しない。差分は宣言 label と記録済み label のみに限定し、LaunchAgents を走査して所有を推測しない。記録がない既存 plist、記録と内容が異なる plist、配備先や記録の symlink、破損記録は競合として停止する。HOME、公開 snapshot root、明示した XDG_STATE_HOME 自体の OS alias は許容し、その配下の symlink は拒否する。記録を失った場合、既存 agent を自動採用・削除せず手動で所有を確認する。欠損した managed plist は再作成差分となる。

launchctl の対象は実行ユーザーの `gui/<euid>` に限定する。`print gui/<euid>/<label>` の登録元が canonical HOME 配下の対象 plist と一致し、種別が LaunchAgent と確認できる場合だけ登録済みとして扱う。以前の plist 所有記録もある場合に限って `bootout gui/<euid>/<label>` を実行する。登録がないと判断するには、status 113、対象 label と uid が一致する既知の missing-service 診断、GUI domain の `print` 成功をすべて要求する。その他のエラーや未知の出力形式は停止条件となる。PID、起動回数、実行中か待機中かは差分に含めない。

plist と所有記録は同じディレクトリ内の一時ファイルから rename する。plist の配備結果を label ごとに記録してから bootstrap するため、起動失敗後もそのファイルの所有を追跡できる。記録は起動成功やプロセスの健全性を保証しない。記録の更新に失敗したときは、CLI が書いた bytes とまだ一致する plist だけを元に戻し、bootstrap は行わない。削除後の記録更新に失敗した場合は旧記録を残し、次回 apply で削除結果を記録する。launchctl が失敗を返した場合は、部分的に反映されていても停止し、次の Plan で登録状態を読み直す。

共通ユーザー apply ロックを launchctl の子プロセスにも継承する。ただし、このロックに従わない別のツールや利用者との排他は保証しない。ファイル確認、rename、launchctl 呼び出しを一つのトランザクションにはできず、直前検証後の競合や、同じ path を使うジョブの外部での差し替えを完全には判別できない。プロセス終了や電源断が plist 書き込みと記録更新の間に起きると、所有記録のない plist が残る場合がある。その場合は自動採用せず停止する。

private agent、system daemon、現在の Nix-backed zundamonotify は対象外とし、既存の構成・所有を変更しない。


### Nix artifact の所有と GC root

`nix/artifacts.nix` をAnkiConnect、Nightlight、有効時のlocalllmの選択元とし、固定・検証済み設定から manifest と package 参照を生成する。通常の CLI、GUI、private module はこの backend に取り込まない。Rust CLI は同じ公開 snapshot の artifact Plan を確認後に realization し、永続 Nix root を登録してから link を配備する。配備先は AnkiConnect の従来 target と `~/.local/bin/localllm`。public Home Manager userは定義しない。private frameworkを残し、Night ShiftはRust、Anki GUIはsystem側Nixが所有する。

copy 優先、desired private HM target の非重複、実際の HM package profile の executable 検査を維持する。旧 HM 配置の採用は有効な宣言と選択ソースの一致を必要とする。artifact の前回結果と copy 引継ぎの観測 receipt は専用記録に置き、宣言や起動指示の第二正本にしない。親プロセスが captured Plan を扱い、世代内 CLI に再計画させない。

登録 root は `$XDG_STATE_HOME/dotfiles/artifact-roots/`、結果は `dotfiles/artifacts.json`。旧 root を保持し、この段階では自動 GC や root pruning を行わない。artifact-only変更はsystem identityから除外し、Darwin activationを呼ばない。旧inventoryでも確認前のsystem buildはしない。
