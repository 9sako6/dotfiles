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

ツールの宣言元は `home/.config/mise/config.toml`、取得先の固定は同じディレクトリの `mise.lock`。home:applyはこの設定を明示して `mise install --locked` を実行する。バイナリはmiseのユーザー共有領域に入り、リポジトリ専用のツールディレクトリは作らない。ホームのmise設定で常用ツールを有効にし、各プロジェクトの設定では必要な版を選ぶ。

Rubyは `settings.ruby.compile = true` でソースからの導入を明示し、lockに保存した取得先と一致させる。mise 2026.8以降はビルド済みバイナリが既定になるため、導入方式を省略すると既存のlockを使えなくなる。

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

公開設定は `dotfiles.toml`。マシン固有の設定はGit管理外の `dotfiles.local.toml` に置く。`private.path` はローカル設定だけで指定し、公開checkoutからの相対パスも使える。

```toml
[private]
path = "../private-dotfiles"
```

private flakeは `darwinModules.default` を公開する。Home Managerのhome.file、packages、launchd、stateVersionとspecialArgsを利用できる。公開配置と重なるprivate home.fileはNix評価時に拒否する。ユーザーLaunchAgentはprivate Nix moduleのlaunchd宣言に置く。

`dotfiles configuration is invalid` が出た場合は、続くファイル名・キー名・理由を確認し、宣言元を修正する。エラーには設定値を含めない。システム用のキーは `copy` と `private.path` で、旧形式のキーがローカル設定に残っている場合も検査で停止する。

公開・privateともNixのGit入力を使う。追跡済みの未コミット編集は評価に入り、未追跡・無視されたファイルは入らない。新しいNixファイルはgit addしてから評価する。例外は、明示的に読み込むdotfiles.local.tomlだけ。ビルド中の宣言変更を固定する独自snapshotは持たないため、反映中に設定を編集しない。

補助資源はNightlight、AnkiConnect。Nixのout-linkを状態ディレクトリの `dotfiles/current` に登録し、依存をGCから保持する。状態ディレクトリはXDG_STATE_HOME、未指定時はホームの `.local/state`。配置したリンクの参照先だけを `dotfiles/artifacts.json` に記録し、版変更では記録と一致するリンクを更新する。管理外のリンクへの置き換えや既存ファイルとの衝突は拒否する。Ankiのprofileやmediaは削除しない。

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

Bunのテストはコピーの更新・削除、未変更時の保持、symlinkの拒否、補助資源の所有確認、APM失敗後の再実行を検証する。実際のmiseタスクも隔離したホームで実行し、NixとRustが不要なことを確認する。Nixのテストはprivate合成、補助資源のGC rootを確認する。

push時のtest workflowに加え、public-install-smokeを手動実行すると、固定ツールの導入、APMの生成・配備、シェルプラグイン、公開システムのビルドをmacOSで確認できる。CIでは実機のmacOS設定をactivateしない。
