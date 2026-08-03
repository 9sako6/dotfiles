---
name: rails-mysql-explain
description: Rails 6以降でActive Recordクエリや生SQLを追加・変更し、MySQLのEXPLAIN、インデックスの要否、走査量を確認するときに使う。クエリ性能のレビューやインデックス追加の判断も対象にする。
compatibility: Requires MySQL 8.0+ and Rails 6.0+
license: Apache-2.0
metadata:
  author: 9sako6
  version: "1.0.0"
---

# Rails / MySQLのクエリ計画レビュー

変更したクエリが実際に発行するSQLを確認し、MySQLの実行計画と呼び出し回数からインデックスの要否を判断する。

## 進め方

### 1. 対象

変更差分から、追加または変更したActive Recordクエリと生SQLを洗い出す。スコープ、関連の先読み、集計、存在確認、Arel、`find_by_sql`、`select_all`、`exec_query` も含める。

各クエリについて、実行箇所、目的、想定するbind値、1リクエストあたりの呼び出し回数を記録する。ループやジョブから呼ばれる場合は、ピーク時の件数と並行数も確認する。

### 2. 実際のSQL

`ActiveRecord::Relation` のまま扱えるクエリは、変更後と同じrelationで `explain` を呼ぶ。

```ruby
Customer.where(active: true).includes(:orders).explain
```

Railsの `explain` は、関連の先読みに必要なクエリを実行してから各SQLの計画を取得する。`includes` がJOINになるか複数クエリになるかを手で決めない。挙動は [Rails GuidesのRunning EXPLAIN](https://guides.rubyonrails.org/active_record_querying.html#running-explain) で確認する。

`exists?`、`pluck`、集計など、呼び出した時点でSQLを発行するメソッドは、テスト環境または承認済みの検証環境のRailsログからSQLとbind値を取得する。元のrelationの `to_sql` で代用しない。

生SQLは、実際に渡すSQLとbind値を組にして扱う。書き込みを伴うメソッドは、計画を得るためだけに実行しない。`find_or_create_by` では検証環境のログから検索SQLを取得し、書き込み側の一意性は別に確認する。

### 3. EXPLAINの取得

データ量と統計が本番に近い、実行を許可された環境を使う。参照専用接続があれば優先する。利用できる環境がなければ、クエリごとに次を依頼する。

```text
次のクエリについて、データ量と統計が本番に近い検証環境でEXPLAINを取得してください。

- 目的:
- 実行箇所:
- Railsの式またはSQL:
- bind値:
- 想定する呼び出し回数:
```

結果と一緒にMySQLのバージョン、対象テーブルのおおよその行数、取得した環境を記録する。

### 4. 読み取り

[EXPLAIN読み方ガイド](references/explain-guide.md)に従う。`type` だけで良否を決めず、`key`、`rows`、`filtered`、`Extra`、テーブル規模、呼び出し回数を合わせて読む。MySQLの各列の定義は [EXPLAIN Output Format](https://dev.mysql.com/doc/refman/8.0/en/explain-output.html) を正本にする。

### 5. 結論

結果はMarkdownで回答する。利用者がファイルを指定した場合だけ保存する。

```markdown
# EXPLAINレビュー

## クエリ1: [目的]

- 実行箇所: `app/...`
- SQLとbind値: `...`
- 呼び出し回数: ...
- EXPLAIN: `type=...`, `key=...`, `rows=...`, `filtered=...`, `Extra=...`
- 読み取り: ...
- インデックス: 追加する / 追加しない / 判断保留
- 根拠: ...

## 結論

- 追加するインデックス:
- 追加しない理由:
- 取得できていない情報:
```

インデックスを提案するときは、列順と対象クエリを示し、書き込み、容量、既存インデックスへの影響も書く。判断材料が足りなければ、追加か不要かを推測せず保留にする。

### 6. 確認

各クエリに、実際のSQLとbind値、`type`、`key`、`rows`、`filtered`、`Extra`、呼び出し回数、インデックス判断の根拠があることを確認する。
