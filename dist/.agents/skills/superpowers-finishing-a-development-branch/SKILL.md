---
name: superpowers-finishing-a-development-branch
description: 実装が完了し、すべての test が通っていて、その作業をどう統合するか判断する必要があるときに使う。merge、PR、cleanup の選択肢を提示して開発完了を導く
license: MIT
metadata:
  original_author: Jesse Vincent
  original_repository: https://github.com/obra/superpowers
  original_path: skills/finishing-a-development-branch/SKILL.md
  translation: Japanese translation
---

> [!NOTE]
> このファイルは `obra/superpowers` の `skills/finishing-a-development-branch/SKILL.md` を日本語訳したものです。原文の著作権は Jesse Vincent に帰属し、原文は MIT License の下で提供されています。詳細は `THIRD_PARTY_NOTICES.md` を参照してください。

# 開発ブランチを完了させる

## 概要

明確な選択肢を提示し、選ばれた workflow を処理することで、開発作業の完了を導く。

**中核原則:** test を検証する → 選択肢を提示する → 選択を実行する → cleanup する。

**開始時に宣言する:** "I'm using the finishing-a-development-branch skill to complete this work."

## プロセス

### Step 1: Test を検証する

**選択肢を提示する前に、test が通ることを確認する:**

```bash
# Run project's test suite
npm test / cargo test / pytest / go test ./...
```

**test が失敗した場合:**
```
Tests failing (<N> failures). Must fix before completing:

[Show failures]

Cannot proceed with merge/PR until tests pass.
```

止まる。Step 2 へ進まない。

**test が通った場合:** Step 2 に進む。

### Step 2: Base Branch を決める

```bash
# Try common base branches
git merge-base HEAD main 2>/dev/null || git merge-base HEAD master 2>/dev/null
```

またはこう聞く: "This branch split from main - is that correct?"

### Step 3: 選択肢を提示する

次の 4 つの選択肢を**正確に**提示する:

```
Implementation complete. What would you like to do?

1. Merge back to <base-branch> locally
2. Push and create a Pull Request
3. Keep the branch as-is (I'll handle it later)
4. Discard this work

Which option?
```

**説明を追加してはならない** - 選択肢は簡潔に保つ。

### Step 4: 選択を実行する

#### Option 1: ローカルで Merge する

```bash
# Switch to base branch
git checkout <base-branch>

# Pull latest
git pull

# Merge feature branch
git merge <feature-branch>

# Verify tests on merged result
<test command>

# If tests pass
git branch -d <feature-branch>
```

その後: worktree を cleanup する（Step 5）

#### Option 2: Push して PR を作る

```bash
# Push branch
git push -u origin <feature-branch>

# Create PR
gh pr create --title "<title>" --body "$(cat <<'EOF'
## Summary
<2-3 bullets of what changed>

## Test Plan
- [ ] <verification steps>
EOF
)"
```

その後: worktree を cleanup する（Step 5）

#### Option 3: そのまま保持する

報告する: "Keeping branch <name>. Worktree preserved at <path>."

**worktree を cleanup してはならない。**

#### Option 4: 破棄する

**最初に確認する:**
```
This will permanently delete:
- Branch <name>
- All commits: <commit-list>
- Worktree at <path>

Type 'discard' to confirm.
```

完全一致の確認を待つ。

確認されたら:
```bash
git checkout <base-branch>
git branch -D <feature-branch>
```

その後: worktree を cleanup する（Step 5）

### Step 5: Worktree を Cleanup する

**Options 1, 2, 4 の場合:**

worktree 中か確認する:
```bash
git worktree list | grep $(git branch --show-current)
```

該当するなら:
```bash
git worktree remove <worktree-path>
```

**Option 3 の場合:** worktree は保持する。

## Quick Reference

| Option | Merge | Push | Keep Worktree | Cleanup Branch |
|--------|-------|------|---------------|----------------|
| 1. Merge locally | ✓ | - | - | ✓ |
| 2. Create PR | - | ✓ | ✓ | - |
| 3. Keep as-is | - | - | ✓ | - |
| 4. Discard | - | - | - | ✓ (force) |

## よくある失敗

**test verification を飛ばす**
- **問題:** 壊れた code を merge したり、失敗する PR を作ったりする
- **修正:** 選択肢を出す前に必ず test を検証する

**open-ended な質問**
- **問題:** "What should I do next?" → 曖昧
- **修正:** 4 つの構造化された選択肢を正確に提示する

**worktree の自動 cleanup**
- **問題:** 後で必要かもしれないのに worktree を消してしまう（Option 2, 3）
- **修正:** cleanup するのは Options 1 と 4 のときだけ

**discard の確認がない**
- **問題:** 作業を誤って削除する
- **修正:** `"discard"` の入力確認を必須にする

## Red Flags

**絶対にしてはならない:**
- failing test のまま進む
- merge 結果で test を確認せずに merge する
- 確認なしで作業を削除する
- 明示的な依頼なしに force-push する

**必ずすること:**
- 選択肢を出す前に test を検証する
- 正確に 4 つの選択肢を提示する
- Option 4 では入力確認を取る
- Options 1 と 4 でのみ worktree を cleanup する

## 統合

**呼び出し元:**
- **subagent-driven-development**（Step 7） - すべての task 完了後
- **executing-plans**（Step 5） - すべての batch 完了後

**組み合わせるもの:**
- **using-git-worktrees** - その skill が作った worktree を cleanup する
