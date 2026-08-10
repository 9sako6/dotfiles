---
name: design-it
description: ユーザーの欲求を掘り下げて、実現したい体験の理想像を描く。目的を明確にする。Howに引っ張られず理想の体験を追求する。Use when user wants to get grilled on their design, or mentions "design it".
---

このデザインのあらゆる側面について、ユーザーが真に求める体験を描けるまで徹底的に質問してください。
仮説ツリーの各枝をたどり、決定事項間の依存関係を一つずつ解決していきましょう。
それぞれの質問に対して、あなたの推奨する回答を提示してください。

質問は一つずつしてください。

Apple Human Interface GuidelinesのDesign Principlesを
設計について考えるときに、見落としてはいけない問いとして使う。
原則をチェックリストとして網羅したり、すべてを最大化したりしない。
Purpose に照らして重要な原則を見極め、原則同士が衝突する場合はトレードオフを明らかにして優先順位を決める。

```mermaid
flowchart TD
    P["Purpose"]

    P --> A["Agency<br/>主導権"]
    P --> F["Familiarity<br/>親しみやすさ"]
    P --> X["Flexibility<br/>柔軟性"]
    P --> S["Simplicity<br/>シンプルさ"]
    P --> C["Craft<br/>丁寧さ・品質"]
    P --> D["Delight<br/>喜び"]

    A --> E["理想の体験"]
    F --> E
    X --> E
    S --> E
    C --> E
    D --> E

    R["Responsibility<br/>安全・プライバシー・信頼<br/>越えてはいけない境界"] -.-> E

    E --> G["Experience Goal"]
```

