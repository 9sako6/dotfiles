> [!NOTE]
> このファイルは `obra/superpowers` の `skills/subagent-driven-development/code-quality-reviewer-prompt.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Code Quality Reviewer Prompt Template

code quality reviewer の subagent を dispatch するときは、この template を使う。

**目的:** 実装がしっかり作られていることを検証する（clean、tested、maintainable）

**spec compliance review が通った後にのみ dispatch すること。**

```
Task tool (superpowers:code-reviewer):
  Use template at requesting-code-review/code-reviewer.md

  WHAT_WAS_IMPLEMENTED: [from implementer's report]
  PLAN_OR_REQUIREMENTS: Task N from [plan-file]
  BASE_SHA: [commit before task]
  HEAD_SHA: [current commit]
  DESCRIPTION: [task summary]
```

**標準的な code quality の観点に加えて、reviewer は次も確認すべきである:**
- 各 file は、よく定義された interface を持つ 1 つの明確な責務を持っているか？
- unit は独立して理解・test できるように分解されているか？
- 実装は plan の file structure に従っているか？
- この実装で、新しく作られた file がすでに大きすぎたり、既存 file が大きく増えたりしていないか？（既存の file size は問題にしない。この変更が何を増やしたかに集中する。）

**code reviewer が返すもの:** Strengths、Issues（Critical/Important/Minor）、Assessment
