# applyの性能検証

[Issue 152](https://github.com/9sako6/dotfiles/issues/152)に関連し、構成入力の判定による短縮経路の性能を検証しました。

## 測定環境と前提

- 日時：2026-09-24
- OS：macOS（arm64）
- ビルド構成：Rust debug build、Lix

測定は隔離されたGit checkout、配備先、active generation fixtureを用いて実施しました。LixおよびRustのapply-built --copy-only、complete-applyを実行し、配備内容の一致まで検証しています。システム世代と入力記録はfixture上で再現しており、sudo待ち時間や実機システムへの反映処理は測定対象に含めていません。Nix storeの依存関係は取得済みであり、評価キャッシュ無効（cold）はeval-cache=falseの設定を示し、空のstoreからのビルドではありません。本結果は単発測定に基づく記録であり、すべての実行環境での数値を保証するものではありません。

## 測定結果

| 実行条件 | 評価キャッシュ無効（cold） | 評価キャッシュ有効（warm） |
| :--- | :--- | :--- |
| 通常経路 plan（構成入力記録なし） | 6.139 秒 | 1.702 秒 |
| 未変更 apply（短縮経路） | 0.579 秒 | 0.601 秒 |
| copyファイルのみ変更 apply（短縮経路） | 0.804 秒 | 0.791 秒 |
| copy apply直後の再実行 | - | 0.607 秒 |

評価キャッシュ設定有効（eval-cache=true）において通常経路へ復帰する測定値は、CLI変更時のplanが1.514 秒、Nix変更時のplanが8.294 秒です。入力変更に伴いキャッシュのwarm状態が定まらないため、上表のwarm列から分離して記載しています。通常経路のplan測定は構成入力記録を未生成の状態で測定しており、パッケージビルドやactivation処理を含まないため、Issue 152のapply合計時間とは直接比較できません。また、実環境のLixを用いてリソースコミット前後でCLI derivationが完全に一致すること、およびCLIコード変更時に不一致となることを確認しています。

## 再現手順

事前準備として、DOTFILES_BENCH_ROOTに修正対象の全追跡ファイルを含む独立Gitリポジトリ（コミット済み、remoteなし、dotfiles.local.tomlなし）を作成します。DOTFILES_BENCH_STATEには空の隔離作業用ディレクトリを指定し、DOTFILES_BENCH_CLIにはcargo build済みのbinary、DOTFILES_BENCH_NIXにはLixのbinaryを指定します。setup実行時にRustテスト自体がfixture用backendを用意します。このbackendはrequire-nixおよびensure-nixで指定Lixを返し、activateはfixture世代かつcopy-onlyに限定してsudoを実行せずにfixture selectionへ渡します。呼出側によるbackend記述は不要です。setupによりfixture inventory、配備先の初期内容、世代hashを生成します。

DOTFILES_BENCH_ROOT、DOTFILES_BENCH_STATE、DOTFILES_BENCH_CLI、DOTFILES_BENCH_NIXに各隔離パスを設定し、DOTFILES_BENCH_OPERATIONにsetup、plan、applyを指定して同一のRustテストを実行します。通常経路planの測定ではDOTFILES_BENCH_STATE/generation/dotfiles-system-inputsを一時的に退避し、短縮経路の測定で復元します。NIX_CONFIGのeval-cache=falseおよびeval-cache=trueを設定し、copyファイルの変更、CLIコード変更、Nix変更の各条件と対照します。

```sh
cargo test --locked --manifest-path cli/Cargo.toml --bin dotfiles system::fast_path_tests::benchmark_fixture -- --ignored --exact --nocapture
```

本番環境のcheckout、ホームディレクトリ、システム世代をfixture変数に設定してはいけません。
