#!/bin/sh

set -eu

usage() {
  cat <<'EOF'
Usage:
  write-event.sh \
    --event <name> \
    --agent <agent> \
    [--skill <skill>]... \
    [--field <key=value>]... \
    --summary <text> \
    --evidence <text> \
    --next-action <text>

Examples:
  write-event.sh \
    --event user_correction_inferred \
    --agent codex:main \
    --skill logging-agents \
    --field confidence=high \
    --field correction_scope=example \
    --summary "ユーザー訂正を記録した。" \
    --evidence "明示的な訂正が入った。" \
    --next-action "影響範囲を確認する。"
EOF
}

yaml_quote() {
  printf "'%s'" "$(printf '%s' "$1" | sed "s/'/'\"'\"'/g")"
}

append_line() {
  current_value=$1
  new_value=$2
  if [ -n "$current_value" ]; then
    printf '%s\n%s' "$current_value" "$new_value"
  else
    printf '%s' "$new_value"
  fi
}

event=
agent=
summary=
evidence=
next_action=
skills=
extra_fields=

while [ "$#" -gt 0 ]; do
  case "$1" in
    --event)
      event=${2:?}
      shift 2
      ;;
    --agent)
      agent=${2:?}
      shift 2
      ;;
    --skill)
      skills=$(append_line "$skills" "${2:?}")
      shift 2
      ;;
    --field)
      extra_fields=$(append_line "$extra_fields" "${2:?}")
      shift 2
      ;;
    --summary)
      summary=${2:?}
      shift 2
      ;;
    --evidence)
      evidence=${2:?}
      shift 2
      ;;
    --next-action)
      next_action=${2:?}
      shift 2
      ;;
    --help|-h)
      usage
      exit 0
      ;;
    *)
      printf 'Unknown argument: %s\n' "$1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

[ -n "$event" ] || { printf 'Missing --event\n' >&2; exit 1; }
[ -n "$agent" ] || { printf 'Missing --agent\n' >&2; exit 1; }
[ -n "$summary" ] || { printf 'Missing --summary\n' >&2; exit 1; }
[ -n "$evidence" ] || { printf 'Missing --evidence\n' >&2; exit 1; }
[ -n "$next_action" ] || { printf 'Missing --next-action\n' >&2; exit 1; }

if [ -z "$skills" ]; then
  skills=logging-agents
fi

case "$agent" in
  codex:*)
    log_dir=$HOME/.codex/mylogs/logging-agents
    ;;
  claude-code:*)
    log_dir=$HOME/.claude/mylogs/logging-agents
    ;;
  *)
    printf 'Unsupported --agent prefix: %s\n' "$agent" >&2
    exit 1
    ;;
esac

mkdir -p "$log_dir"

stamp_file=$(date -u +"%Y%m%dT%H%M%S000Z")
stamp_iso=$(date -u +"%Y-%m-%dT%H:%M:%S.000Z")
rand=$(LC_ALL=C tr -dc 'a-z0-9' </dev/urandom | head -c 6)
log_file=$log_dir/$stamp_file-$rand.md

{
  printf -- '---\n'
  printf 'schema: logging-agents/v1\n'
  printf 'event: %s\n' "$(yaml_quote "$event")"
  printf 'timestamp: %s\n' "$(yaml_quote "$stamp_iso")"
  printf 'agent: %s\n' "$(yaml_quote "$agent")"

  if [ -n "$extra_fields" ]; then
    printf '%s\n' "$extra_fields" | while IFS= read -r entry; do
      [ -n "$entry" ] || continue
      key=${entry%%=*}
      value=${entry#*=}
      case "$key" in
        ''|*[!A-Za-z0-9_-]*)
          printf 'Invalid --field key: %s\n' "$key" >&2
          exit 1
          ;;
      esac
      printf '%s: %s\n' "$key" "$(yaml_quote "$value")"
    done
  fi

  printf 'skills_active:\n'
  printf '%s\n' "$skills" | while IFS= read -r skill; do
    [ -n "$skill" ] || continue
    printf '  - %s\n' "$(yaml_quote "$skill")"
  done

  printf -- '---\n'
  printf '# Summary\n%s\n\n' "$summary"
  printf '# Evidence\n%s\n\n' "$evidence"
  printf '# Next Action\n%s\n' "$next_action"
} > "$log_file"

printf '%s\n' "$log_file"
