#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
WRITER="$SCRIPT_DIR/write-event.sh"

tmpdir=$(mktemp -d)
trap 'rm -rf "$tmpdir"' EXIT

fail() {
  printf 'not ok - %s\n' "$1" >&2
  exit 1
}

assert_multiline_field_file() {
  home="$tmpdir/home"
  feedback_file="$tmpdir/feedback.txt"

  mkdir -p "$home"
  cat >"$feedback_file" <<'EOF'
line 1
line 2
EOF

  log_file=$(
    HOME="$home" "$WRITER" \
      --event artifact_change \
      --agent codex:main \
      --skill logging-agents \
      --field phase=testing \
      --field action=verify \
      --field score=0.75 \
      --field-file evaluation_trace="$feedback_file" \
      --field-file feedback_text="$feedback_file" \
      --summary "μf を記録する。" \
      --evidence "理由付き評価を残す。" \
      --next-action "recording に渡す。"
  )

  [ -f "$log_file" ] || fail "writer did not create log file"
  grep -q "^score: '0.75'$" "$log_file" || fail "missing scalar score field"
  grep -q "^evaluation_trace: |-$" "$log_file" || fail "missing evaluation_trace block"
  grep -q "^feedback_text: |-$" "$log_file" || fail "missing feedback_text block"
  grep -q "^  line 1$" "$log_file" || fail "missing first multiline line"
  grep -q "^  line 2$" "$log_file" || fail "missing second multiline line"
}

assert_multiline_field_file

printf 'ok - write-event\n'
