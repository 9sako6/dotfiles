---
name: rails-mysql-explain
description: 'Use when Rails 6+ で ActiveRecord クエリや生SQLを新規作成・変更し、EXPLAINで実行計画、インデックス要否、rows を確認したいとき。where/joins/includes/exists?/pluck/find_by_sql/connection.select_all などを含む変更、またはクエリ性能レビューやインデックス追加判断が必要なときに使う。'
compatibility: Requires MySQL 8.0+ and Rails 6.0+
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
---

# Rails MySQL EXPLAIN Check

RailsでActiveRecordクエリや生SQLを書いた・変更したとき、MySQL EXPLAINを使ってインデックスの必要性を検証するワークフロー。

## ワークフロー

### Step 1: 対象クエリの特定

変更差分から、以下を含むActiveRecordクエリを全て洗い出す:

- `where`, `joins`, `left_joins`, `includes`, `eager_load`
- `exists?`, `pluck`, `count`, `sum`, `maximum`, `minimum`
- `find_by`, `find_or_create_by`
- `order`, `group`, `having`, `distinct`
- スコープ定義の中のクエリ
- `find_by_sql`, `connection.select_all`, `connection.exec_query`
- Arel から組み立てたSQL、ヒアドキュメントや文字列リテラルで直接書かれたSQL

### Step 2: SQLの抽出

各クエリについて、EXPLAIN用のSQLを取得する。**メソッドによって取得方法が異なる**ことに注意。

#### 通常のリレーション（where, joins, order等）

```ruby
Model.where(...).joins(...).to_sql
```

#### exists?

`exists?` は実行時に `order` / `select` / `distinct` を除去してからSQLを発行するため、`to_sql` で得たSQLとは異なる実行計画になりうる。正確なSQLはログから取得する:

```ruby
ActiveRecord::Base.logger = Logger.new(STDOUT)
Model.where(...).exists?(['condition'])
# ログに出力されたSELECT 1 AS one FROM ... LIMIT 1 を使う
```

#### pluck

`pluck` も実行時にSQLを組み立てるため、ログから取得する:

```ruby
ActiveRecord::Base.logger = Logger.new(STDOUT)
Model.where(...).pluck(:column_name)
# ログに出力されたSELECT文を使う
```

#### eager_load（常にJOIN）

`eager_load` は常に `LEFT OUTER JOIN` を含む単一SQLを生成する。`to_sql` でEXPLAIN対象のSQLを取得できる:

```ruby
Model.eager_load(:assoc).where(...).to_sql
```

#### includes（場合分けが必要）

`includes` は条件によって挙動が変わる:

- **WHERE句やORDER句で関連テーブルを参照している場合** → `eager_load` と同様に `LEFT OUTER JOIN` の単一SQLになる。`to_sql` で取得可能
- **関連テーブルを参照していない場合** → 別クエリ（preload）で発行される。ログから複数のSQLを取得する

```ruby
# JOINになるケース（to_sqlで取得可能）
Model.includes(:assoc).where(assoc: { column: value }).to_sql

# preloadになるケース（ログから取得）
ActiveRecord::Base.logger = Logger.new(STDOUT)
Model.includes(:assoc).where(column: value).load
# メインクエリ + IN句のpreloadクエリ、それぞれをEXPLAIN
```

#### 生SQL

`find_by_sql` や `connection.select_all` / `connection.exec_query` 等で直接SQL文字列を書いている場合は、そのSQLをそのままEXPLAIN対象として扱う。プレースホルダを使っている場合は、実際に評価するバインド値も合わせて記録する:

```ruby
sql = <<~SQL
  SELECT users.*
  FROM users
  WHERE users.account_id = ?
SQL

User.find_by_sql([sql, account_id])
# EXPLAINには実際に評価するSQLとバインド値を対応づけて渡す
```

### Step 3: EXPLAIN実行依頼

生成したSQLスニペットをヒトに渡し、**本番相当のデータ量がある環境**のコンソールで実行してもらう。

小規模なデータではフルスキャンでも高速に完了するため、MySQLオプティマイザがインデックスを使わない実行計画を選ぶことがある。テーブル統計と行数がオプティマイザの判断に影響するため、本番相当のデータ量がある環境での検証が必須。

実行依頼テンプレート:

```
以下のSQLについて、本番相当のデータ量がある環境のRailsコンソールで
ActiveRecord::Base.connection.explain(sql) を実行し、結果を共有してください。

リードレプリカ等の参照専用接続がある場合はそちらを推奨します。

1. [クエリの目的]
   sql = "..."

2. [クエリの目的]
   sql = "..."
```

### Step 4: EXPLAIN結果の読み取り

結果を `references/explain-guide.md` の判定基準に照らして分析する。

主要な確認ポイント:

1. **`type`列**: `ALL`（フルテーブルスキャン）がないか
2. **`key`列**: 適切なインデックスが使われているか
3. **`rows`列**: 走査行数が妥当か（テーブルの総行数に対する割合）
4. **`Extra`列**: `Using filesort`, `Using temporary` がないか

MySQL EXPLAIN出力の一次情報: https://dev.mysql.com/doc/refman/8.0/en/explain-output.html

### Step 5: 結論の記述

下記を含む分析結果をMarkdown形式で、一時ファイルを置くことが許可されたディレクトリに出力する。

- 各クエリのEXPLAIN結果と読み取り
- インデックスの要否判断と根拠
- 必要な場合: マイグレーションの提案
- ループ内で呼ばれるクエリがある場合: ピーク時の同時実行数を考慮した負荷見積もり

出力テンプレート:

```markdown
# EXPLAIN Review

## Query 1: [クエリの目的]
- SQL: `...`
- 実行箇所: `app/...`
- EXPLAIN要約: `type=...`, `key=...`, `rows=...`, `Extra=...`
- 読み取り: ...
- インデックス要否: 必要 / 不要
- 根拠: ...

## Query 2: [クエリの目的]
- SQL: `...`
- 実行箇所: `app/...`
- EXPLAIN要約: `type=...`, `key=...`, `rows=...`, `Extra=...`
- 読み取り: ...
- インデックス要否: 必要 / 不要
- 根拠: ...

## Conclusion
- 追加すべきインデックス:
- 追加不要と判断した理由:
- 残るリスク:
```

### Step 6: 自己確認

出力前に `references/explain-guide.md` を見直し、各クエリについて少なくとも `type`, `key`, `rows`, `Extra`, インデックス要否の根拠が記述されていることを確認する。

## 判断基準サマリ

| 状況 | 判断 |
|------|------|
| `type: ALL` かつ `rows` が数万以上 | インデックス追加を検討 |
| `type: ref` または `eq_ref` | 通常は問題なし |
| `type: range` | 範囲が広すぎないか `rows` を確認 |
| `type: index`（フルインデックススキャン） | テーブルサイズ次第で要検討 |
| `possible_keys` にキーがあり `key` がNULL | オプティマイザが使わなかった理由を調査 |
| ループ内クエリで `type: ref` | 1回あたりは軽量でもN回の積算コストを評価 |

詳細な判定基準は `references/explain-guide.md` を参照。
