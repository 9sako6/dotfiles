# 運用ガイド

設定・ツール、エージェント資源、macOSシステムを、それぞれのmiseタスクで反映する。リポジトリに移動して実行する。

| 変更したもの | 実行するコマンド |
|---|---|
| 通常の設定、コピー対象、ユーザーツール、シェルプラグイン | `mise run home:apply` |
| APMで管理する指示・スキル | `mise run agents:apply` |
| macOS、Homebrew、private Nix、Night Shift、AnkiConnect | `mise run system:apply` |

管理境界とバージョン固定の規則は[設計](repo-map.md)に従う。変更前に対象の宣言元を確認し、その領域のタスクだけを実行する。

## 初回セットアップ

Apple Silicon macOSで `./install.sh` を実行する。別の場所から取得する場合は、スクリプト全体を保存してから実行する。

Bootstrapはorigin/masterを取得し、固定miseとBunを導入して、home、agents、systemの順に反映する。既存checkoutに未コミット変更、別ブランチ、origin/masterから分岐したコミットがあれば停止する。成功後はmasterをorigin/masterへ接続する。

## 設定の編集とツール導入

通常の設定は `.mise.toml` の `[dotfiles]` からcheckoutへリンクする。編集はそのまま反映される。追加した配置やコピー対象を反映するときは `mise run home:apply` を実行する。プレビューは `mise dotfiles apply --dry-run` で確認できるが、コピー・ツール導入はこのプレビューに含まれない。

`.zsh.d` は `symlink-each` で、`home/.zsh.d` にあるファイルを個別にリンクする。Git管理外の `home/.zsh.d/local.zsh` と `home/.zsh.d/secrets.zsh` も、配置元にあれば `home:apply` で反映する。旧 `dist/` を参照するリンクも現在の配置元へ更新する。配置元にないファイルはリンクを作らず、配置先にだけあるファイルは保持する。

通常のCLIを更新するときは、バージョンとlockの変更を取り込んで `mise run home:apply` を実行する。ツール更新のためにsystem:applyを実行する必要はない。AWS CLIとNightlightは公式のビルド済み配布物をmiseで導入し、Nix側の依存を再コンパイルしない。

ツールの宣言元は `home/.config/mise/config.toml`、取得先の固定は同じディレクトリの `mise.lock`。home:applyはこの設定を明示して `mise install --locked` を実行する。バイナリはmiseのユーザー共有領域に入り、リポジトリ専用のツールディレクトリは作らない。ホームのmise設定で常用ツールを有効にし、各プロジェクトの設定では必要な版を選ぶ。

