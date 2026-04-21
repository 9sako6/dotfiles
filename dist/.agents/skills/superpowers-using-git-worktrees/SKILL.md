---
name: superpowers-using-git-worktrees
description: 現在の workspace から隔離が必要な feature 作業を始めるとき、または実装計画を実行する前に使う。賢いディレクトリ選択と安全確認を伴う isolated git worktree を作成する
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/using-git-worktrees/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/using-git-worktrees/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# Git Worktree を使う

## 概要

Git worktree は同じ repository を共有する isolated workspace を作り、branch を切り替えることなく複数 branch の作業を同時に可能にする。

**中核原則:** 系統的な directory selection + safety verification = 信頼できる isolation。

**開始時に宣言する:** "I'm using the using-git-worktrees skill to set up an isolated workspace."

## ディレクトリ選択プロセス

次の優先順に従う:

### 1. 既存ディレクトリを確認する

```bash
# Check in priority order
ls -d .worktrees 2>/dev/null     # Preferred (hidden)
ls -d worktrees 2>/dev/null      # Alternative
```

**見つかった場合:** その directory を使う。両方あるなら `.worktrees` が優先。

### 2. CLAUDE.md を確認する

```bash
grep -i "worktree.*director" CLAUDE.md 2>/dev/null
```

**好みが指定されている場合:** 聞き返さずそのまま使う。

### 3. ユーザーに聞く

directory もなく、CLAUDE.md の指定もない場合:

```
No worktree directory found. Where should I create worktrees?

1. .worktrees/ (project-local, hidden)
2. ~/.config/superpowers/worktrees/<project-name>/ (global location)

Which would you prefer?
```

## 安全性の確認

### Project-local directory（`.worktrees` または `worktrees`）の場合

**worktree を作る前に、その directory が ignore されていることを必ず確認する:**

```bash
# Check if directory is ignored (respects local, global, and system gitignore)
git check-ignore -q .worktrees 2>/dev/null || git check-ignore -q worktrees 2>/dev/null
```

**ignore されていない場合:**

Jesse のルール "Fix broken things immediately" に従って:
1. `.gitignore` に適切な行を加える
2. その変更を commit する
3. worktree 作成を続行する

**なぜ重要か:** worktree の内容を repository に誤って commit するのを防ぐため。

### Global directory（`~/.config/superpowers/worktrees`）の場合

project の外なので `.gitignore` 確認は不要。

## 作成手順

### 1. Project Name を検出する

```bash
project=$(basename "$(git rev-parse --show-toplevel)")
```

### 2. Worktree を作る

```bash
# Determine full path
case $LOCATION in
  .worktrees|worktrees)
    path="$LOCATION/$BRANCH_NAME"
    ;;
  ~/.config/superpowers/worktrees/*)
    path="~/.config/superpowers/worktrees/$project/$BRANCH_NAME"
    ;;
esac

# Create worktree with new branch
git worktree add "$path" -b "$BRANCH_NAME"
cd "$path"
```

### 3. Project Setup を実行する

自動判定して適切な setup を走らせる:

```bash
# Node.js
if [ -f package.json ]; then npm install; fi

# Rust
if [ -f Cargo.toml ]; then cargo build; fi

# Python
if [ -f requirements.txt ]; then pip install -r requirements.txt; fi
if [ -f pyproject.toml ]; then poetry install; fi

# Go
if [ -f go.mod ]; then go mod download; fi
```

### 4. Clean Baseline を検証する

worktree が clean に始まることを確認するため test を走らせる:

```bash
# Examples - use project-appropriate command
npm test
cargo test
pytest
go test ./...
```

**test が失敗した場合:** failure を報告し、続けるか調査するかを聞く。

**test が通った場合:** 準備完了を報告する。

### 5. 場所を報告する

```
Worktree ready at <full-path>
Tests passing (<N> tests, 0 failures)
Ready to implement <feature-name>
```

## Quick Reference

| Situation | Action |
|-----------|--------|
| `.worktrees/` exists | Use it (verify ignored) |
| `worktrees/` exists | Use it (verify ignored) |
| Both exist | Use `.worktrees/` |
| Neither exists | Check CLAUDE.md → Ask user |
| Directory not ignored | Add to .gitignore + commit |
| Tests fail during baseline | Report failures + ask |
| No package.json/Cargo.toml | Skip dependency install |

## よくある失敗

### ignore 確認を飛ばす

- **問題:** worktree 内容が track され、git status を汚染する
- **修正:** project-local worktree を作る前に必ず `git check-ignore` を使う

### directory location を決め打ちする

- **問題:** 一貫性を壊し、project convention に反する
- **修正:** 優先順に従う: existing > CLAUDE.md > ask

### failing test のまま進む

- **問題:** 新しい bug と既存問題を区別できない
- **修正:** failure を報告し、続ける明示的許可を得る

### setup command を決め打ちする

- **問題:** 違う tool を使う project で壊れる
- **修正:** project file（package.json など）から自動判定する

## Workflow 例

```
You: I'm using the using-git-worktrees skill to set up an isolated workspace.

[Check .worktrees/ - exists]
[Verify ignored - git check-ignore confirms .worktrees/ is ignored]
[Create worktree: git worktree add .worktrees/auth -b feature/auth]
[Run npm install]
[Run npm test - 47 passing]

Worktree ready at /Users/jesse/myproject/.worktrees/auth
Tests passing (47 tests, 0 failures)
Ready to implement auth feature
```

## Red Flags

**絶対にしてはならない:**
- ignore 確認なしで worktree を作る（project-local）
- baseline test verification を飛ばす
- failing test のまま無断で進む
- 曖昧なのに directory location を決めつける
- CLAUDE.md 確認を飛ばす

**必ずすること:**
- directory の優先順に従う: existing > CLAUDE.md > ask
- project-local の場合、directory が ignore されていることを確認する
- project setup を自動判定して実行する
- clean な test baseline を検証する

## 統合

**呼び出し元:**
- **brainstorming**（Phase 4） - 設計が承認され、実装に進む場合に必須
- **subagent-driven-development** - task 実行前に必須
- **executing-plans** - task 実行前に必須
- isolated workspace を必要とする他の skill

**組み合わせるもの:**
- **finishing-a-development-branch** - 作業完了後の cleanup に必須
