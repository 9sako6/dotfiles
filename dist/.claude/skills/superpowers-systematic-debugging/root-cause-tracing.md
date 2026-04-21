> [!NOTE]
> このファイルは `obra/superpowers` の `skills/systematic-debugging/root-cause-tracing.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Root Cause Tracing

## 概要

bug はしばしば call stack の深い場所で表面化する（wrong directory での `git init`、間違った location に file が作られる、誤った path で database が開かれる）。error が起きた地点で直したくなるが、それは symptom を扱っているだけである。

**中核原則:** call chain を逆向きに辿り、元の trigger を見つける。fix は source に対して行う。

## 使うタイミング

次のときに使う:
- error が実行の深い場所で起きている（entry point ではない）
- stack trace が長い call chain を示している
- 無効な data がどこで発生したか不明
- どの test / code が問題を起こしているか特定したい

## Tracing Process

1. **Symptom を観察する**
2. **直接原因を見つける**
3. **「これを呼んだのは何か？」と問う**
4. **source に当たるまで上へ辿り続ける**
5. **元の trigger を見つける**

## Stack Trace の追加

手で辿れない場合は instrumentation を足す:

```typescript
async function gitInit(directory: string) {
  const stack = new Error().stack;
  console.error('DEBUG git init:', {
    directory,
    cwd: process.cwd(),
    nodeEnv: process.env.NODE_ENV,
    stack,
  });
}
```

**重要:** test では `logger` ではなく `console.error()` を使うこと。出ないことがあるため。

## 原則

**error が出た場所だけを直してはならない。** 元の trigger を見つけるまで trace を戻す。