Blacksmithの組織トークン発行と使用量照会には[公式CLI](https://docs.blacksmith.sh/blacksmith-cli/overview)を使う。miseのレジストリに未登録のため、[公式installer](https://get.blacksmith.sh)と同じ配布元から、`http`バックエンドでバージョン指定のバイナリとSHA-256を取得する。0.5.2の配布とmacOS arm64での起動を2026-10-23に確認した。取得先とchecksumはmise.lockへ固定し、`BLACKSMITH_DISABLE_AUTO_UPDATE=1`でCLI自身の自動更新を無効にする。更新時は設定の完全なバージョンを変更し、`mise lock --global http:blacksmith --platform macos-arm64` を実行する。以前の版へ戻す場合も設定とlockを一緒に戻してhome:applyを実行する。公開バイナリのソースとの対応とbuild provenanceは未確認。

通常のツール導入は同時２件までとする。Rubyは `settings.ruby.compile = false` として配布済みバイナリの取得先とchecksumをlockへ固定する。APM用Pythonはツールの `python` オプションで3.13.18へ固定し、ビルド済み配布物を使う。Pythonツールの導入では `UV_NO_BUILD=1` を指定し、wheelがない依存を端末上でコンパイルしない。必要な配布物を取得できない場合は導入を失敗させ、ソースビルドへの切り替えで補わない。

シェルプラグインは同じmise設定の `[bootstrap.repos]` に40桁のコミットで固定する。home:applyで取得し、zshは取得済みのファイルを直接読む。シェル起動時にGit検査やダウンロードを行わない。

`dotfiles.toml` の `copy` に指定したものは実ファイルとして配置する。`.agents`、`.claude`、`.codex` 配下はagents:apply、それ以外はhome:applyが担当する。指定したディレクトリ全体を所有するため、コピー元から消した子要素は配置先からも消す。親や同階層のランタイムファイルは保持する。未変更ファイルのinodeとmtimeは保ち、変更ファイルは同じディレクトリ内でrenameして置き換える。

コピー元、配置先、配置先の親にsymlinkがある場合は停止する。ファイルとディレクトリの衝突も、管理元を確認してから解消する。`copy` とmiseのリンク宣言を同じパスに重ねない。

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

system:apply のNixビルドは `--max-jobs 1 --cores 2` を指定する。同時に１件をビルドし、各ビルドへ２並列を要求して、更新中のPCへの負荷を抑える。既存のNix設定が高い並列数でも、設定反映前の最初の実行からこの指定を使う。反映後に直接Nixを使う場合も、既定値は同時１件・各２並列とする。

`cores` はビルドスクリプトへの指定であり、CPU・メモリー使用量の厳密な制限ではない。また、再ビルドする部品数は減らない。ビルド済み配布物がない場合は、待ち時間が長くなることがある。ビルドが失敗した場合は、出力の末尾と最初に失敗したパッケージを確認する。成功した成果物は同じコマンドの再実行で再利用できる。並列設定の意味は [Lixの公式説明](https://docs.lix.systems/manual/lix/stable/advanced-topics/cores-vs-jobs.html)を参照する。

公開設定は `dotfiles.toml`。マシン固有の設定はGit管理外の `dotfiles.local.toml` に置く。`private.path` はローカル設定だけで指定し、公開checkoutからの相対パスも使える。

```toml
[private]
path = "../private-dotfiles"
```

private flakeは `darwinModules.default` を公開する。Home Managerのhome.file、packages、launchd、stateVersionとspecialArgsを利用できる。公開配置と重なるprivate home.fileはNix評価時に拒否する。ユーザーLaunchAgentはprivate Nix moduleのlaunchd宣言に置く。

`dotfiles configuration is invalid` が出た場合は、続くファイル名・キー名・理由を確認し、宣言元を修正する。エラーには設定値を含めない。システム用のキーは `copy` と `private.path` で、旧形式のキーがローカル設定に残っている場合も検査で停止する。

公開・privateともNixのGit入力を使う。追跡済みの未コミット編集は評価に入り、未追跡・無視されたファイルは入らない。新しいNixファイルはgit addしてから評価する。例外は、明示的に読み込むdotfiles.local.tomlだけ。ビルド中の宣言変更を固定する独自snapshotは持たないため、反映中に設定を編集しない。

Nixの補助資源はAnkiConnect。Nixのout-linkを状態ディレクトリの `dotfiles/current` に登録し、依存をGCから保持する。状態ディレクトリはXDG_STATE_HOME、未指定時はホームの `.local/state`。配置したリンクの参照先だけを `dotfiles/artifacts.json` に記録し、版変更では記録と一致するリンクを更新する。管理外のリンクへの置き換えや既存ファイルとの衝突は拒否する。Ankiのprofileやmediaは削除しない。

Nightlightはmise管理の公式Apple Siliconバイナリを使う。初回はhome:applyで導入してからsystem:applyを実行する。以前のNix版へのリンクは、所有記録と一致するものだけhome:applyで解除する。利用者が置き換えたリンクやファイルは保持する。

Night Shiftは `[settings.night_shift]` のstart、end、temperatureで指定する。ローカル設定では個々の値を上書きできる。時刻はHH:MM、temperatureは0〜100の整数。スケジュールと色温度を反映し、手動のON/OFFは変更しない。

途中で失敗したら原因を解消し、同じタスクを再実行する。system profile更新後にactivationが失敗した場合は部分変更が残る場合がある。直前の世代へ戻すには次を実行する。

```sh
mise run system:rollback
```

ロールバックはmacOSシステムを戻す。実体コピーした設定とエージェント資源は、戻したいcheckoutのhome:apply・agents:applyで復元する。

## FFmpegの配布と検証

FFmpegはconda-forgeのDarwin arm64配布を固定miseで導入する。`home/.config/mise/mise.lock` に本体と依存artifactのURL・SHA256を固定し、通常のapplyでは `mise install --locked` を使う。lockはmacOS arm64上の固定miseで生成する。Linuxからのcross-platform lockはmacOSのvirtual packageを解決できず、不完全なエントリを出力する場合があるため使用しない。

`tests/test_ffmpeg.py` は合成入力でH.264/AAC変換、ffprobe、全フレームのデコード、WAV/MP3音声抽出、PNG画像抽出、stream copyによるremuxを検証する。macOS CIではmise配布に対して実行し、必要な変換が満たされることを確認する。

```sh
DOTFILES_TEST_FFMPEG="$(mise which ffmpeg)" \
DOTFILES_TEST_FFPROBE="$(mise which ffprobe)" \
  python3 -m unittest discover -s tests -p test_ffmpeg.py -v
```

## Brotliのセキュリティ更新

[CVE-2026-111497](https://patchtheplanet.org/brotli)への対応として、Bunは1.4.4、Nixの共有BrotliライブラリとFFmpegのBrotli依存は1.2.1に固定する。Nixでは `nix/brotli-overlay.nix` を公開設定とprivate設定で共有する。上流のNixpkgsを更新するときは、このoverrideが引き続き必要か確認する。

マージ後は設定を取得し、次の順に反映する。以前のFFmpegを導入済みの場合も、固定依存を含めて確実に置き換える。

```sh
mise run home:apply
mise install --force --locked conda:ffmpeg
mise run system:apply
```

反映後は利用中のアプリとサービスを再起動する。サービスが古いNode.jsやBunの絶対パスを保持している場合は、宣言元の実行パスも更新する。シェルで選ばれる版を更新しただけでは、実行中のプロセスは変わらない。

Python wheelや `bun build --compile` の実行ファイルなど、Brotliを内部に含む配布物にはNixの共有ライブラリ更新は届かない。所有するプロジェクトで依存を更新し、再ビルド・再配布する。Node.jsの修正版は22.23.4、24.22.1、26.12.1で、Brotliの表示バージョンだけでは修正済みか判定しない。詳細は[Node.js公式告知](https://nodejs.org/en/blog/vulnerability/october-2026-security-releases)、[Bun公式告知](https://github.com/oven-sh/bun/security/advisories/GHSA-663w-cv8g-jj2p)、[Brotli公式リリース](https://github.com/google/brotli/releases/tag/v1.2.1)を参照する。

## 検証

コミット・push前の検査入口は `bin/check-for-agent`。dotfilesで固定したTypeScript・Bun・Gitを使い、型検査、既存テスト、公開文書と秘密情報の検査を実行する。隔離環境を作るテストへmiseの親SessionのPATH差分を持ち込まないよう、子プロセスでmiseのSession用変数を外す。Nixの宣言を変更した場合は、下記のNixビルドも実行する。

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

Bunのテストはコピーの更新・削除、未変更時の保持、symlinkの拒否、補助資源の所有確認、APM失敗後の再実行を検証する。通常の導入はPRのmacOS CIで実際のhome:apply・agents:applyを実行し、ツールの起動、シェルプラグイン、生成物の配備、FFmpegの変換を確認する。Nixはこれらの確認が終わってから導入する。Nixのテストはprivate合成、補助資源のGC rootを確認する。

test workflowはPRとmasterへのpushで実導入と振る舞いを検証する。public-system-buildは定期実行または手動実行で公開システム全体をビルドする。通常のツール導入はtest workflowだけに置き、二重に実行しない。CIではmacOS設定をactivateしない。
