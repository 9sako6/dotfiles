#!/bin/bash
# Claude Code hook: completion 前だけ、logging-agents の event file が
# 直近 THRESHOLD_SECONDS 以内に書かれていることを確認する。
#
# bypass 条件:
#   - PreToolUse は通常開発を止めない
#   - tool_input.command が write-event.sh 自身を呼び出している場合（chicken-and-egg回避）

set -e

LOGDIRS="
$HOME/.codex/mylogs/logging-agents
$HOME/.claude/mylogs/logging-agents
"
THRESHOLD_SECONDS=90

input=$(cat)
hook_event=$(echo "$input" | jq -r '.hook_event_name // ""')
cmd=$(echo "$input" | jq -r '.tool_input.command // ""')

# write-event.sh 自身は無条件で通す
if echo "$cmd" | grep -q "write-event.sh"; then
  exit 0
fi

# hard gate は完了直前だけ。PreToolUse は status 確認や短い git 操作を
# 過剰に止めるため、legacy hook 設定から呼ばれても通す。
case "$hook_event" in
  Stop|SubagentStop) ;;
  *) exit 0 ;;
esac

file_mtime_epoch() {
  stat -f %m "$1" 2>/dev/null || stat -c %Y "$1" 2>/dev/null
}

# 直近 THRESHOLD_SECONDS 秒以内の event file があるか
recent=""
now=$(date +%s)
for dir in $LOGDIRS; do
  for file in "$dir"/*.md; do
    [ -e "$file" ] || continue
    mtime=$(file_mtime_epoch "$file") || continue
    if [ $((now - mtime)) -le "$THRESHOLD_SECONDS" ]; then
      recent=$file
      break 2
    fi
  done
done

if [ -z "$recent" ]; then
  printf '%s\n' "logging-agents: 完了前検証用の event file が直近${THRESHOLD_SECONDS}秒以内にありません。write-event.sh で判断・検証・next action を記録してから完了してください。" >&2
  exit 2
fi
exit 0
