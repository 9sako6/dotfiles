from pathlib import Path


def replace(path, before, after):
    path = Path(path)
    text = path.read_text()
    assert text.count(before) == 1, (path, before[:80], text.count(before))
    path.write_text(text.replace(before, after))


path = 'cli/README.md'
replace(path, '| [apply](#apply) | システムとホーム設定をビルドし、確認後に反映 |', '| [apply](#apply) | 差分を確認し、承認したシステムとホーム設定を反映 |')
replace(path, '| [plan](#plan) | システムとホーム設定をビルドし、差分・配備計画を表示 |', '| [plan](#plan) | システムとホーム設定の差分・配備計画を表示 |')
replace(path,
    'システム構成とBrewfileをビルドし、システムのパッケージ差分、Homebrewの不足パッケージと削除候補、ホームへのコピー対象を表示します。システム、Homebrew、ホームファイルへの反映は行いませんが、依存の取得やNixストアへの書き込みは発生します。',
    '固定した入力から宣言一覧とスキル情報のJSONを生成し、アクティブ世代との宣言差分と、ホームへのcopy配備計画を表示します。通常はシステム世代をビルドしません。宣言一覧が同じ場合は予定出力パスも比較し、CLI自身の更新などを確認できるようにします。\n\n旧形式の世代など、inventoryを直接比較できない場合だけ、システム構成とBrewfileをビルドしてネイティブ差分へ切り替えます。この経路では、Homebrewの不足パッケージや削除候補も表示します。\n\nシステム、Homebrew、ホームファイルへの反映は行いません。一覧生成に必要なCLIなどのビルド、固定依存の取得、Nixストアへの書き込みは発生し得ます。差分がなければ標準出力に差分を表示せず、終了コード0で終了します。処理中の進捗は標準エラー出力へ表示します。')
replace(path,
    'この実行でビルドした構成をプレビューし、`Apply this system plan? Type yes:`の確認に`yes`を入力すると、そのビルド済み世代を反映します。別途実行した`plan`の結果を引き継ぐ操作ではありません。',
    'この実行で固定した入力の差分を表示し、`Apply this system plan? Type yes:`の確認に`yes`を入力すると反映へ進みます。通常は承認後にシステム世代をビルドします。旧形式の世代との比較で先にビルドした場合は、その出力を再利用します。別途実行した`plan`の結果を引き継ぐ操作ではありません。\n\n差分がなければ確認も反映も行わず、終了コード0で終了します。ビルドの前後に構成入力とアクティブ世代を再検証し、プレビュー後に変化していれば反映しません。')
replace(path,
    '導入済みのLix、公開リポジトリのGitスナップショット、検証を通る設定が必要です。システムやモデルのビルド、`private.path`のチェックアウト読み込みは行いません。表示処理は[src/settings.rs](src/settings.rs)、評価処理は[src/system.rs](src/system.rs)にあります。',
    '導入済みのLix、公開リポジトリのGitスナップショット、検証を通る設定が必要です。`private.path`が設定されている場合は、そのチェックアウトも読み込んで宣言構成を合成します。非公開入力を読めない場合は、一覧の一部だけを表示せずエラーで終了します。\n\n設定一覧に加え、packages、system、services、agentsを表示します。packagesのcurrentは宣言バージョンであり、稼働状態や最新バージョンの照会結果ではありません。一覧用JSONを生成しますが、システム世代やモデルのビルド、構成の反映、lock更新は行いません。表示処理は[src/settings.rs](src/settings.rs)、評価処理は[src/system.rs](src/system.rs)にあります。')
replace(path,
    'モデルIDは[カタログ](../nix/localllm/catalog.nix)で定義します。現在の既知のIDは`qwen3.8-27b-4bit`です。`models`は無効時も既知のIDだけを重複なくアルファベット順に並べる必要があります。',
    '利用できるモデルIDと取得データ量は[カタログ](../nix/localllm/catalog.nix)で定義します。以下の設定はカタログのモデルを選ぶ例であり、利用可能なモデルの完全な一覧ではありません。`models`は無効時も既知のIDだけを重複なくアルファベット順に並べる必要があります。')
replace(path,
    '有効時の初回ビルドには約16 GBのモデルデータ取得を伴います。`plan`もビルドするため取得が発生し得ます。',
    '初回ビルドでは、選択したモデルのデータ取得が発生します。取得量はカタログで確認してください。通常の`plan`はモデルをビルドしませんが、旧形式の世代との比較でシステムをビルドする場合には取得が発生し得ます。')
# Command contracts live in the CLI guide; operations owns setup and recovery.
p = Path('docs/operations.md')
s = p.read_text()
start = s.index('`dotfiles settings`\n')
end = s.index('公開リポジトリの更新には通常のGit操作を使います。', start)
s = s[:start] + '''コマンドの表示内容、入力の読み取り範囲、確認・ビルドのタイミング、副作用は[CLIのUsage](../cli/README.md#usage)を正本とします。設定の所有境界と反映の不変条件は[設計文書](repo-map.md#cli-と評価反映の整合性)、初回導入と失敗時の復旧はこの運用ガイドで扱います。

''' + s[end:]
# The ordinary inventory path compares declarations, not installed Homebrew state.
s = s.replace('なお、キャッシュ利用時であってもHomebrew状態の確認、copy計画の策定、およびapply直前の入力検証は毎回必ず実施されます。',
              'キャッシュ利用時もcopy計画の策定とapply前後の入力検証は省略しません。Homebrewの実状態を照合するネイティブ差分の利用条件は、CLIのplan節を参照してください。')
p.write_text(s)
p = Path('docs/repo-map.md')
s = p.read_text()
needle = '- planは固定した公開・非公開ソースと設定入力を一時GCルートで保持し、宣言inventoryから差分とcopy配備計画を作成する。'
assert needle in s
s = s.replace(needle,
    '- 世代inventoryには、生成時にAPM境界で正規化したスキル情報を含める。比較・描画は保存済みの情報だけを使い、過去世代のAPMファイルを現在の規則で再解釈しない。\n' + needle)
p.write_text(s)
