#!/bin/sh
set -eu
base=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
uid=$(cat "$base/uid")
printf '%s\n' "$*" >> "$base/calls"
case "$1" in
  lock-probe)
    (sleep 1; touch "$base/child-finished") >/dev/null 2>&1 &
    exit 0
    ;;
  print)
    if [ "$2" = gui/$uid ]; then
      [ ! -f "$base/domain-error" ] || exit 5
      printf 'gui/%s = {\n}\n' "$uid"
      exit 0
    fi
    if [ -f "$base/print-code" ]; then
      cat "$base/print-stdout"
      cat "$base/print-stderr" >&2
      exit "$(cat "$base/print-code")"
    fi
    label=${2#gui/$uid/}
    [ "$2" = "gui/$uid/$label" ] || exit 88
    [ ! -f "$base/print-error" ] || exit 5
    if [ -f "$base/unknown-print" ]; then printf 'unknown\n'; exit 0; fi
    if [ -f "$base/loaded/$label" ]; then
      path=$(cat "$base/loaded/$label")
      printf '%s = {\n\tpath = %s\n\ttype = LaunchAgent\n\tstate = waiting\n}\n' "$2" "$path"
    else
      printf 'Bad request.\nCould not find service "%s" in domain for user gui: %s\n' "$label" "$uid" >&2
      exit 113
    fi
    ;;
  bootstrap)
    if [ -f "$base/prerequisite" ]; then [ -e "$(cat "$base/prerequisite")" ] || exit 89; fi
    [ "$2" = gui/$uid ] || exit 88
    label=${3##*/}; label=${label%.plist}
    [ ! -f "$base/fail-bootstrap" ] && [ ! -f "$base/fail-bootstrap-$label" ] || exit 5
    printf '%s' "$3" > "$base/loaded/$label"
    if [ -f "$base/partial-bootstrap" ]; then exit 5; fi
    if [ -f "$base/vanish-bootstrap" ]; then rm "$base/loaded/$label"; fi
    ;;
  bootout)
    label=${2#gui/$uid/}
    [ "$2" = "gui/$uid/$label" ] || exit 88
    [ ! -f "$base/fail-bootout" ] || exit 5
    rm "$base/loaded/$label"
    if [ -f "$base/partial-bootout" ]; then exit 5; fi
    ;;
  *) exit 88 ;;
esac
