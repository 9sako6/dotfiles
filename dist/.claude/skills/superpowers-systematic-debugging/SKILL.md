---
name: superpowers-systematic-debugging
description: bug、test failure、予期しない挙動などの技術的問題に遭遇したとき、fix を提案する前に使う
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/systematic-debugging/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/systematic-debugging/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Systematic Debugging

## 概要

行き当たりばったりの fix は時間を浪費し、新しい bug を作る。手早い patch は根本問題を覆い隠す。

**中核原則:** fix を試みる前に、常に root cause を見つける。症状への fix は失敗である。

**このプロセスの字面に違反することは、debugging の精神に違反することである。**

## 鉄則

```
NO FIXES WITHOUT ROOT CAUSE INVESTIGATION FIRST
```

Phase 1 を終えていないなら、fix を提案してはならない。

## 使うタイミング

あらゆる技術的問題で使う:
- test failure
- production bug
- 予期しない挙動
- performance 問題
- build failure
- integration 問題

**特に次のときに使う:**
- 時間的プレッシャーがあるとき（緊急時ほど推測に流れやすい）
- "just one quick fix" が明らかに見えるとき
- すでに複数の fix を試しているとき
- 前の fix が効かなかったとき
- 問題を十分理解していないとき

**次のときでも飛ばさない:**
- 問題が単純に見えるとき（単純な bug にも root cause はある）
- 急いでいるとき（急ぐほど手戻りが増える）
- manager が「今すぐ直して」と言っているとき（systematic な方が結果的に速い）

## 4 つの Phase

次の phase へ進む前に、各 phase を必ず完了しなければならない。

### Phase 1: Root Cause Investigation

**あらゆる fix を試す前に:**

1. **Error Message を丁寧に読む**
   - error や warning を読み飛ばさない
   - そこに正解がそのまま含まれていることが多い
   - stack trace を最後まで読む
   - line number、file path、error code を控える

2. **一貫して再現する**
   - 信頼して再現できるか？
   - 正確な再現手順は何か？
   - 毎回起きるか？
   - 再現できないなら → データを増やす。推測しない

3. **最近の変更を確認する**
   - 何が変わって、これを引き起こしうるか？
   - git diff、最近の commit
   - 新しい dependency、config change
   - 環境差分

4. **複数コンポーネント系では証拠を集める**

   **システムが複数コンポーネントを持つ場合（CI → build → signing、API → service → database）:**

   **fix を提案する前に、診断用 instrumentation を足す:**
   ```
   For EACH component boundary:
     - component に入る data を log する
     - component から出る data を log する
     - environment/config の伝播を確認する
     - 各 layer の state を確認する

   まず 1 回走らせて、どこで壊れているかを示す証拠を集める
   THEN その証拠を分析して failing component を特定する
   THEN その特定 component を調べる
   ```

5. **Data Flow を辿る**

   **error が call stack の深いところで起きている場合:**

   完全な backward tracing 手法は、この directory の `root-cause-tracing.md` を参照する。

   **短縮版:**
   - 壊れた値はどこで生まれたか？
   - それにその悪い値を渡した caller は何か？
   - source に辿り着くまで上へ遡る
   - 症状ではなく source で直す

### Phase 2: Pattern Analysis

**fix の前に pattern を見つける:**

1. **動いている例を探す**
   - 同じ codebase で似た working code を見つける
   - 壊れているものに似ていて、動いているものは何か？

2. **reference と比較する**
   - pattern を実装するなら、reference implementation を**最後まで**読む
   - skim しない。全行読む
   - 当てはめる前に pattern 全体を理解する

3. **差分を特定する**
   - working と broken の違いは何か？
   - 小さく見えるものも含め、違いをすべて列挙する
   - "that can't matter" と決めつけない

4. **dependency を理解する**
   - これは他にどの component を必要とするか？
   - 必要な setting、config、environment は？
   - どんな前提に依存しているか？

### Phase 3: Hypothesis and Testing

**科学的方法:**

1. **仮説を 1 つだけ立てる**
   - 明確に述べる: "I think X is the root cause because Y"
   - 書き出す
   - 曖昧ではなく具体的に

2. **最小限で test する**
   - 仮説を test するための、可能な限り最小の変更を行う
   - 一度に 1 変数
   - 複数のことを一度に直さない

3. **続ける前に検証する**
   - 効いたか？ Yes → Phase 4
   - 効かなかったか？ → **新しい**仮説を立てる
   - その上に fix を積み増してはならない

4. **分からないとき**
   - "I don't understand X" と言う
   - 分かったふりをしない
   - 助けを求める
   - さらに調べる

### Phase 4: Implementation

**症状ではなく root cause を直す:**

1. **失敗する test case を作る**
   - 可能な限り単純な再現
   - 可能なら automated test
   - framework がなければ one-off test script
   - fix 前に**必須**
   - ちゃんと failing test を書くには `superpowers:test-driven-development` skill を使う

2. **単一の fix を実装する**
   - 特定した root cause に対処する
   - 一度に 1 変更
   - 「ついでに」改善しない
   - refactoring を抱き合わせない

