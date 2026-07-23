---
name: create-anki-cards
description: 学習対象と既存のAnki構成を調べ、答えやすさと継続性を優先した暗記カードを設計し、検証済みのテキストインポートTSVとMarkdownプレビューを生成・更新する。技術知識や業務知識などをAnkiで覚えたい、既存カードを学習履歴を保って改稿したい、Anki用TSVやカード案を作成・監査してほしいと依頼されたときに使う。
---

# Ankiカード作成

デッキ、ノートタイプ、フィールド、タグ、カード形式を固定しない。既存の構成を調べ、不明な契約だけ利用者へ確認する。

## 進め方

次のチェックリストで進捗を管理する。

```text
- [ ] 既存の契約と進捗を確認する
- [ ] 到達目標、範囲、分量を決める
- [ ] 一次資料と前提知識を確認する
- [ ] anki.jsonへカードを書く
- [ ] checkとbuildを実行する
- [ ] プレビューをレビューして修正する
- [ ] インポート方法と確認項目を渡す
```

### 1. 契約

次の順で確認する。

1. プロジェクトの指示ファイル
2. 既存の `anki.json` と、継続案件なら `ANKI_PLAN.md`
3. 既存TSVまたはAnkiから書き出したテキスト
4. 利用者の回答

デッキ名、ノートタイプ、フィールド名と順序、各フィールドの役割、HTML、タグ規約、新規作成か既存更新かを確定する。資料同士が矛盾する場合は推測せず確認する。

複数回に分ける案件では `ANKI_PLAN.md` に到達目標、判断理由、未解決事項、弾ごとの進捗を記録する。CLIが読む契約は `anki.json` だけに置き、PLANへ複製しない。規約を変えたら、既存カードにも遡って適用する。

到達目標、苦手領域、1弾の枚数を作成前に確認する。枚数の指定がなければ20〜30枚から始め、最初のプレビューへの反応を見て増減する。

### 2. 事実と前提

コード、設定、仕様書、公式文書などの一次資料を直接読む。推測をカードへ書かない。カードごとに根拠を `sources` へ残す。共有する成果物では、ローカル絶対パスではなくリポジトリ相対パスまたは公開URLを使う。

本編で使う語彙の前提を確認し、必要なら基礎カードを先に作る。技術領域では「一般概念→製品や基盤→対象システム」が使いやすいが、三階層へ固定しない。

カードを書く前に [references/card-design.md](references/card-design.md) をすべて読む。

### 3. 正規データ

`anki.json` を唯一の正規データにする。TSVとプレビューは手編集しない。

```json
{
  "version": 1,
  "contract": {
    "mode": "create",
    "output": "cards.tsv",
    "preview": "cards.preview.md",
    "deck": "学習対象",
    "noteType": "使用中のノートタイプ",
    "html": true,
    "fields": [
      { "name": "問題", "role": "question", "required": true },
      { "name": "答え", "role": "answer", "required": true },
      { "name": "参考", "role": "reference", "required": false }
    ],
    "tagPolicy": {
      "mode": "restricted",
      "allowed": ["基礎", "応用"],
      "requireAtLeastOne": true
    }
  },
  "cards": [
    {
      "id": "topic-001",
      "fields": {
        "問題": "具体的な条件を満たす仕組みを何と呼ぶ？",
        "答え": "用語",
        "参考": "https://example.com/spec"
      },
      "tags": ["基礎"],
      "sources": ["https://example.com/spec"],
      "notes": "必要な場合だけ判断理由を書く"
    }
  ]
}
```

`role` は `question`、`answer`、`reference`、`media`、`id`、`other` のいずれかにする。Ankiのフィールド名は制限しない。

タグを制限しない場合は `tagPolicy` を次の形にする。

```json
{ "mode": "open", "requireAtLeastOne": false }
```

カードの `id` はレビュー用であり、Ankiへ出力しない。一度付けたIDは文面を直しても変えない。

### 4. 検査と生成

スキルディレクトリにあるBun CLIを使う。

```text
bun <skill-directory>/tools/anki-cards.ts check <project>/anki.json
bun <skill-directory>/tools/anki-cards.ts build <project>/anki.json
```

`check` は構造、一意性、フィールド、タグ、一次資料、制御文字を検査する。複数回答や長すぎる答えは警告する。警告を機械的に無視せず、カードを読むか、妥当な例外の理由を `notes` または `ANKI_PLAN.md` に残す。

`build` は同じ検査に成功した場合だけ、`output` のTSVと `preview` のMarkdownを生成する。出力は `anki.json` と同じディレクトリ内の相対パスに限る。

カード本文は `stop-ai-slop-jp` が利用できる場合は、その基準で推敲してから最終生成する。

### 5. 既存カードの更新

Anki GUIDを独自に作らない。新規カードはGUID列なしで生成し、AnkiにGUIDを生成させる。この扱いは[Anki ManualのGUID Column](https://docs.ankiweb.net/importing/text-files.html#guid-column)に従う。

既存カードを更新する場合は、契約を `update` にして安定した識別フィールドを指定する。

```json
{
  "mode": "update",
  "identityField": "変更しない識別フィールド"
}
```

Ankiの画面からGUIDを含むテキストを書き出し、次を実行する。

```text
bun <skill-directory>/tools/anki-cards.ts build <project>/anki.json --anki-export <exported.tsv>
```

CLIは識別値を一対一で完全照合する。不足、余剰、GUID重複、識別値重複があれば更新TSVを作らない。

Ankiの実データベースを直接編集しない。インポート前に復元可能なバックアップを作り、Ankiの画面から読み込む。更新後は再度テキストを書き出し、更新件数、GUID、フィールド、タグ、ノートタイプ、学習状態に想定外の変化がないか確認する。

### 6. レビュー

利用者にはTSVではなくプレビューを先に見せ、レビューIDで修正を受ける。修正は `anki.json` に入れ、TSVとプレビューを再生成する。

完了時に次を報告する。

- 生成したカード数とファイル
- `check` のエラー数と警告数
- 新規作成か既存更新か
- Ankiで選ぶインポート方法と、更新後に確認する項目
