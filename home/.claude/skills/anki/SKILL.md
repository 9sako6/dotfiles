---
name: anki
description: 学びたい主題とAnkiの既存ノートから現在地を組み立て、前提知識から自然につながる次の学習カードを設計し、承認後だけAnkiConnectで追加する。Ankiで新しい内容を学び始めたい、続きを作りたい、既存カードとの重複を避けて少量ずつ学びたいときに使う。
---

# Anki学習

Ankiを唯一の正本にする。

追加で永続化する学習メタデータは、学習の目的地を表す `Goal` だけにする。現在地、ロードマップ、次に学ぶ範囲、進捗率、バッチ番号は保存せず、毎回Ankiと一次資料から導出する。

同じ依頼を再実行しても、Ankiの現在状態を読み直してから判断する。既に存在する知識を重複追加しない。

カードを書く前に [references/card-design.md](references/card-design.md) をすべて読む。日本語のカードを作成するときは、`stop-ai-slop-jp` が利用できればその `SKILL.md` も読む。

## 進め方

```text
- [ ] Ankiを読む
- [ ] Goalを確認する
- [ ] 必要ならdesign-itでGoalを明らかにする
- [ ] 既存カードと一次資料から現在地とロードマップを導出する
- [ ] 次のカード案を10枚作る
- [ ] カード案をレビューする
- [ ] 明示的な承認を得る
- [ ] Ankiを再確認して追加する
```

## Goal

学習の目的地だけを、通常のAnkiノートとして保存する。

```text
Note Type: Goal
Field: Definition
Tag: quint
```

`Goal` ノートタイプは `Definition` 一フィールドだけを持つ。`Definition` には、学習後に何を説明、判断、実装、検証できるようになりたいかを書く。

Goalと対応する学習ノートには、`quint` のように主題を表す同じ通常タグを付ける。このタグに特別な種類や名前空間を設けない。

Goalから生成されるカードはsuspendする。Goalは目的地のメタデータであり、通常の復習対象や「次の10枚」に含めない。

最初にGoalを読む。

```sh
bun <skill-directory>/tools/goal.ts get quint
```

- `missing`: Goalがない。会話だけで目的地が十分に分からなければ `design-it` を使い、一問ずつ明らかにする。確定したDefinitionを提示し、人間の明示的承認後だけ作成する。
- `found`: 既存Definitionを目的地として使う。十分に分かっている目的を聞き直さない。
- `conflict`: 同じタグにGoalが複数ある。勝手に選択、統合、削除せず、人間に確認する。

作成または更新は、Definitionと対象が明確な承認を得た後だけ行う。

```sh
printf '%s\n' 'Quintで小さな仕様を書き、性質を検証し、結果を解釈できる。' \
  | bun <skill-directory>/tools/goal.ts set quint
```

既存Goalの更新ではdeck指定を要求しない。新規Goalでは、同じタグを持つ既存学習カードが一つのdeckに属していれば、そのdeckを使う。deckを一意に決められない場合は `needs-deck` を返すので、人間に確認してから選んだdeckを渡す。

```sh
printf '%s\n' 'Quintで小さな仕様を書き、性質を検証し、結果を解釈できる。' \
  | bun <skill-directory>/tools/goal.ts set quint '技術'
```

`Goal` ノートタイプが存在しなければ、初回の承認済みGoal作成時に `Goal / Definition` の最小構成で作る。既に `Goal` というノートタイプがあり、フィールド構成が異なる場合は勝手に変更しない。

学習途中で目的地を変える場合も、新しいDefinitionを提示して明示的承認を得てから更新する。

## Ankiを読む

スキルディレクトリのCLIを使う。

```sh
bun <skill-directory>/tools/anki-connect.ts snapshot
bun <skill-directory>/tools/anki-connect.ts snapshot quint
bun <skill-directory>/tools/anki-connect.ts model-fields 'Basic'
```

`snapshot` はデッキ、タグ、ノートタイプを返す。タグを一つ渡すと、そのタグを正確に持つノートとカードも返す。Ankiの階層タグ検索の詳細はCLI内部で扱う。

接続先の既定値は `http://127.0.0.1:8765`。コンテナ内からホスト側のAnkiへ接続するときは `ANKI_CONNECT_URL=http://host.docker.internal:8765` を使う。CLIはloopbackと `host.docker.internal` 以外を拒否する。認証情報をリポジトリへ置かない。

AnkiまたはAnkiConnectへ接続できない場合はAnkiを変更せず、接続できなかった事実と確認すべき点を示す。

## デッキとタグ

整理方法はAnki公式の考え方を優先する。

- デッキは、常に別々に学習したい広いカテゴリに使う。
- 細かい分類にはタグやフィールドを使う。
- タグはノートに付く。一つのノートへ複数タグを付けてよい。
- タグのツリーはAnkiの通常機能として使う。

