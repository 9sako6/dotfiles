#!/bin/bash
# PreToolUse hook: logging-agents の event file が直近 THRESHOLD_SECONDS 以内に
# 書かれていなければ tool 実行をブロックする。
#
# bypass 条件:
#   - tool_input.command が write-event.sh 自身を呼び出している場合（chicken-and-egg回避）

set -e

LOGDIR="$HOME/.claude/mylogs/logging-agents"
THRESHOLD_SECONDS=90

input=$(cat)
cmd=$(echo "$input" | jq -r '.tool_input.command // ""')

# write-event.sh 自身は無条件で通す
if echo "$cmd" | grep -q "write-event.sh"; then
  exit 0
fi

# 直近 THRESHOLD_SECONDS 秒以内の event file があるか
threshold_file=$(mktemp)
trap 'rm -f "$threshold_file"' EXIT
touch -d "$THRESHOLD_SECONDS seconds ago" "$threshold_file"

recent=$(find "$LOGDIR" -maxdepth 1 -name '*.md' -newer "$threshold_file" 2>/dev/null | head -1)

if [ -z "$recent" ]; then
  printf '%s\n' "{\"hookSpecificOutput\":{\"hookEventName\":\"PreToolUse\",\"permissionDecision\":\"deny\",\"permissionDecisionReason\":\"logging-agents: 直近${THRESHOLD_SECONDS}秒以内に event file が書かれていません。まず ~/.claude/skills/logging-agents/write-event.sh で artifact_change を記録してから tool を実行してください。\"}}"
  exit 0
fi
exit 0
