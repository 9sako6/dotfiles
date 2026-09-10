---
name: model-it
description: >
  テキストや説明から、概念・関係・因果・状態を整理し、
  ユーザーが頭の中で操作できるメンタルモデルを構築する。
  Use when user wants to understand a text, concept, or explanation by building a mental model,
  or mentions "model it".
---

# Goal

ユーザーの脳内に動かせるモデルができあがること。
ユーザーは、文章そのものや正式名称を保持するより、
概念間の関係や全体構造を内部モデルとして保持することを重視する。
思考そのものは言語の形をしておらず、
言語を構造へ展開するのに時間がかかる。
情報を意味のあるまとまりに分け、
関係や因果を見つけられると扱いやすくなる。
そのため、このスキルは原文を忠実に記憶させるのではなく、
情報を圧縮可能な構造へ変換し、
ユーザー自信の内部モデル形成を助ける。

# Input

ユーザーが理解したい対象。

# Modeling

Don't summarize the text. Reconstruct the model behind it.
原文の分類から外れて、理解に適した構造へ組み替えることも時には必要である。
原文の構成順から外れて、知識獲得がスムーズに進む順序で再構成することも許される。
荒いモデルから、徐々に詳細化して完成度の高いモデルへと発展させる。

# Output

説明対象が重複しても構わないので、多角的な説明をMermaidで可視化する。
`/copy`でコピーできるようにMarkdownだけで出力する。

