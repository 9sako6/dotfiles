#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
CHECKER="$SCRIPT_DIR/check-logging-event.sh"

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

fail() {
  printf 'not ok - %s\n' "$1" >&2
  exit 1
}

run_hook() {
  home=$1
  input=$2
  stdout=$3
  stderr=$4

  printf '%s' "$input" | HOME="$home" "$CHECKER" >"$stdout" 2>"$stderr"
}

assert_blocked_without_recent_event() {
  name=$1
  input=$2
  home="$tmpdir/$name-home"
  stdout="$tmpdir/$name.stdout"
  stderr="$tmpdir/$name.stderr"

  mkdir -p "$home"

  set +e
  run_hook "$home" "$input" "$stdout" "$stderr"
  code=$?
  set -e
  [ "$code" -eq 2 ] || fail "$name: expected exit 2, got $code"
  [ ! -s "$stdout" ] || fail "$name: expected empty stdout"
  grep -q 'logging-agents skill' "$stderr" || fail "$name: missing skill reminder"
  grep -q 'write-event.sh' "$stderr" || fail "$name: missing write-event.sh reminder"
}

assert_allows_write_event_command() {
  home="$tmpdir/write-event-home"
  stdout="$tmpdir/write-event.stdout"
  stderr="$tmpdir/write-event.stderr"

  mkdir -p "$home"

  run_hook "$home" '{"hook_event_name":"PreToolUse","tool_input":{"command":"sh ~/.claude/skills/logging-agents/write-event.sh"}}' "$stdout" "$stderr"
  code=$?
  [ "$code" -eq 0 ] || fail "write-event bypass: expected exit 0, got $code"
  [ ! -s "$stdout" ] || fail "write-event bypass: expected empty stdout"
  [ ! -s "$stderr" ] || fail "write-event bypass: expected empty stderr"
}

assert_allows_recent_event() {
  home="$tmpdir/recent-event-home"
  stdout="$tmpdir/recent-event.stdout"
  stderr="$tmpdir/recent-event.stderr"

  mkdir -p "$home/.claude/mylogs/logging-agents"
  : > "$home/.claude/mylogs/logging-agents/recent.md"

  run_hook "$home" '{"hook_event_name":"Stop"}' "$stdout" "$stderr"
  code=$?
  [ "$code" -eq 0 ] || fail "recent event: expected exit 0, got $code"
  [ ! -s "$stdout" ] || fail "recent event: expected empty stdout"
  [ ! -s "$stderr" ] || fail "recent event: expected empty stderr"
}

assert_blocked_without_recent_event pre-tool-use '{"hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{"command":"mise run dev:test"}}'
assert_blocked_without_recent_event stop '{"hook_event_name":"Stop"}'
assert_blocked_without_recent_event subagent-stop '{"hook_event_name":"SubagentStop"}'
assert_allows_write_event_command
assert_allows_recent_event

printf 'ok - check-logging-event\n'
