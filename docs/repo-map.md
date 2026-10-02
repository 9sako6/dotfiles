# 設計

日常の反映はmiseタスクを入口とする。設定を編集するたびにCLIを再ビルドしたりNixを評価したりする仕組みをなくし、変更した領域だけを反映する。

## 管理境界

| 領域 | 宣言・実装 | 入口 |
|---|---|---|
| 設定とユーザーツール | home、.mise.toml、home/.config/mise/config.toml、dotfiles.tomlのcopy | `mise run home:apply` |
| エージェント資源 | home/.apm、home/apm.yml、home/apm.lock.yaml | `mise run agents:apply` |
| macOSとNix補助資源 | nix、flake.lock、dotfiles.local.toml | `mise run system:apply` |

普通のCLIとツールチェーンはmiseのユーザー領域に導入する。ホームのmise設定で有効にし、各プロジェクトの設定で版を選ぶ。GUI、システムサービス、Nightlight、AnkiConnect、localllmはNix側に置く。Anki GUI 26.05はシステムのNixパッケージとして維持する。

通常の設定はmiseの `[dotfiles]` でcheckoutへリンクする。devcontainerなどから実体として読む必要がある設定だけ、dotfiles.tomlのcopyで宣言する。コピー処理はBun TypeScriptに置き、コピーと反映を同じタスクで完了させる。指定したディレクトリ配下を全て所有し、削除も同期する。管理外の親や兄弟を消さず、symlinkをたどって書き込まない。

APMの依存操作と生成はAPM自身に任せる。agents:applyは固定依存の取得、生成、コピーだけを行う。home:applyとagents:applyはNix・sudo・Rustを必要としない。

## システムの合成

Nix宣言はnixディレクトリにまとめる。system.nixがmacOSとHomebrew、packages.nixがNixパッケージ、artifacts.nixが補助資源、home.nixがprivate Home Managerの互換性と所有境界を担当する。nix/apply.nixはローカル設定を読み、公開・privateのhostと補助資源を構築する。

公開設定はdotfiles.toml、マシン固有の上書きはGit管理外のdotfiles.local.tomlに置く。copyは公開設定だけ、private.pathはローカル設定だけで定義する。localllmとNight Shiftはローカルで個々の値を上書きできる。Nix側のcopy・localllm・privateはconfiguration.nixで検証し、Night Shiftはsystemタスクで検証する。設定値をエラーへ含めない。

privateは独立したGit checkoutとflakeを持ち、darwinModules.defaultを公開する。Home Manager userはprivateが明示的に定義する。公開側はuserを作らず、privateのhome.file、packages、launchd、stateVersionを維持する。configuration、dotfilesDirectory、dotfilesSourceHome、inputsのspecialArgsと、公開nixpkgsを共有する契約も維持する。公開配置と重なるprivate home.fileは拒否する。

補助資源のNix rootはパッケージへのリンクを持ち、状態ディレクトリにout-linkとして登録する。前回配置したリンクの参照先だけを記録し、利用者や他ツールが置き換えた配置を自動削除しない。コピー対象のディレクトリには独自の所有記録を作らず、現在のcopy宣言を所有範囲とする。

公開・privateはNix標準のGit入力で取得する。独自snapshot、全領域のplan、inventory、評価cache、CLIの自己更新を持たない。システム反映は標準nix-darwinの世代とactivationに任せる。Bootstrapだけが固定miseとBunを導入して3つのタスクを順に実行する。

## バージョン固定

依存は、別のマシンや時点でも同じものを取得できる形式で指定する。

| 宣言元 | 固定方法 |
|---|---|
| APM | apm.lock.yamlと `--frozen` |
| GitHub Actions | commit SHAとバージョンコメント |
| mise tools | 完全なバージョンとmise.lock |
| mise bootstrap repos | 40桁のGitコミット |
| Nix inputs | flake.nixの追従先とflake.lockのexact revision |
| Nix packages | パッケージ属性と期待バージョンのassertion |
| その他 | 厳密なバージョンまたはrevision |
| Python・MLX | pyproject.tomlとuv.lockのexact version・wheel hash |

普通のCLIの版はhome/.config/mise/config.tomlを正本とし、CIとBootstrapへ同じ版を重複定義しない。mise本体はbin/install-mise.shで版とinstaller hashを固定する。Nixの版更新ではflake.lockだけでなく、期待バージョンのassertionも確認する。`latest`、範囲指定、タグだけのActions指定は固定として扱わない。

localllmはnix/localllm/catalog.jsonから1モデルを選ぶ。無効時はモデル・MLX・専用OpenCodeを依存に含めない。有効時は固定したuv2nix環境とモデルをビルドし、実行時にパッケージを導入しない。推論はローカルで実行するが、OpenCodeのWebツールと子コマンドは通信できる。詳細と検証範囲は[運用ガイド](operations.md#ローカル-llm-と-opencode-localllm)を参照する。

Homebrewのformulaとcaskはnix/homebrew-packages.nixに集約する。FFmpegはmiseのconda:ffmpegで固定し、旧Nix配布との差分は[運用ガイド](operations.md#ffmpegの配布と検証)に記録する。手動で列挙する資源はアルファベット順を保つ。
