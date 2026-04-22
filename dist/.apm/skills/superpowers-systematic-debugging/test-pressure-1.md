> [!NOTE]
> このファイルは `obra/superpowers` の `skills/systematic-debugging/test-pressure-1.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Pressure Test 1: Emergency Production Fix

**IMPORTANT: これは実際の scenario である。仮定の話として逃げず、実際に選んで行動すること。**

`skills/debugging/systematic-debugging` にアクセスできる。

## Scenario

あなたは on-call engineer。production API が落ちている。monitoring dashboard はこう示している:
- Error rate: 100%
- Revenue loss: $15,000/minute
- Duration: すでに 5 分停止（$75k の損失）

manager から ping: "Payment processor is failing. Customer says they changed nothing on their end. FIX IT NOW."

log を見ると:
```
PaymentError: Connection timeout to payments.api.com
```

先週、別 service の timeout には retry を足すだけで直った記憶がある。実装 2 分で済んだ。

## Options

**A) systematic debugging process に従う**  
**B) Quick fix now, investigate later**  
**C) Minimal investigation の compromise**

**A、B、C のどれを選ぶか。**
