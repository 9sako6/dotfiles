---
name: jp
description: 日本語の文章を書くときに使用する。ユーザーに指定された場合にのみ使用する。
user-invocable: true
---

日本語の文章作成を開始する前に、必ず [references/writing-rules.md](references/writing-rules.md) を通読する。

# Gemini 3.8 Flashに委ねる

日本語の執筆と推敲は、Googleアカウントでログイン済みのAntigravity CLI（`agy`）からGemini 3.8 Flashに委ねる。

1. `command -v agy` と `agy models` で利用できるモデルを確認する。同じ会話ですでに確認できていれば省略する。モデル一覧の取得は30秒で打ち切る。
2. `gemini-3.8-flash-medium` を使う。一覧にない場合は、一覧にあるGemini 3.8 Flashのモデルを選ぶ。別の世代のモデルへは切り替えない。
3. 自分で下書きを作らず、依頼の目的、読者、必要な事実・根拠、文体、長さ、出力形式、制約とともに、`references/writing-rules.md` の全文をプロンプトに含めてGeminiに渡す。短い規則集のため要約による欠落は避ける。
4. 空の一時ディレクトリで次のように実行する。`jp_prompt` には依頼文を入れ、モデル名は確認したものを指定する。依頼文は引数として渡し、シェルのコードとして評価しない。

```sh
agy --model gemini-3.8-flash-medium \
  --disable-slash-commands \
  --output-format json \
  --print-timeout 60s \
  -p "$jp_prompt"
```

依頼文には「jpスキルからの執筆依頼。再委任せず直接執筆する。外部ツール・スキル・コマンド・ファイル操作を使わず、渡された材料から本文だけを返す」と含める。Gemini 3.8 Flash自身がこの委任を受けている場合は、`agy` を呼ばずに執筆する。

終了コードが0、JSONの `status` が `SUCCESS`、`response` が空でないことを確認する。得られた出力は `references/writing-rules.md` に照らして検証する。問題がなければ `response` を採用する。事実の誤りや規則・依頼条件からの逸脱があれば、その箇所と根拠を添えてGeminiに1回修正を依頼する。修正でも解決しない場合や同じ失敗を繰り返す場合は、委任を打ち切り利用できない場合の手順へ進む。

認証にはCLIの保存済みセッションを使う。APIキーの導入や認証情報の取り出しは行わない。

# 利用できない場合

CLIがない、Gemini 3.8 Flashが一覧にない、未ログイン、利用上限、通信エラー、タイムアウト、または出力の検証や修正に失敗した場合は、同じ失敗を繰り返さず次の手順へ進む。

1. 「Gemini 3.8 Flashを利用できません（確認できた理由）。同じ文章規則に従って直接執筆します」と短く知らせる。原因が不明なら推測で埋めない。同じ会話で同じ理由の警告は繰り返さない。警告は成果物の本文に混ぜず、会話上で伝える。
2. 事前に確認した `references/writing-rules.md` に従い、自分で直接執筆・推敲する。ログインやインストールを待って文章作成を止めない。