参考:

- https://docs.ankiweb.net/editing.html#using-decks-appropriately
- https://docs.ankiweb.net/editing.html#using-tags
- https://docs.ankiweb.net/searching.html#tags-decks-cards-and-notes

スキル独自のタグ命名規約や分類軸の名前空間は作らない。

既存のデッキ、タグ、学習用ノートタイプを優先する。適切なものが複数あれば候補を示し、人間が決める。適切なタグがなければ候補を示し、人間が選んだ後だけ新しいタグを使う。新しいデッキも、人間が名前を決めた後だけ作成する。

```sh
bun <skill-directory>/tools/anki-connect.ts create-deck '技術'
```

このスキルは既存コレクションの移行、タグの一括整理、既存カードの付け替えを行わない。期待する構造と合わない場合は変更せず、別作業で整えてから再実行する。

## 現在地とロードマップ

Goalと学習ノートに共通して付いている通常タグでAnkiを読む。

```sh
bun <skill-directory>/tools/goal.ts get quint
bun <skill-directory>/tools/anki-connect.ts snapshot quint
```

`snapshot quint` の結果では `Goal` ノートを学習済み知識として数えない。

次の三つから、その場でロードマップを組み立てる。

1. `Goal.Definition`: どこへ向かうか。
2. Ankiの実際の学習カード: 何を既に覆っているか。
3. 一次資料: Goalまでに必要な概念と依存関係は何か。

既存カードの問題、答え、補足から、既に直接問われている知識と前提語彙を読み取る。Goalと一次資料を照らし、まだ覆っていない前提や概念を依存順に並べ、現在地の直後から次のカードを選ぶ。

ロードマップ、現在地、習得率、`next-up`、バッチ番号は保存しない。Definitionまたは実カードが変われば、次回は最新のAnkiから再計算する。

初版では正答率や復習履歴に基づく順番の最適化をしない。

## カードを作る

コード、設定、仕様書、公式文書などの一次資料を直接読む。推測をカードへ書かない。

本編で使う語彙の前提を確認し、必要なら基礎カードを先に作る。枚数指定がなければ一度に10枚を提案し、10枚に満たない方が自然なら無理に埋めない。

学習カードの形式、フィールド名、ノートタイプは固定しない。既存ノートの構成を調べ、それに合わせる。新しい学習用ノートタイプが必要なら人間に確認する。

候補を作る前に、同じタグを持つ既存学習ノートを読み、同じ知識を同じ想起方向で問うカードを重複させない。応用や別状況からの想起がGoalに必要なら、同じ知識を使っていても重複とはみなさない。

## レビューと承認

カード案はAnkiへ追加する前に提示する。デッキ、ノートタイプ、タグも一緒に示す。

[カード設計規約](references/card-design.md)でレビューし、明示的に承認されたカードだけを追加する。一部だけ承認された場合は、そのカードだけ追加する。

Goalの作成またはDefinition変更も、対象と新しいDefinitionが明確な承認を得るまでAnkiを変更しない。

## 追加

承認後も、書き込み直前に同じタグの `snapshot` を取り直す。提案後に同じ知識が追加されていれば書き込まず、最新状態から候補を組み直す。

承認されたカードが最新状態でも重複していないことを確認したら、次の形式を標準入力へ渡す。

```json
{
  "notes": [
    {
      "deckName": "技術",
      "modelName": "Basic",
      "fields": {
        "Front": "条件を満たす仕組みを何と呼ぶ？",
        "Back": "用語"
      },
      "tags": ["quint"]
    }
  ]
}
```

```sh
bun <skill-directory>/tools/anki-connect.ts add < notes.json
```

`notes.json` は受け渡し例であり、永続的な正本ではない。stdinへ直接渡してよい。

CLIは書き込み前の回復、追加前検証、追加後検証を内部で行う。結果は次の意味だけを扱う。

- `success`: 全件の追加をAnki上で確認できた。
- `rejected`: 今回のノートが追加されていない。理由が前回書き込みの回復なら、Ankiを読み直してから候補を組み直す。
- `partial`: 一部だけ追加されたことを確認できた。
- `indeterminate`: 結果を安全に確定できない。同じ追加を繰り返さない。

`success` 以外では追加処理を続けず、確認できた結果を人間へ報告する。

## 扱わないこと

- 正答率や復習履歴を使った順番の最適化
- Goal以外の既存学習カードの自動編集や削除
- 既存コレクションの移行、デッキ整理、タグ整理
- 承認なしのGoal作成、Goal更新、カード追加
- 外部ネットワークからAnkiConnectへ接続する構成
- ローカルファイルへの学習進捗保存
- ロードマップや現在地の永続化
