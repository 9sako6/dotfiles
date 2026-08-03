# MySQL EXPLAINの読み方

判断の根拠には、MySQL公式の [EXPLAIN Output Format](https://dev.mysql.com/doc/refman/8.0/en/explain-output.html)、[Optimizing Queries with EXPLAIN](https://dev.mysql.com/doc/refman/8.0/en/using-explain.html)、[Optimization and Indexes](https://dev.mysql.com/doc/refman/8.0/en/optimization-indexes.html) を使う。

## 先に記録する情報

実行計画だけではインデックスの要否を決められない。クエリごとに次を揃える。

- 実際のSQLとbind値
- MySQLのバージョンと実行環境
- 対象テーブルのおおよその行数
- 1回の処理で返す行数
- 1リクエストまたは1ジョブでの呼び出し回数
- ピーク時の並行数
- 現在の応答時間やDB負荷に問題があるか

## 出力列

| 列 | 読み方 |
|---|---|
| `id` | SELECT単位の識別子。サブクエリやUNIONでは複数行になる |
| `select_type` | `SIMPLE`、`PRIMARY`、`SUBQUERY`、`DERIVED` などのSELECT種別 |
| `table` | その行でアクセスするテーブル |
| `type` | 行へ到達する方法。速度の採点ではない |
| `possible_keys` | オプティマイザが候補にしたインデックス |
| `key` | 選択されたインデックス。`NULL` なら使っていない |
| `key_len` | 使用したキー長。複合インデックスの使用範囲を調べる手掛かりになる |
| `ref` | `key` と比較する列または定数 |
| `rows` | MySQLが調べると見積もった行数。InnoDBでは推定値 |
| `filtered` | テーブル条件を通過すると見積もった割合 |
| `Extra` | 絞り込み、並べ替え、一時テーブル、カバリングなどの補足 |

`rows × filtered ÷ 100` は、次のテーブルへ渡す推定行数である。最終的な返却行数や実測行数ではない。

## アクセス方法

`type` はアクセス方法を示す。名前だけで結論を出さず、`rows`、`filtered`、返却行数、呼び出し回数と一緒に読む。

| 値 | 意味 |
|---|---|
| `system` / `const` | 主キーまたは一意キーなどで、最大1行として扱う |
| `eq_ref` | JOINの各組み合わせに対して、主キーまたは一意キーから1行を読む |
| `ref` | 非一意インデックスから一致する行を読む |
| `range` | インデックスの範囲を読む |
| `index` | インデックス全体を走査する。カバリングとは限らない |
| `ALL` | テーブル全体を走査する |

`eq_ref` や `ref` でも、外側の行数や反復回数が多ければ負荷は増える。`ALL` でも、小さいテーブルや大半の行を返すクエリでは妥当な場合がある。

## Extra

| 値 | 読み方 |
|---|---|
| `Using where` | 読み取った行を条件で絞る。これだけでは問題ではない |
| `Using index` | 必要な列をインデックスだけで取得するカバリング |
| `Using index condition` | Index Condition Pushdownでインデックス上の条件を先に評価する |
| `Using filesort` | インデックス順だけでは並べられず、追加のソートを行う。ディスクファイルの使用を意味しない |
| `Using temporary` | 内部一時テーブルを使う。対象件数とメモリ上限を確認する |
| `Using join buffer` | JOINでバッファを使う。結合条件と利用できるインデックスを確認する |

`Using filesort` や `Using temporary` は警告名ではない。少量で上限のある結果なら、そのままのほうが単純なこともある。

## 推定値

`rows` と `filtered` が予想と大きく違う場合は、データ分布と統計を確認する。統計が古いと決めつけない。bind値による偏り、複合条件の相関、候補インデックスの費用も考える。

実測が必要なら、そのMySQLバージョンで `EXPLAIN ANALYZE` を利用できるか確認する。`EXPLAIN ANALYZE` はクエリを実行するため、承認済みの検証環境でSELECTだけに使う。`ANALYZE TABLE` も統計を変更する操作なので、レビュー目的で無断実行しない。

## インデックスの判断

次の順で判断する。

1. 実際のSQL、bind値、返却行数、呼び出し回数を確定する。
2. 選択された `key` と各テーブルの推定走査量を読む。
3. 既存インデックスで同じ用途を満たせないか確認する。
4. 候補インデックスを使う計画を、代表的なデータ量と値で比較する。
5. 読み取りの改善と、INSERT、UPDATE、DELETE、容量への費用を比較する。
6. 追加、不要、判断保留のいずれかを根拠付きで選ぶ。

次の形は調査の入口になる。

- `ALL` または `index` で、返却する割合に比べて多くの行を繰り返し走査している
- `ref` または `range` でも、`rows` と呼び出し回数の積が大きい
- `possible_keys` に候補があるのに `key` が `NULL` で、期待した費用と計画が合わない
- 複合条件の一部しか `key` で絞れず、`rows` に対して `filtered` が低い
- ループ内で同じSQLを発行し、走査量より往復回数が支配的になっている

固定の行数だけでインデックスを追加しない。遅延や負荷の問題が観測できず、将来の件数も確認できない場合は判断を保留する。
