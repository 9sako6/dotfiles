---
name: superpowers-writing-plans
description: spec または multi-step task の requirements があり、code に触る前に使う
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/writing-plans/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/writing-plans/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Plan を書く

## 概要

engineer がこの codebase について文脈ゼロで、しかも趣味が怪しいと仮定して、包括的な実装 plan を書く。各 task でどの file を触るか、必要な code、test、確認すべき docs、どう test するか、必要なものをすべて文書化する。plan 全体を bite-sized な task に分解して渡す。DRY。YAGNI。TDD。頻繁な commit。

彼らは skilled developer だが、toolchain や problem domain はほとんど知らないと仮定する。良い test design もあまり分かっていないと仮定する。

**開始時に宣言する:** "I'm using the writing-plans skill to create the implementation plan."

**文脈:** これは dedicated worktree（brainstorming skill によって作られたもの）で実行されるべきである。

**plan の保存先:** `docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md`
- （plan の保存場所に関するユーザーの好みがある場合は、このデフォルトを上書きする）

## スコープ確認

spec が複数の独立した subsystem を含んでいるなら、ブレインストーミング段階でサブプロジェクト spec に分解されているべきである。そうなっていないなら、subsystem ごとに plan を分ける提案をする。各 plan は単独で動作・検証可能な software を生むべきである。

## ファイル構成

task を定義する前に、どの file を作る / 変更するのか、そして各 file が何を担当するのかを整理する。ここで分解の判断が固まる。

- 境界が明確で、よく定義されたインターフェイスを持つ単位として設計する。各 file は 1 つの明確な責務を持つべきである。
- 手元の文脈に収まる code の方が推論しやすく、file が責務に集中している方が編集は信頼できる。多くを詰め込んだ large file より、小さく焦点の絞られた file を好む。
- 一緒に変わる file は近くに置く。技術レイヤではなく責務で分ける。
- 既存 codebase では既存パターンに従う。codebase が large file を使っているからといって、一方的に再構成してはならない。ただし、変更対象 file が扱いにくいほど大きいなら、plan に分割を含めるのは妥当である。

この構造が task の分解を導く。各 task は独立に意味を持つ self-contained な変更を生むべきである。

## Bite-Sized な Task 粒度

**各 step は 1 アクション（2〜5 分）:**
- "Write the failing test" - step
- "Run it to make sure it fails" - step
- "Implement the minimal code to make the test pass" - step
- "Run the tests and make sure they pass" - step
- "Commit" - step

## Plan Document Header

**すべての plan はこの header で始まらなければならない:**

```markdown
# [Feature Name] Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** [One sentence describing what this builds]

**Architecture:** [2-3 sentences about approach]

**Tech Stack:** [Key technologies/libraries]

---
```

## Task Structure

````markdown
### Task N: [Component Name]

**Files:**
- Create: `exact/path/to/file.py`
- Modify: `exact/path/to/existing.py:123-145`
- Test: `tests/exact/path/to/test.py`

- [ ] **Step 1: Write the failing test**

```python
def test_specific_behavior():
    result = function(input)
    assert result == expected
```

- [ ] **Step 2: Run test to verify it fails**

Run: `pytest tests/path/test.py::test_name -v`
Expected: FAIL with "function not defined"

- [ ] **Step 3: Write minimal implementation**

```python
def function(input):
    return expected
```

- [ ] **Step 4: Run test to verify it passes**

Run: `pytest tests/path/test.py::test_name -v`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add tests/path/test.py src/path/file.py
git commit -m "feat: add specific feature"
```
````

## プレースホルダ禁止

各 step には、engineer が必要とする実際の内容が必ず入っていなければならない。次のようなものは **plan failure** であり、絶対に書いてはならない:
- "TBD"、"TODO"、"implement later"、"fill in details"
- "Add appropriate error handling" / "add validation" / "handle edge cases"
- "Write tests for the above"（実際の test code なし）
- "Similar to Task N"（code を繰り返し書く。engineer は順不同で読むかもしれない）
- 何をすべきかだけ書いて、どうやるかを示さない step（code step には code block が必須）
- どの task にも定義されていない type、function、method への参照

## 覚えておくこと
- file path は常に正確に
- 各 step で code を変えるなら完全な code を示す
- command は正確に。期待出力も付ける
- DRY、YAGNI、TDD、頻繁な commit

## セルフレビュー

plan を書き終えたら、新鮮な目で spec を見直し、plan を照合する。これは subagent dispatch ではなく、自分で回す checklist である。

**1. Spec coverage:** spec の各節 / 要件をざっと見る。それを実装する task を指させるか。抜けがあれば列挙する。

**2. Placeholder scan:** "No Placeholders" 節で挙げた赤信号パターンがないか plan を探す。見つけたら直す。

**3. Type consistency:** 後段の task で使った type、method signature、property 名が、前段で定義したものと一致しているか。Task 3 では `clearLayers()` だったのに Task 7 では `clearFullLayers()` になっている、のようなものは bug である。

問題を見つけたら inline で直す。再レビューは不要。そのまま直して進める。spec 要件に対して task がなければ task を追加する。

## 実行への引き継ぎ

plan を保存したあと、実行方法を提示する:

**"Plan complete and saved to `docs/superpowers/plans/<filename>.md`. Two execution options:**

**1. Subagent-Driven (recommended)** - task ごとに新しい subagent を dispatch し、task 間で review しながら高速に反復する

**2. Inline Execution** - `executing-plans` を使ってこの session 内で task を実行し、checkpoints 付きで batch 実行する

**Which approach?"**

**Subagent-Driven が選ばれた場合:**
- **必須サブ skill:** `superpowers:subagent-driven-development` を使う
- task ごとに fresh な subagent + 二段階 review

**Inline Execution が選ばれた場合:**
- **必須サブ skill:** `superpowers:executing-plans` を使う
- checkpoints 付きの batch 実行
