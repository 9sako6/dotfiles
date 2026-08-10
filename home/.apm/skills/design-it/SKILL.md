---
name: design-it
description: ユーザーが本当に欲しい体験を一問ずつ掘り、実装方法に降りる前に体験の核を明らかにする。Use when user wants to get grilled on their design, or mentions "design it".
---

実装方法を決める前に、ユーザーが本当に欲しい体験を明らかにする。機能名や一般的な製品像に引っ張られない。

## 進め方

最初に Purpose と Person & Context を掘る。特に「誰が」「どんな瞬間に」「何が負担で」「終わった直後どうなっていてほしいか」を早くつかむ。

会話中は、あり得る体験仮説を複数持ち、回答を受けるたびに強弱を更新する。数値確率は使わない。次の質問は、重要な競合仮説を最もよく分け、後続の分岐を大きく減らせるものを選ぶ。

質問は必ず一度に一つ。必要なら推奨案と理由を短く添える。codebase、既知情報、既回答から分かることは聞かない。

機能を横展開しない。「できるならこれも」ではなく、目的に必要かで判断する。自動化できることだけでなく、ユーザー自身に残すべきこと、システムが奪わない方がよいことも確認する。

実装案が出たら、それ自体を要件化する前に「それでどんな体験を実現したいか」へ戻す。

## 観点

Apple Human Interface Guidelines の Design Principles を使う: Purpose, Agency, Responsibility, Familiarity, Flexibility, Simplicity, Craft, Delight。

チェックリストとして網羅しない。必要な観点だけ使い、衝突する原則は Purpose に照らして優先順位を決める。

## 終了

Purpose と Person & Context が一文で言え、主要な Experience invariants が矛盾なく説明でき、残りが実装設計に渡せる内容になったら質問を止める。次の質問で Experience Brief がほぼ変わらないなら聞かない。

最後に技術非依存の Experience Brief を出す。Purpose、Person & Context、Desired Experience、Experience invariants、Non-goals など、必要な項目だけ残す。

DB、API、framework などの実装方法は、ユーザー体験へ直接影響する制約でない限り含めない。
