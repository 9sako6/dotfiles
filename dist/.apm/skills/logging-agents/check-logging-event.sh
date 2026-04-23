#!/bin/bash
# Claude Code hook: logging-agents の event file が直近 THRESHOLD_SECONDS 以内に
# 書かれていなければ logging-agents skill の発動を促してブロックする。
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

file_mtime_epoch() {
  stat -f %m "$1" 2>/dev/null || stat -c %Y "$1" 2>/dev/null
}

# 直近 THRESHOLD_SECONDS 秒以内の event file があるか
recent=""
now=$(date +%s)
for file in "$LOGDIR"/*.md; do
  [ -e "$file" ] || continue
  mtime=$(file_mtime_epoch "$file") || continue
  if [ $((now - mtime)) -le "$THRESHOLD_SECONDS" ]; then
    recent=$file
    break
  fi
done

if [ -z "$recent" ]; then
  printf '%s\n' "logging-agents: 直近${THRESHOLD_SECONDS}秒以内に event file がありません。logging-agents skill を発動し、~/.claude/skills/logging-agents/write-event.sh で summary、evidence、next action を自由記述してから続行してください。" >&2
  exit 2
fi
exit 0
