# MySQL EXPLAIN 読み方ガイド

一次情報: https://dev.mysql.com/doc/refman/8.0/en/explain-output.html

## EXPLAIN出力の主要カラム

| カラム | 意味 |
|--------|------|
| `id` | SELECT識別子。サブクエリやUNIONで複数行になる |
| `select_type` | `SIMPLE`, `PRIMARY`, `SUBQUERY`, `DERIVED` 等 |
| `table` | 対象テーブル |
| `type` | **結合タイプ（最重要）** — 下記の階層を参照 |
| `possible_keys` | 使用候補のインデックス |
| `key` | 実際に使われたインデックス |
| `key_len` | 使われたインデックスの長さ（複合インデックスの何カラム目まで使われたか推定可能） |
| `ref` | インデックスと比較されたカラムまたは定数 |
| `rows` | 走査予測行数（統計ベースの推定値） |
| `filtered` | テーブル条件でフィルタされる行の割合（%） |
| `Extra` | 追加情報 — 下記参照 |

## type列の階層（上ほど高速）

```
system  — テーブルに1行しかない（constの特殊ケース）
const   — PRIMARY KEY / UNIQUEインデックスで1行特定
eq_ref  — JOINで相手テーブルから1行特定（PRIMARY KEY / UNIQUE）
ref     — 非UNIQUEインデックスで一致する行を取得
fulltext — FULLTEXTインデックス使用
ref_or_null — refと同様だがNULLも検索
index_merge — 複数インデックスのマージ
unique_subquery — INサブクエリでPRIMARY KEY使用
index_subquery — INサブクエリで非UNIQUEインデックス使用
range   — インデックスを使った範囲検索（BETWEEN, IN, >, < 等）
index   — フルインデックススキャン（データは読まないがインデックス全走査）
ALL     — フルテーブルスキャン（最も遅い）
```

### 判定基準

- **`const`, `eq_ref`**: 問題なし。主キーやユニークキーで1行特定
- **`ref`**: 通常は問題なし。`rows` が極端に多くないか確認
- **`range`**: 範囲の広さを `rows` で確認。数百〜数千行なら通常OK
- **`index`**: フルインデックススキャン。テーブルが小さければ許容、大きければ要改善
- **`ALL`**: フルテーブルスキャン。小テーブル（数百行以下）以外では要改善

## Extra列の注目すべき値

| 値 | 意味 | 対応 |
|----|------|------|
| `Using index` | カバリングインデックスで完結（データページ不要） | 良好 |
| `Using where` | WHERE句でフィルタリング | `rows` と `filtered` を合わせて確認 |
| `Using index condition` | Index Condition Pushdown | 良好（MySQL 5.6+） |
| `Using temporary` | 一時テーブル使用（GROUP BY, DISTINCT等） | 大量データでは要注意 |
| `Using filesort` | ソート処理にファイルソート使用 | 大量データでは要注意 |
| `Using join buffer` | JOINバッファ使用（インデックスなしJOIN） | インデックス追加を検討 |

## よくあるパターンと判断

### 外部キーのJOIN

```
type: eq_ref, key: PRIMARY
```

主キーでのJOINは最適。外部キー側のインデックスも `possible_keys` に現れていれば問題なし。

### WHERE句での絞り込み

```
type: ref, key: index_on_column, rows: 15
```

インデックスが使われ、走査行数が少なければ問題なし。

### 複合条件のWHERE

```
type: ref, key: index_on_col_a, rows: 1000, filtered: 10.00
```

`rows` x `filtered` / 100 = 実際に返る推定行数（この例では100行）。
`filtered` が低い場合、複合インデックスの追加で改善できる可能性がある。

### サブクエリ / EXISTS

```
id: 2, select_type: DEPENDENT SUBQUERY, type: ref
```

外側の行ごとにサブクエリが実行される。外側の `rows` x 内側の `rows` が総走査行数の目安。

### IN句（ActiveRecordのincludesが生成するクエリ）

```sql
SELECT * FROM images WHERE id IN (1, 2, 3, ...)
```

```
type: range, key: PRIMARY, rows: N
```

IN句の要素数がNに反映される。主キーでの `range` なので効率的。

### LEFT OUTER JOIN（eager_load / includesのJOIN化）

```
id: 1, table: parent,  type: ALL,    rows: 100
id: 1, table: child,   type: eq_ref, key: PRIMARY, rows: 1
```

親テーブルのスキャン方式と子テーブルのJOIN方式を両方確認する。子側が `eq_ref` なら効率的。親側の `type` が `ALL` でも行数が少なければ許容。

## インデックス追加の判断フロー

```
1. type が ALL または index か？
   ├─ YES → テーブルの行数を確認
   │   ├─ 数百行以下 → 許容（フルスキャンでも十分高速）
   │   └─ 数千行以上 → インデックス追加を検討
   └─ NO → rows を確認
       ├─ 妥当な行数 → 問題なし
       └─ 想定より多い → 複合インデックスやカバリングインデックスを検討

2. ループ内で呼ばれるクエリか？
   ├─ YES → 1回あたりの rows が小さくても、N回の積算を評価
   │   例: rows=5 x 100レコード = 500行走査（許容範囲内）
   │   例: rows=5000 x 100レコード = 50万行走査（要改善）
   └─ NO → 1回の実行コストのみで判断

3. possible_keys にキーがあるのに key が NULL か？
   └─ YES → オプティマイザが「インデックスを使うより全スキャンが速い」と判断した
       → テーブル統計が古い可能性あり（ANALYZE TABLE を検討）
       → または本当に全スキャンが最適（小テーブルなど）
```

## 本番相当のデータ量がある環境で実行すべき理由

MySQLオプティマイザはテーブル統計（行数、カーディナリティ等）に基づいて実行計画を決定する。小規模データでは:

- フルテーブルスキャンがインデックスアクセスより速いと判断されることがある
- `rows` の推定値が実際の本番と大きく異なる
- 複合インデックスの選択判断が変わる

そのため、EXPLAIN結果は本番相当のデータ量がある環境で取得したものを信頼する。リードレプリカなど参照専用の接続先があればそちらを使う。
