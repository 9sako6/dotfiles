---
name: create-anki-cards
description: AnkiConnectから既存のAnki構成とノートを調べ、答えやすさと継続性を優先した暗記カードを設計し、検査後にAnkiへ直接追加・更新する。技術知識や業務知識などをAnkiで覚えたい、既存カードを学習履歴を保って改稿したい、カード案を作成・監査してほしいと依頼されたときに使う。
---

# Ankiカード作成

Ankiを唯一の正本にする。永続的な中間データ、プレビュー、import用TSVを作らない。Ankiのデータベースを直接編集せず、起動中のAnkiへAnkiConnect経由で接続する。

## 進め方

次のチェックリストで進捗を管理する。

```text
- [ ] AnkiConnectから既存構成と対象ノートを取得する
- [ ] 到達目標、範囲、分量を決める
- [ ] 一次資料と前提知識を確認する
- [ ] カード案を作り、表面と短答を対でレビューする
- [ ] checkを実行し、警告を解消または説明する
- [ ] applyを実行し、再取得した結果を確認する
```

### 1. 接続と既存構成

AnkiConnectを導入してAnkiを起動する。接続できない場合は処理を止め、ファイル生成へfallbackしない。

スキルディレクトリにあるBun CLIを使う。

```text
bun <skill-directory>/tools/anki-cards.ts context
bun <skill-directory>/tools/anki-cards.ts context --query 'deck:"対象デッキ"'
```

`context`は現在のprofile、デッキ、ノートタイプとフィールド順、タグを返す。`--query`を指定した場合は、Anki検索に一致するノートID、フィールド、タグ、カードIDも返す。更新対象の`noteId`はこの結果から取得する。

接続先は既定でloopbackのAnkiConnectを使う。portを変更した場合だけ`ANKI_CONNECT_URL`を設定する。API keyを設定したAnkiConnectでは`ANKI_CONNECT_API_KEY`をlocal-onlyの環境から渡す。共有する指示や成果物へ値を書かない。

デッキ、ノートタイプ、フィールド、タグ、カード形式を固定しない。既存構成とプロジェクトの指示を調べ、不明な契約だけ利用者へ確認する。複数回に分ける案件では、必要に応じて`ANKI_PLAN.md`へ到達目標、判断理由、未解決事項、弾ごとの進捗だけを書く。カード本文やCLI契約を複製しない。

### 2. 事実とカード設計

コード、設定、仕様書、公式文書などの一次資料を直接読む。推測をカードへ書かない。カード案ごとに検査用の`sources`を付ける。`sources`はAnkiへ保存しないため、利用中のノートタイプに参考フィールドがある場合は、必要な根拠をそのフィールドにも書く。

本編で使う語彙の前提を確認し、必要なら基礎カードを先に作る。カードを書く前に[カード設計規約](references/card-design.md)をすべて読む。

日本語のカードを作成または改稿するときは、`stop-ai-slop-jp`が利用できればその`SKILL.md`も読む。主体の不在、文のねじれ、抽象語、翻訳調、不要な記号を直すために使う。一次資料に基づく断定を伝聞調へ弱めたり、主観や皮肉を加えたりしない。

### 3. 検査入力

カード案を次のJSON構造にし、ファイルへ保存せず標準入力からCLIへ渡す。

```json
{
  "version": 1,
  "contract": {
    "noteType": "使用中のノートタイプ",
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
  "notes": [
    {
      "id": "topic-001",
      "deck": "学習対象",
      "fields": {
        "問題": "具体的な条件を満たす仕組みを何と呼ぶ？",
        "答え": "用語",
        "参考": "https://example.com/spec"
      },
      "tags": ["基礎"],
      "sources": ["https://example.com/spec"]
    }
  ]
}
```

新規ノートには`deck`を指定する。既存ノートの更新では`deck`の代わりに`context`で取得した`noteId`を指定する。`id`は一回の検査とレビューで使う識別子であり、Ankiへ保存しない。

`role`は`question`、`answer`、`reference`、`media`、`id`、`other`のいずれかにする。フィールド名と順序はAnkiのノートタイプと完全に一致させる。

タグを制限しない場合は`tagPolicy`を次の形にする。

```json
{ "mode": "open", "requireAtLeastOne": false }
```

### 4. 検査と書き込み

同じJSONを標準入力から順に渡す。

```text
bun <skill-directory>/tools/anki-cards.ts check
bun <skill-directory>/tools/anki-cards.ts apply
```

`check`は構造、一意性、フィールド、タグ、一次資料、制御文字を検査する。複数回答や長すぎる答えは警告する。警告が出たカードを読み直し、妥当な例外だけカード案の`reason`へ理由を書く。

`apply`は同じ検査を繰り返し、Anki上のデッキ、ノートタイプ、フィールド、更新対象を照合する。更新を先に実行し、更新が一部でも失敗した場合は新規ノートを追加しない。成功後は対象ノートを再取得し、フィールド、タグ、ノートタイプ、新規ノートのデッキを検証する。カード作成または更新の依頼を受けている場合、`check`成功後の追加確認は不要とする。

AnkiConnectから応答を受け取れなかった場合は、書き込みの成否を推測しない。`context`で現在状態を再取得し、確認できるまで`apply`を再実行しない。

削除はこのスキルの対象外とする。利用者がAnki内で削除したノートを復元しない。

### 5. レビューと完了

[カード設計規約](references/card-design.md)の「7. レビュー」を一枚ずつ行う。不一致が見つかった観点では、同じ弾の全カードを見直す。CLIでは目的の欠落や疑問詞と短答の意味上の不一致を判定できないため、警告0件でも通読を省略しない。

完了時に次を報告する。

- 追加・更新したノート数とノートID
- `check`のエラー数と警告数
- 再取得検証の結果
- 書き込みの成否が不明な操作がないこと
