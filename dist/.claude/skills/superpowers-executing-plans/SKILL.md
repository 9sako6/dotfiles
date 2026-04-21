---
name: superpowers-executing-plans
description: 書かれた実装計画があり、レビュー用のチェックポイント付きで別セッションでそれを実行するときに使う
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/executing-plans/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/executing-plans/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# 計画の実行

## 概要

計画を読み込み、批判的にレビューし、すべてのタスクを実行し、完了したら報告する。

**開始時に宣言する:** "I'm using the executing-plans skill to implement this plan."

**注記:** Superpowers は subagent へのアクセスがある環境の方がはるかにうまく機能することを human partner に伝えること。subagent を利用できる platform（Claude Code や Codex など）で実行した方が、作業品質は明確に高くなる。subagent が使えるなら、この skill ではなく `superpowers:subagent-driven-development` を使う。

## プロセス

### Step 1: 計画を読み込んでレビューする
1. plan file を読む
2. 批判的にレビューする - plan についての疑問や懸念を特定する
3. 懸念がある場合: 開始前に human partner に伝える
4. 懸念がない場合: TodoWrite を作成して進む

### Step 2: タスクを実行する

各タスクについて:
1. `in_progress` にする
2. 各 step に厳密に従う（plan は bite-sized step を持つ）
3. 指示どおり verification を実行する
4. `completed` にする

### Step 3: 開発を完了する

すべてのタスクが完了し、検証も済んだら:
- 次を宣言する: "I'm using the finishing-a-development-branch skill to complete this work."
- **必須サブ skill:** `superpowers:finishing-a-development-branch` を使う
- その skill に従って test を検証し、選択肢を提示し、選ばれた手順を実行する

## 立ち止まって助けを求めるタイミング

**次のときは即座に実行を止める:**
- blocker に当たったとき（依存不足、test failure、指示不明）
- plan に、開始できないほど重大な欠落があるとき
- 指示が理解できないとき
- verification が何度も失敗するとき

**推測するより確認を求めること。**

## 以前の step に戻るタイミング

**次の場合は Review（Step 1）に戻る:**
- あなたのフィードバックを踏まえて partner が plan を更新した
- 根本のアプローチを見直す必要がある

**blocker を無理に突破しない** - 止まって確認する。

## 覚えておくこと
- まず批判的に plan をレビューする
- plan の step に厳密に従う
- verification を飛ばさない
- plan が言及している skill は参照する
- 詰まったら止まる。推測しない
- 明示的な user consent なしに main/master branch で実装を始めない

## 統合

**必須の workflow skill:**
- **superpowers:using-git-worktrees** - 必須: 開始前に隔離された workspace をセットアップする
- **superpowers:writing-plans** - この skill が実行する plan を作る
- **superpowers:finishing-a-development-branch** - すべての task の後に開発を完了する
