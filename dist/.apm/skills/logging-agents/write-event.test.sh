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

assert_reports_write_failure() {
  home="$tmpdir/unwritable-home"
  log_dir="$home/.codex/mylogs/logging-agents"
  stdout="$tmpdir/write-failure.stdout"
  stderr="$tmpdir/write-failure.stderr"

  mkdir -p "$log_dir"
  chmod 500 "$log_dir"

  set +e
  HOME="$home" "$WRITER" \
    --event artifact_change \
    --agent codex:main \
    --skill logging-agents \
    --summary "失敗を検証する。" \
    --evidence "出力先を書き込み不可にする。" \
    --next-action "非 0 exit を確認する。" \
    >"$stdout" 2>"$stderr"
  code=$?
  set -e

  chmod 700 "$log_dir"

  [ "$code" -ne 0 ] || fail "write failure: expected non-zero exit"
  [ ! -s "$stdout" ] || fail "write failure: expected empty stdout"
  [ -s "$stderr" ] || fail "write failure: expected stderr"
}

assert_multiline_field_file
assert_reports_write_failure

printf 'ok - write-event\n'
