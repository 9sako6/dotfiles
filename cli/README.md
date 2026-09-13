# dotfiles CLI

macOSのシステム設定とHome Managerの反映、エージェント用リソースの管理に使うRust製CLIです。公開リポジトリの`flake.nix`を構成ルートとして実行します。導入は[初回セットアップ](../docs/operations.md#初回セットアップ)を参照してください。

## Usage

```text
dotfiles [COMMAND]
```

| コマンド | 用途 |
| --- | --- |
| [agents](#agents) | エージェント設定とスキルの生成・依存管理 |
| [apply](#apply) | システムとホーム設定をビルドし、確認後に反映 |
| [help](#help) | ヘルプを表示 |
| [plan](#plan) | システムとホーム設定をビルドし、差分・配備計画を表示 |
| [settings](#settings) | 現在の設定値と設定元を一覧表示 |
| [version](#version) | ビルド元のコミットを表示 |

```sh
dotfiles plan
dotfiles apply
dotfiles settings
dotfiles help agents
dotfiles version
```

CLIが未導入の場合や、チェックアウト内の実装を直接実行する場合は、リポジトリルートでCargoを使います。

```sh
DOTFILES_DIR="$PWD" cargo run --locked --manifest-path cli/Cargo.toml -- plan
DOTFILES_DIR="$PWD" cargo run --locked --manifest-path cli/Cargo.toml -- apply
DOTFILES_DIR="$PWD" cargo run --locked --manifest-path cli/Cargo.toml -- settings
DOTFILES_DIR="$PWD" cargo run --locked --manifest-path cli/Cargo.toml -- agents build
DOTFILES_DIR="$PWD" cargo run --locked --manifest-path cli/Cargo.toml -- help
cargo run --locked --manifest-path cli/Cargo.toml -- version
```

正常終了は終了コード`0`、実行時エラーは`1`、不正な引数は`2`です。引数なしの実行ではヘルプを表示して`1`で終了します。コマンド定義は[src/main.rs](src/main.rs)にあります。

## plan

```sh
dotfiles plan
dotfiles plan --show-trace
```

システム構成とBrewfileをビルドし、システムのパッケージ差分、Homebrewの不足パッケージと削除候補、ホームへのコピー対象を表示します。システム、Homebrew、ホームファイルへの反映は行いませんが、依存の取得やNixストアへの書き込みは発生します。

実行には導入済みのLixが必要です。`sudo`を付けず、ログインユーザーとして実行してください。

`--show-trace`はNixの評価エラーとトレースを端末に表示するオプションです。原因調査には`plan --show-trace`を使います。非公開設定が含まれる場合があるため、その端末内で確認し、診断出力をファイル保存・アップロードしたり、公開Issueへそのまま貼り付けたりしないでください。

## apply

```sh
dotfiles apply
dotfiles apply --show-trace
```

この実行でビルドした構成をプレビューし、`Apply this system plan? Type yes:`の確認に`yes`を入力すると、そのビルド済み世代を反映します。別途実行した`plan`の結果を引き継ぐ操作ではありません。

`yes`以外の入力では中止し、終了コード`1`を返します。確認を省略する`--yes`オプションはありません。別の`apply`が実行中の場合や、確認後に入力の変更が見つかった場合も反映を中止します。

ログインユーザーとして実行します。Lixが未導入なら導入処理が走り、必要な処理で内部から`sudo`を呼び出します。`--show-trace`の挙動と診断出力の扱いは[plan](#plan)と同じです。

反映途中で失敗すると、システム、ホーム、Homebrewに部分的な変更が残る場合があります。[ロールバック](../docs/operations.md#ロールバック)の手順で復旧し、検証が済むまで以前の固定ファイルやキャッシュを保持してください。反映順序と整合性の設計は[設計文書](../docs/repo-map.md#cli-と評価反映の整合性)を参照してください。

## settings

```sh
dotfiles settings
```

現在の`dotfiles.toml`と`dotfiles.local.toml`を評価し、既定値を含むすべての設定を表示します。表示対象は現在のファイルから得られる設定であり、最後に反映した世代の設定ではありません。

- 列は`Key`・`Value`・`Source`です。既定値を使う項目の`Source`は空欄になります。
- `null`、`false`、空配列も表示します。配列は要素数や端末幅にかかわらず常に複数行で表示します。
- 端末では見出しを表示し、パイプ出力では見出しを省きます。
- キー指定や絞り込み、JSON出力などのオプションはありません。

導入済みのLix、公開リポジトリのGitスナップショット、検証を通る設定が必要です。システムやモデルのビルド、`private.path`のチェックアウト読み込みは行いません。表示処理は[src/settings.rs](src/settings.rs)、評価処理は[src/system.rs](src/system.rs)にあります。

## agents

リポジトリの`home/`にあるエージェント設定とスキルを管理します。APMを利用できるmise環境が必要です。操作に伴うClaude・Codex・OpenCode向けリソースの再生成はCLIが行います。

| 構文 | 動作 |
| --- | --- |
| `dotfiles agents build` | 固定済みの依存からエージェント用リソースを生成 |
| `dotfiles agents install [ARGS]...` | スキルなどの依存を導入 |
| `dotfiles agents remove-local <SKILL_NAME>` | ローカルスキルの依存登録を削除し、再生成後にソースを削除 |
| `dotfiles agents uninstall [ARGS]...` | 外部スキルなどの依存を削除。ローカルスキルの指定は拒否 |
| `dotfiles agents update [ARGS]...` | 依存と固定ファイルを更新 |

`ARGS`はAPMの引数です。`install`・`uninstall`では同名のAPMコマンド、`update`では`apm deps update`へ渡します。生成処理の詳細は[src/agents.rs](src/agents.rs)にあります。

`remove-local`の`SKILL_NAME`はASCII英数字で始まり、ASCII英数字で終わる名前です。途中では英数字・`.`・`_`・`-`を使えます。`home/.apm/skills/<SKILL_NAME>/SKILL.md`が存在する必要があり、再生成が成功するとそのスキルのソースディレクトリも削除されます。

```sh
dotfiles agents build
dotfiles agents install --help
dotfiles agents remove-local --help
```

これらの操作はチェックアウト内の`home/`を更新します。コピー対象の配備は[plan](#plan)で確認し、[apply](#apply)で反映します。

## help

```sh
dotfiles help
dotfiles help plan
dotfiles help agents build
```

全体や指定したコマンドのヘルプを表示します。下位コマンドの指定にも対応しています。`dotfiles plan --help` や `-h` の形式も利用できます。リポジトリや設定ファイルが存在しない環境でも実行できます。

## version

```sh
dotfiles version
dotfiles --version
dotfiles -V
```

基本の指定方法は `version` で、各フラグも利用でき出力は同一です。出力形式は `dotfiles <commit>` で、ビルド元リポジトリの完全なコミットハッシュを表示します。ビルド時に追跡ファイルの未コミット変更があれば `-dirty` 接尾辞が付き、由来不明のソースでは `unknown` と表示されます。手動の採番は不要です。値はビルド時に固定されるため、チェックアウトを更新しても再ビルドするまで表示は変わりません。表示にあたってGitやNix、リポジトリ、設定は不要です。

## 実行対象の選択

リポジトリルートは次の順に決まります。

1. `DOTFILES_DIR`で指定した絶対パス。相対パスの指定はエラーになります。
2. カレントディレクトリとその祖先のうち、`flake.nix`と`dotfiles.toml`の両方がある最も近いディレクトリ。
3. `HOME`が指すホームディレクトリ直下の`dotfiles`ディレクトリ。

選ばれたディレクトリには`flake.nix`が必要です。明示的に指定する場合は、リポジトリルートで`DOTFILES_DIR="$PWD"`を使えます。

ホームファイルの配備先は`HOME`が指すホームディレクトリです。非公開設定は[private.path](#privatepath)で指定します。

`plan`と`apply`は公開・非公開リポジトリのGit追跡ファイルを、未コミット変更も含めて使います。新しい管理対象ファイルは事前にステージングしてください。公開側の`flake.nix`と`flake.lock`も追跡済みである必要があります。`dotfiles.local.toml`はGit管理外のまま読み込み、Gitへの追加は拒否します。

`plan`と`apply`はclone、pull、固定ファイルの更新を自動では行いません。更新が必要な場合は、実行前に通常の開発操作で行ってください。

## 設定ファイル

公開リポジトリのルートに共有設定の[dotfiles.toml](../dotfiles.toml)を置きます。マシンごとの上書きはGit管理外の`dotfiles.local.toml`に記述します。ローカル設定に機密情報を含めず、必要に応じて個別にバックアップしてください。

設定の優先順位は「既定値 < `dotfiles.toml` < `dotfiles.local.toml`」です。テーブルは再帰的にマージし、配列とスカラー値は上位の値で完全に置き換えます。`false`や`[]`も置き換えに使えます。

ローカル設定がなければ共有設定と既定値で評価し、非公開リポジトリの探索は行いません。設定ファイルの読み取り不能、TOML構文エラー、未知のキー、型や値の制約違反はエラーになります。

| 設定キー | 型 | 既定値 | 記述先 | 説明 |
| --- | --- | --- | --- | --- |
| `copy` | 文字列配列 | `[]` | 共有のみ | ホームへ実体コピーする相対パス |
| `localllm.default_model` | 文字列または`null` | `null` | 共有・ローカル | 起動時にロードするモデルID |
| `localllm.enabled` | 真偽値 | `false` | 共有・ローカル | ローカルLLMを構成に含めるか |
| `localllm.models` | 文字列配列 | `[]` | 共有・ローカル | 構成に含めるモデルIDの一覧 |
| `private.path` | 文字列または`null` | `null` | ローカルのみ | 非公開モジュールを取り込むチェックアウト |

表の既定値は[nix/configuration.nix](../nix/configuration.nix)のスキーマに基づきます。現在の共有設定には`copy`の一覧があり、ローカルLLMは無効です。

TOMLには`null`を直接記述できません。既定値の`null`を使う場合は、そのキーを省略します。ただし、共有設定に値がある場合、ローカル側で省略してもその値を引き継ぎます。

### copy

リポジトリの`home/`を基準とする相対パスを指定し、ホームディレクトリ内の同じ相対位置へ配備します。ファイルとディレクトリを指定できます。

- パスはアルファベット順で、重複や親子関係がないことが必要です。
- 空文字、絶対パス、`.`や`..`、空のパス要素、末尾の`/`は拒否されます。
- コピー元は実在する必要があり、シンボリックリンクを含められません。
- ディレクトリを指定するとその配下全体を管理し、コピー元にない子要素を反映時に削除します。指定したディレクトリの兄弟要素は保持します。
- 重複する公開構成のHome Managerリンクは自動除外します。その上で、有効な`home.file.target`がコピー対象と一致または親子関係にある場合はNix評価で拒否します。
- コピー先には所有者の書き込みビットを加え、実行権限を保ちます。`0444`は`0644`、`0555`は`0755`になります。

共有設定での記述例です。配列全体が設定値になるため、追加時は既存の必要な項目も残してください。

```toml
copy = [
  ".agents/skills",
  ".codex/AGENTS.md",
]
```

### private.path

`dotfiles.local.toml`だけで指定できます。空文字は拒否され、公開リポジトリルートからの相対パスまたは絶対パスを受け付けます。

指定先は、事前に用意した独立したGitチェックアウトである必要があります。`flake.nix`と`flake.lock`を追跡し、非公開側の`flake.lock`はコミット済みで変更のない状態にします。

非公開flakeは`darwinModules.default`を公開します。公開ルートからそのモジュールを取り込み、公開側のnixpkgsパッケージセットを使って構成します。

ローカル設定の例です。

```toml
[private]
path = "../private-dotfiles"
```

### localllm

モデルIDは[カタログ](../nix/localllm/catalog.nix)で定義します。現在の既知のIDは`qwen3.8-27b-4bit`です。`models`は無効時も既知のIDだけを重複なくアルファベット順に並べる必要があります。

`enabled = true`にする場合は、`models`にちょうど1モデルを指定し、`default_model`にそのIDを指定します。共有設定では無効を維持し、利用するマシンの`dotfiles.local.toml`で有効にします。

```toml
[localllm]
default_model = "qwen3.8-27b-4bit"
enabled = true
models = [
  "qwen3.8-27b-4bit",
]
```

有効時の初回ビルドには約16 GBのモデルデータ取得を伴います。`plan`もビルドするため取得が発生し得ます。無効な構成にはLLM固有の依存を含めませんが、取得済みのデータは無効化だけでは削除されず、不要になったストアパスは後続のGCで回収されます。ランチャーの使い方と動作環境は[ローカルLLMとOpenCode](../docs/operations.md#ローカル-llm-と-opencode-localllm)を参照してください。
