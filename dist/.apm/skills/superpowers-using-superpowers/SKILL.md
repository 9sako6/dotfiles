---
name: superpowers-using-superpowers
description: 会話を始めるときに使う。clarifying question を含むあらゆる応答の前に Skill tool を起動し、skill の見つけ方と使い方を定める
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/using-superpowers/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/using-superpowers/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

<SUBAGENT-STOP>
subagent として特定 task を実行するために dispatch されたなら、この skill は飛ばす。
</SUBAGENT-STOP>

<EXTREMELY-IMPORTANT>
skill が適用される可能性が 1% でもあると思うなら、必ずその skill を起動しなければならない。

skill が task に適用されるなら、選択の余地はない。必ず使うこと。

これは交渉の余地がない。任意ではない。理屈を付けて回避してはならない。
</EXTREMELY-IMPORTANT>

## 指示の優先順位

Superpowers skill は default system prompt の挙動を上書きするが、**ユーザー指示が常に最優先**である:

1. **ユーザーの明示的指示**（CLAUDE.md、GEMINI.md、AGENTS.md、直接の依頼）— 最優先
2. **Superpowers skill** — 衝突時は default system behavior を上書きする
3. **Default system prompt** — 最下位

CLAUDE.md、GEMINI.md、AGENTS.md が「TDD を使うな」と言い、skill が「常に TDD」と言っていても、ユーザー指示に従う。主導権はユーザーにある。

## Skill へのアクセス方法

**Claude Code では:** `Skill` tool を使う。skill を起動すると、その内容が読み込まれて提示されるので、そのまま従う。skill file に Read tool を使ってはならない。

**Copilot CLI では:** `skill` tool を使う。skill は installed plugin から自動 discovery される。`skill` tool は Claude Code の `Skill` tool と同じように動く。

**Gemini CLI では:** skill は `activate_skill` tool で有効化する。Gemini は session 開始時に skill metadata を読み込み、必要時に完全な内容を有効化する。

**その他の環境では:** skill の読み込み方法は、その platform の documentation を確認する。

## Platform Adaptation

skill は Claude Code の tool 名を使う。非 CC platform では、対応する tool は `references/copilot-tools.md`（Copilot CLI）、`references/codex-tools.md`（Codex）を参照する。Gemini CLI user には tool mapping が GEMINI.md 経由で自動ロードされる。

# Skill の使い方

## ルール

**関係しそうな skill やリクエストされた skill は、応答や行動の前に必ず起動すること。** 適用の可能性が 1% でもあるなら、その skill を起動して確認する。起動してみて状況に合わないと分かったなら、使わなくてよい。

```dot
digraph skill_flow {
    "User message received" [shape=doublecircle];
    "About to EnterPlanMode?" [shape=doublecircle];
    "Already brainstormed?" [shape=diamond];
    "Invoke brainstorming skill" [shape=box];
    "Might any skill apply?" [shape=diamond];
    "Invoke Skill tool" [shape=box];
    "Announce: 'Using [skill] to [purpose]'" [shape=box];
    "Has checklist?" [shape=diamond];
    "Create TodoWrite todo per item" [shape=box];
    "Follow skill exactly" [shape=box];
    "Respond (including clarifications)" [shape=doublecircle];

    "About to EnterPlanMode?" -> "Already brainstormed?";
    "Already brainstormed?" -> "Invoke brainstorming skill" [label="no"];
    "Already brainstormed?" -> "Might any skill apply?" [label="yes"];
    "Invoke brainstorming skill" -> "Might any skill apply?";

    "User message received" -> "Might any skill apply?";
    "Might any skill apply?" -> "Invoke Skill tool" [label="yes, even 1%"];
    "Might any skill apply?" -> "Respond (including clarifications)" [label="definitely not"];
    "Invoke Skill tool" -> "Announce: 'Using [skill] to [purpose]'";
    "Announce: 'Using [skill] to [purpose]'" -> "Has checklist?";
    "Has checklist?" -> "Create TodoWrite todo per item" [label="yes"];
    "Has checklist?" -> "Follow skill exactly" [label="no"];
    "Create TodoWrite todo per item" -> "Follow skill exactly";
}
```

## Red Flags

こんな考えが浮かんだら STOP。合理化している:

| Thought | Reality |
|---------|---------|
| "This is just a simple question" | 質問も task である。skill を確認する |
| "I need more context first" | skill 確認は clarifying question より先に来る |
| "Let me explore the codebase first" | どう探索すべきかを skill が教える。先に確認する |
| "I can check git/files quickly" | file には会話文脈がない。skill を確認する |
| "Let me gather information first" | 情報収集のやり方も skill が教える |
| "This doesn't need a formal skill" | skill があるなら使う |
| "I remember this skill" | skill は進化する。現在版を読む |
| "This doesn't count as a task" | 行動 = task。skill を確認する |
| "The skill is overkill" | 単純なことほど複雑化しうる。使う |
| "I'll just do this one thing first" | 何かする前に確認する |
| "This feels productive" | 規律のない行動は時間を無駄にする。skill はそれを防ぐ |
| "I know what that means" | 概念を知っていること ≠ skill を使ったこと。起動する |

## Skill の優先順位

複数 skill が適用できる場合はこの順:

1. **Process skill を先に**（brainstorming、debugging）- どう進めるかを決める
2. **Implementation skill を後に**（frontend-design、mcp-builder）- 実行を導く

"Let's build X" → まず brainstorming、次に implementation skill  
"Fix this bug" → まず debugging、次に domain-specific skill

## Skill の種類

**Rigid**（TDD、debugging）: 厳密に従う。規律を都合よく緩めない。

**Flexible**（pattern）: 原則を文脈に合わせて適用する。

どちらかは skill 自体が示す。

## ユーザー指示

指示は WHAT を言うのであって、HOW ではない。"Add X" や "Fix Y" は workflow を飛ばしてよいという意味ではない。
