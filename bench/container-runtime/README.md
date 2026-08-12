# Container runtime benchmark

macOS native、Colima、Apple `container machine` の開発時オーバーヘッドを同じ workload で比較する。
単一の総合スコアにはせず、共有ファイルシステム、VM ローカルディスク、起動時間、macOS 側の常駐メモリを分けて観測する。

## 比較対象

| Target | 実行場所 | source / scratch |
|---|---|---|
| `native` | macOS | macOS ローカル |
| `colima` | Colima VZ 上の Docker container | Mac の bind mount |
| `apple-home` | Apple container machine | Mac の home mount |
| `apple-local` | Apple container machine | VM ローカル `/var/tmp` |

Colima は専用 profile を `vz` + `virtiofs` + Docker runtime で作る。普段使いの Colima profile や Docker context は削除しない。
Apple 側も専用 machine だけを作り、既存 machine は触らない。

`apple-home` と `colima` では Rust workload をもう一度 VM ローカルの target directory に出す。
`rust_*_local_target_seconds` と通常の `rust_*_seconds` を比べると、source は共有したまま `CARGO_TARGET_DIR` だけ VM 内へ置く効果を見られる。

## 準備

Docker Desktop から Colima へ移行する PR が反映済みで、`colima`、`docker`、Apple の `container` が PATH にあることを前提とする。
Apple `container` は `container system start` できる状態にしておく。

最初に専用環境と benchmark image を作る。

```sh
mise run container-runtime:prepare
```

既定値は 4 CPU / 8 GiB。比較条件を変える場合は prepare と bench の両方へ同じ値を渡す。

```sh
mise run container-runtime:prepare -- --cpus 6 --memory 12
```

benchmark image は `rust:1.88.0-alpine3.22` を基準にし、fio / git / jq / openssl を追加する。
Docker image store と Apple `container` image store の両方で同じ Dockerfile を build する。

Dockerfile を変更した後など、Apple machine の root filesystem も作り直したい場合だけ `--recreate` を使う。
このオプションが削除するのは既定では `runtime-bench` という benchmark 専用 machine だけ。

```sh
mise run container-runtime:prepare -- --recreate
```

## 実行

```sh
mise run container-runtime:bench
```

Hermes など実 repository の metadata I/O も測る場合は `--repo` を付ける。
repository は Apple container machine の home sharing で参照するため、macOS の `$HOME` 以下にある必要がある。

```sh
mise run container-runtime:bench -- \
  --repo "$HOME/ghq/github.com/9sako6/hermes"
```

既定では各 metric を 5 回測る。負荷を軽くした試走は次のようにできる。

```sh
mise run container-runtime:bench -- \
  --iterations 2 \
  --io-mib 64 \
  --file-count 2000
```

## 測定内容

- `startup_seconds`: 停止済みの専用 VM/machine が command を受け付けるまで
- `host_rss_total_mib`: 起動後に macOS の `ps` から関連プロセスを合計した RSS
- `host_rss_delta_mib`: 停止時から起動後までの関連プロセス RSS 増分
- `seq_write_seconds` / `seq_read_seconds`: portable な `dd` ベースの sequential I/O
- `fio_seq_*` / `fio_rand*`: direct I/O の throughput / IOPS
- `metadata_create_seconds`: 小ファイルを大量生成
- `metadata_walk_seconds`: 大量ファイルの directory walk
- `metadata_delete_seconds`: 大量ファイル削除
- `sha256_seconds`: 生成済みデータの SHA-256。CPU と memory/file read の複合値
- `rust_cold_check_seconds`: 300 module の synthetic Rust project を空 target から `cargo check`
- `rust_warm_check_seconds`: 同じ target へ no-op に近い `cargo check`
- `rust_release_build_seconds`: synthetic Rust project の release build
- `rust_*_local_target_seconds`: source は host 共有、Cargo target だけ VM ローカルに置いた場合
- `git_status_seconds`: `--repo` 指定時だけ、実 repository の untracked files を含む `git status`

portable read は OS page cache の影響を受ける。ストレージ層そのものを見る場合は direct I/O の fio を優先する。
`native` は macOS target、Colima / Apple は Linux target なので Rust の native 対 Linux の絶対値は完全な同条件ではない。Colima と Apple 同士の比較を主に使う。

## 結果

結果は repository ではなく次へ保存する。

```text
~/Library/Caches/dotfiles/container-runtime-bench/results/<timestamp>/
├── metadata.json
├── report.md
├── results.csv
└── *-before-*.ps.txt / *-after-*.ps.txt
```

`report.md` は metric ごとの median / min / max を出す。
RSS は process name による近似なので、判定が微妙な場合は同じ directory の process snapshot を確認する。

判断するときは、まず次を見る。

1. `colima` と `apple-home` の metadata / fio / Rust を比較する。
2. `apple-home` と `apple-local` の差から home sharing のコストを見る。
3. `rust_*_local_target_seconds` で build artifact だけ VM local に逃がした効果を見る。
4. `startup_seconds` と `host_rss_delta_mib` で常用時のオーバーヘッドを見る。

1 回の結果だけで移行判断はせず、Mac がアイドルに近い状態で再測定する。