3. **fix を検証する**
   - 今は test が通るか？
   - 他の test は壊れていないか？
   - 問題は本当に解消したか？

4. **fix が効かなかった場合**
   - STOP
   - 数える: 何回 fix を試したか？
   - 3 未満なら: Phase 1 に戻り、新しい情報を使って再分析する
   - **3 以上なら: STOP して architecture を疑う（次の step へ）**
   - architecture を議論せずに Fix #4 を試してはならない

5. **3 回以上 fix が失敗したら: Architecture を疑う**

   **architecture 問題を示す pattern:**
   - 各 fix が、別の場所の shared state / coupling / 問題を新たに露出させる
   - fix の実装に「massive refactoring」が必要になる
   - 各 fix が別の場所に新しい symptom を作る

   **基本前提を疑って止まる:**
   - この pattern 自体は妥当か？
   - "sticking with it through sheer inertia" になっていないか？
   - 症状を直し続けるより architecture を変えるべきではないか？

   **これ以上 fix を試す前に human partner と議論する**

   これは failed hypothesis ではない。間違った architecture である。

## Red Flags - 止まってプロセスに戻る

次のような思考が出たら:
- "Quick fix for now, investigate later"
- "Just try changing X and see if it works"
- "Add multiple changes, run tests"
- "Skip the test, I'll manually verify"
- "It's probably X, let me fix that"
- "I don't fully understand but this might work"
- "Pattern says X but I'll adapt it differently"
- "Here are the main problems: [lists fixes without investigation]"
- data flow を辿る前に solution を提案している
- **"One more fix attempt"（すでに 2 回以上試しているのに）**
- **各 fix が別の場所の新しい問題を露出している**

**これらはすべて STOP を意味する。Phase 1 に戻る。**

**3 回以上 fix が失敗した場合:** architecture を疑う（Phase 4.5 参照）

## human partner からの「やり方が間違っている」サイン

**次の差し戻しに注意する:**
- "Is that not happening?" - 検証せずに仮定した
- "Will it show us...?" - 証拠集めを入れるべきだった
- "Stop guessing" - 理解せずに fix を提案している
- "Ultrathink this" - 症状ではなく前提を疑うべき
- "We're stuck?"（苛立ちを伴う）- アプローチが機能していない

**これを見たら:** STOP。Phase 1 に戻る。

## よくある合理化

| Excuse | Reality |
|--------|---------|
| "Issue is simple, don't need process" | 単純な問題にも root cause はある。単純な bug なら process も速い。 |
| "Emergency, no time for process" | systematic debugging の方が、guess-and-check より速い。 |
| "Just try this first, then investigate" | 最初の fix が pattern を決める。最初から正しくやる。 |
| "I'll write test after confirming fix works" | test なしの fix は残らない。先に test を書くことで証明できる。 |
| "Multiple fixes at once saves time" | 何が効いたのか分からない。新しい bug を作る。 |
| "Reference too long, I'll adapt the pattern" | 理解が半端だと bug は避けられない。最後まで読む。 |
| "I see the problem, let me fix it" | symptom が見えたこと ≠ root cause を理解したこと。 |
| "One more fix attempt"（2 回以上失敗後） | 3 回以上失敗 = architecture 問題。pattern を疑え。 |

## Quick Reference

| Phase | Key Activities | Success Criteria |
|-------|---------------|------------------|
| **1. Root Cause** | error を読む、再現、変更確認、証拠収集 | WHAT と WHY を理解している |
| **2. Pattern** | working example を探す、比較する | 差分を特定している |
| **3. Hypothesis** | theory を立て、最小限に test する | 確認されたか、新しい仮説が得られた |
| **4. Implementation** | test を作る、fix する、検証する | bug 解消、test pass |

## プロセスの結果「root cause なし」と見えるとき

systematic な調査の結果、本当に環境依存・タイミング依存・外部要因だと分かった場合:

1. その時点でプロセスは完了している
2. 何を調査したか文書化する
3. 適切な handling（retry、timeout、error message）を実装する
4. 次回調査のため monitoring / logging を追加する

**ただし:** 「root cause がない」ように見えるケースの 95% は、調査不足である。

## 補助テクニック

次のテクニックは systematic debugging の一部であり、この directory にある:

- **`root-cause-tracing.md`** - bug を call stack を遡って tracing し、元の trigger を見つける
- **`defense-in-depth.md`** - root cause を見つけた後、複数 layer に validation を足す
- **`condition-based-waiting.md`** - 任意の timeout を condition polling に置き換える

**関連 skill:**
- **superpowers:test-driven-development** - failing test case を作るため（Phase 4, Step 1）
- **superpowers:verification-before-completion** - 成功を主張する前に fix が効いたか検証する

## 現実の効果

debugging session より:
- Systematic approach: 修正まで 15〜30 分
- Random fixes approach: 2〜3 時間の空転
- 初回 fix 成功率: 95% vs 40%
- 新しい bug の混入: ほぼゼロ vs よく起きる
